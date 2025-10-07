mod arima;

use crate::Result;
use crate::config;
use crate::logging::*;
use crate::persistence::TimeRange;
use crate::persistence::evaluation_period::{EvaluationPeriod, NewEvaluationPeriod};
use crate::persistence::token_rate::TokenRate;
use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount};
use crate::trade::predict::PredictionService;
use crate::trade::recorder::TradeRecorder;
use crate::trade::swap;
use crate::types::MilliNear;
use crate::wallet::Wallet;
use bigdecimal::BigDecimal;
use chrono::{Duration, NaiveDateTime, Utc};
use futures_util::future::join_all;
use near_sdk::AccountId;
use num_traits::Zero;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Display;
use std::ops::{Add, Div, Mul, Sub};
use std::str::FromStr;
use zaciraci_common::algorithm::{
    portfolio::{PortfolioData, execute_portfolio_optimization},
    types::{PriceHistory, TokenData, TradingAction, WalletInfo},
};

#[derive(Clone)]
pub struct SameBaseTokenRates {
    #[allow(dead_code)]
    pub base: TokenOutAccount,
    #[allow(dead_code)]
    pub quote: TokenInAccount,
    pub points: Vec<Point>,
}

#[derive(Clone)]
pub struct Point {
    pub rate: BigDecimal,
    pub timestamp: NaiveDateTime,
}

pub struct StatsInPeriod<U> {
    pub timestamp: NaiveDateTime,
    pub period: Duration,

    pub start: U,
    pub end: U,
    pub average: U,
    pub max: U,
    pub min: U,
}
pub struct ListStatsInPeriod<U>(Vec<StatsInPeriod<U>>);

pub async fn start() -> Result<()> {
    let log = DEFAULT.new(o!("function" => "trade::start"));

    info!(log, "starting portfolio-based trading strategy");

    // Step 1: 資金準備 (NEAR -> wrap.near)
    let available_funds = prepare_funds().await?;
    info!(log, "Prepared funds"; "available_funds" => available_funds);

    if available_funds == 0 {
        info!(log, "no funds available for trading");
        return Ok(());
    }

    // Step 1.5: 評価期間のチェックと管理
    let (period_id, is_new_period, existing_tokens) =
        manage_evaluation_period(available_funds).await?;
    info!(log, "evaluation period status";
        "period_id" => %period_id,
        "is_new_period" => is_new_period,
        "existing_tokens_count" => existing_tokens.len()
    );

    // Step 2: PredictionServiceの初期化
    let chronos_url =
        std::env::var("CHRONOS_URL").unwrap_or_else(|_| "http://localhost:8000".to_string());

    let prediction_service = PredictionService::new(chronos_url);

    // Step 3: トークン選定 (評価期間に応じて処理を分岐)
    let selected_tokens = if is_new_period {
        // 新規期間: 新しくトークンを選定
        let tokens = select_top_volatility_tokens(&prediction_service).await?;

        // 選定したトークンをデータベースに保存
        if !tokens.is_empty() {
            let token_strs: Vec<String> = tokens.iter().map(|t| t.to_string()).collect();
            match EvaluationPeriod::update_selected_tokens_async(period_id.clone(), token_strs)
                .await
            {
                Ok(_) => {
                    info!(log, "updated selected tokens in database"; "count" => tokens.len());
                }
                Err(e) => {
                    error!(log, "failed to update selected tokens"; "error" => ?e);
                }
            }
        }

        tokens
    } else {
        // 評価期間中: 既存のトークンを使用
        existing_tokens
            .into_iter()
            .filter_map(|s| s.parse::<AccountId>().ok())
            .collect()
    };

    info!(log, "Selected tokens"; "count" => selected_tokens.len(), "is_new_period" => is_new_period);

    if selected_tokens.is_empty() {
        info!(log, "no tokens selected for trading");
        return Ok(());
    }

    // Step 3.5: REF Finance のストレージセットアップを確認・実行
    let client = crate::jsonrpc::new_client();
    let wallet = crate::wallet::new_wallet();

    // トークンを TokenAccount に変換
    let token_accounts: Vec<crate::ref_finance::token_account::TokenAccount> = selected_tokens
        .iter()
        .filter_map(|t| t.as_str().parse().ok())
        .collect();

    info!(log, "ensuring REF Finance storage setup"; "token_count" => token_accounts.len());
    crate::ref_finance::storage::ensure_ref_storage_setup(&client, &wallet, &token_accounts)
        .await?;
    info!(log, "REF Finance storage setup completed");

    // Step 4: ポートフォリオ最適化と実行
    match execute_portfolio_strategy(
        &prediction_service,
        &selected_tokens,
        available_funds,
        is_new_period,
        &client,
        &wallet,
    )
    .await
    {
        Ok(report) => {
            info!(log, "portfolio strategy executed successfully";
                "total_actions" => report.len()
            );

            // 実際の取引実行
            let executed_actions = execute_trading_actions(&report, available_funds).await?;
            info!(log, "trades executed"; "success" => executed_actions.success_count, "failed" => executed_actions.failed_count);

            // Step 5: ハーベスト判定と実行
            check_and_harvest(available_funds).await?;
        }
        Err(e) => {
            error!(log, "failed to execute portfolio strategy"; "error" => ?e);
        }
    }

    info!(log, "success");
    Ok(())
}

/// 資金準備 (NEAR -> wrap.near 変換)
async fn prepare_funds() -> Result<u128> {
    let log = DEFAULT.new(o!("function" => "prepare_funds"));

    // JSONRPCクライアントとウォレットを取得
    let client = crate::jsonrpc::new_client();
    let wallet = crate::wallet::new_wallet();

    // 初期投資額の設定値を取得
    let target_investment = config::get("TRADE_INITIAL_INVESTMENT")
        .ok()
        .and_then(|v| v.parse::<u128>().ok())
        .map(|v| MilliNear::from_near(v).to_yocto())
        .unwrap_or_else(|| MilliNear::from_near(100).to_yocto());

    // 必要な wrap.near 残高として投資額を設定（NEAR -> wrap.near変換）
    // アカウントには10 NEARを残し、それ以外を wrap.near に変換
    let required_balance = target_investment;
    let account_id = wallet.account_id();
    let balance = crate::ref_finance::balances::start(
        &client,
        &wallet,
        &crate::ref_finance::token_account::WNEAR_TOKEN.clone(),
        Some(required_balance),
    )
    .await?;

    // wrap.near の全額が投資可能
    // 設定された投資額と実際の残高の小さい方を使用
    let available_funds = balance.min(target_investment);

    if available_funds == 0 {
        return Err(anyhow::anyhow!(
            "Insufficient wrap.near balance for trading: {} yoctoNEAR",
            balance
        ));
    }

    info!(log, "prepared funds";
        "account" => %account_id,
        "wrap_near_balance" => balance,
        "available_funds" => available_funds
    );

    Ok(available_funds)
}

/// トップボラティリティトークンの選定 (PredictionServiceを使用)
async fn select_top_volatility_tokens(
    prediction_service: &PredictionService,
) -> Result<Vec<AccountId>> {
    let log = DEFAULT.new(o!("function" => "select_top_volatility_tokens"));

    let limit = config::get("TRADE_TOP_TOKENS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);

    // 過去7日間のボラティリティトークンを取得
    let end_date = Utc::now();
    let start_date = end_date - chrono::Duration::days(7);

    match prediction_service
        .get_top_tokens(start_date, end_date, limit, "wrap.near")
        .await
    {
        Ok(top_tokens) => {
            // TopToken を AccountId に変換
            let tokens: anyhow::Result<Vec<AccountId>> = top_tokens
                .into_iter()
                .map(|token| {
                    token
                        .token
                        .parse::<AccountId>()
                        .map_err(|e| anyhow::anyhow!("Failed to parse account ID: {}", e))
                })
                .collect();
            let tokens = tokens?;

            if tokens.is_empty() {
                return Err(anyhow::anyhow!(
                    "No volatility tokens returned from prediction service"
                ));
            }

            info!(log, "selected tokens from prediction service"; "count" => tokens.len());
            Ok(tokens)
        }
        Err(e) => {
            error!(log, "failed to get tokens from prediction service"; "error" => ?e);
            Err(anyhow::anyhow!("Failed to get volatility tokens: {}", e))
        }
    }
}

/// ポートフォリオ戦略の実行
async fn execute_portfolio_strategy<C, W>(
    prediction_service: &PredictionService,
    tokens: &[AccountId],
    available_funds: u128,
    is_new_period: bool,
    client: &C,
    wallet: &W,
) -> Result<Vec<TradingAction>>
where
    C: crate::jsonrpc::ViewContract
        + crate::jsonrpc::AccountInfo
        + crate::jsonrpc::SendTx
        + crate::jsonrpc::GasInfo,
    W: crate::wallet::Wallet,
{
    let log = DEFAULT.new(o!("function" => "execute_portfolio_strategy"));

    // ポートフォリオデータの準備
    let mut token_data = Vec::new();
    let mut predictions = BTreeMap::new();
    let mut historical_prices = Vec::new();

    for token in tokens {
        let token_str = token.to_string();

        // PredictionServiceを使用して価格履歴と予測を取得
        let end_date = Utc::now();
        let start_date = end_date - chrono::Duration::days(30);

        // 価格履歴の取得
        let history = match prediction_service
            .get_price_history(&token_str, "wrap.near", start_date, end_date)
            .await
        {
            Ok(hist) => {
                // PredictionServiceのTokenPriceHistoryをcommonのPriceHistoryに変換
                zaciraci_common::algorithm::types::PriceHistory {
                    token: hist.token,
                    quote_token: hist.quote_token,
                    prices: hist
                        .prices
                        .into_iter()
                        .map(|p| zaciraci_common::algorithm::types::PricePoint {
                            timestamp: p.timestamp,
                            price: p.price,
                            volume: p.volume,
                        })
                        .collect(),
                }
            }
            Err(e) => {
                error!(log, "failed to get price history for token"; "token" => %token, "error" => ?e);
                return Err(anyhow::anyhow!(
                    "Failed to get price history for token {}: {}",
                    token,
                    e
                ));
            }
        };

        // 現在価格を履歴から取得
        let current_price = if let Some(latest_price) = history.prices.last() {
            // BigDecimalをyoctoNEAR (u128)に変換
            let yocto_multiplier = BigDecimal::from(10u128.pow(24));
            let price_yocto = &latest_price.price * &yocto_multiplier;

            debug!(log, "converting price to u128";
                "token" => %token,
                "original_price" => %latest_price.price,
                "price_yocto" => %price_yocto,
                "price_yocto_string" => price_yocto.to_string()
            );

            // BigDecimalをu128に変換（整数部分のみ）
            use num_bigint::ToBigInt;
            let price_bigint = price_yocto.to_bigint().ok_or_else(|| {
                anyhow::anyhow!("Failed to convert BigDecimal to BigInt for token {}", token)
            })?;

            price_bigint.to_string().parse::<u128>().map_err(|e| {
                anyhow::anyhow!(
                    "Failed to parse price for token {}: {} (value: {})",
                    token,
                    e,
                    price_bigint.to_string()
                )
            })?
        } else {
            error!(log, "no price data available for token"; "token" => %token);
            return Err(anyhow::anyhow!(
                "No price data available for token {}",
                token
            ));
        };

        // PredictionServiceの形式に合わせてhistoryを再構築
        let predict_history = crate::trade::predict::TokenPriceHistory {
            token: history.token.clone(),
            quote_token: history.quote_token.clone(),
            prices: history
                .prices
                .iter()
                .map(|p| crate::trade::predict::PricePoint {
                    timestamp: p.timestamp,
                    price: p.price.clone(),
                    volume: p.volume.clone(),
                })
                .collect(),
        };

        // 予測の取得
        let prediction = match prediction_service.predict_price(&predict_history, 24).await {
            Ok(pred) => {
                // 最初の予測値を返却値として使用
                pred.predictions
                    .first()
                    .map(|p| p.price.clone())
                    .ok_or_else(|| {
                        anyhow::anyhow!("No prediction values returned for token {}", token)
                    })?
            }
            Err(e) => {
                error!(log, "failed to get prediction for token"; "token" => %token, "error" => ?e);
                return Err(anyhow::anyhow!(
                    "Failed to get prediction for token {}: {}",
                    token,
                    e
                ));
            }
        };
        // BigDecimal を f64 に変換（外部構造体の制約のため）
        let prediction_f64 = prediction
            .to_string()
            .parse::<f64>()
            .map_err(|e| anyhow::anyhow!("Failed to convert prediction to f64: {}", e))?;
        predictions.insert(token.to_string(), prediction_f64);

        // ボラティリティの計算
        let volatility = calculate_volatility_from_history(&history)?;

        // 流動性スコアの計算（プール情報 + 取引量ベース）
        let liquidity_score =
            calculate_enhanced_liquidity_score(client, &token_str, &history).await;

        historical_prices.push(history);

        // BigDecimalをf64に変換（外部構造体の制約のため）
        let volatility_f64 = volatility
            .to_string()
            .parse::<f64>()
            .map_err(|e| anyhow::anyhow!("Failed to convert volatility to f64: {}", e))?;

        // 市場規模の推定（実際の発行量データを取得）
        let market_cap = estimate_market_cap_async(client, &token_str, current_price).await;

        token_data.push(TokenData {
            symbol: token.to_string(),
            current_price: BigDecimal::from(current_price),
            historical_volatility: volatility_f64,
            liquidity_score: Some(liquidity_score),
            market_cap: Some(market_cap),
            decimals: Some(24),
        });
    }

    let portfolio_data = PortfolioData {
        tokens: token_data,
        predictions,
        historical_prices,
        correlation_matrix: None,
    };

    // BigDecimalで正確に計算してからf64に変換（外部構造体の制約のため）
    let funds_bd = BigDecimal::from(available_funds);
    let total_value_f64 = funds_bd
        .to_string()
        .parse::<f64>()
        .map_err(|e| anyhow::anyhow!("Failed to convert total_value to f64: {}", e))?;

    // 既存ポジションの取得（評価期間中のみ）
    let holdings = if is_new_period {
        // 新規期間: ポジションなし
        info!(log, "new evaluation period, starting with empty holdings");
        BTreeMap::new()
    } else {
        // 評価期間中: 既存のポジションを取得
        info!(
            log,
            "continuing evaluation period, loading current holdings"
        );
        let token_strs: Vec<String> = tokens.iter().map(|t| t.to_string()).collect();
        let current_balances =
            swap::get_current_portfolio_balances(client, wallet, &token_strs).await?;

        // u128をf64に変換（外部構造体の制約のため）
        let mut holdings_f64 = BTreeMap::new();
        for (token, balance) in current_balances {
            if balance > 0 {
                let balance_f64 = BigDecimal::from(balance)
                    .to_string()
                    .parse::<f64>()
                    .map_err(|e| anyhow::anyhow!("Failed to convert balance to f64: {}", e))?;
                holdings_f64.insert(token.clone(), balance_f64);
                info!(log, "loaded existing position"; "token" => token, "balance" => balance);
            }
        }
        holdings_f64
    };

    let wallet_info = WalletInfo {
        holdings,
        total_value: total_value_f64,
        cash_balance: total_value_f64,
    };

    // ポートフォリオ最適化の実行
    let execution_report = execute_portfolio_optimization(
        &wallet_info,
        portfolio_data,
        0.1, // rebalance threshold
    )
    .await?;

    info!(log, "portfolio optimization completed";
        "actions" => execution_report.actions.len()
    );

    Ok(execution_report.actions)
}

/// 取引アクションを実際に実行
async fn execute_trading_actions(
    actions: &[TradingAction],
    _available_funds: u128,
) -> Result<ExecutionSummary> {
    let log = DEFAULT.new(o!("function" => "execute_trading_actions"));

    let mut summary = ExecutionSummary {
        total: actions.len(),
        success_count: 0,
        failed_count: 0,
        skipped_count: 0,
    };

    // JSONRPCクライアントとウォレットを取得
    let client = crate::jsonrpc::new_client();
    let wallet = crate::wallet::new_wallet();

    // TradeRecorderを作成（バッチIDで関連取引をグループ化）
    let recorder = TradeRecorder::new();
    info!(log, "created trade recorder"; "batch_id" => recorder.get_batch_id());

    for action in actions {
        match execute_single_action(&client, &wallet, action, &recorder).await {
            Ok(_) => {
                info!(log, "action executed successfully"; "action" => ?action);
                summary.success_count += 1;
            }
            Err(e) => {
                error!(log, "action execution failed"; "action" => ?action, "error" => ?e);
                summary.failed_count += 1;
            }
        }
    }

    Ok(summary)
}

/// 単一の取引アクションを実行
async fn execute_single_action<C, W>(
    client: &C,
    wallet: &W,
    action: &TradingAction,
    recorder: &TradeRecorder,
) -> Result<()>
where
    C: crate::jsonrpc::AccountInfo
        + crate::jsonrpc::SendTx
        + crate::jsonrpc::ViewContract
        + crate::jsonrpc::GasInfo,
    <C as crate::jsonrpc::SendTx>::Output: std::fmt::Display,
    W: crate::wallet::Wallet,
{
    let log = DEFAULT.new(o!("function" => "execute_single_action"));

    match action {
        TradingAction::Hold => {
            // HODLなので何もしない
            info!(log, "holding position");
            Ok(())
        }
        TradingAction::Sell { token, target } => {
            // token を売却して target を購入
            info!(log, "executing sell"; "from" => token, "to" => target);

            // 2段階のswap: token → wrap.near → target
            let wrap_near = &crate::ref_finance::token_account::WNEAR_TOKEN;

            // Step 1: token → wrap.near
            if token != &wrap_near.to_string() {
                swap::execute_direct_swap(client, wallet, token, &wrap_near.to_string(), recorder)
                    .await?;
            }

            // Step 2: wrap.near → target
            if target != &wrap_near.to_string() {
                swap::execute_direct_swap(client, wallet, &wrap_near.to_string(), target, recorder)
                    .await?;
            }

            info!(log, "sell completed"; "from" => token, "to" => target);
            Ok(())
        }
        TradingAction::Switch { from, to } => {
            // from から to へ切り替え（直接スワップ）
            info!(log, "executing switch"; "from" => from, "to" => to);

            swap::execute_direct_swap(client, wallet, from, to, recorder).await?;

            info!(log, "switch completed"; "from" => from, "to" => to);
            Ok(())
        }
        TradingAction::Rebalance { target_weights } => {
            // ポートフォリオのリバランス
            info!(log, "executing rebalance"; "weights" => ?target_weights);

            // 現在の保有量を取得
            let tokens: Vec<String> = target_weights.keys().cloned().collect();
            let current_balances =
                crate::trade::swap::get_current_portfolio_balances(client, wallet, &tokens).await?;

            // 総ポートフォリオ価値を計算
            let total_portfolio_value = crate::trade::swap::calculate_total_portfolio_value(
                client,
                wallet,
                &current_balances,
            )
            .await?;
            let total_value_u128 =
                total_portfolio_value
                    .to_string()
                    .parse::<u128>()
                    .map_err(|e| {
                        anyhow::anyhow!("Failed to convert total portfolio value to u128: {}", e)
                    })?;

            // 各トークンの目標ウェイトに基づいてリバランス
            for (token, target_weight) in target_weights.iter() {
                let current_balance = current_balances.get(token).copied().unwrap_or(0);

                // 目標金額を計算
                let target_weight_decimal = BigDecimal::from_str(&target_weight.to_string())
                    .map_err(|e| {
                        anyhow::anyhow!("Failed to convert target weight to BigDecimal: {}", e)
                    })?;
                let target_value = BigDecimal::from(total_value_u128) * target_weight_decimal;
                // BigDecimalをu128に変換（整数部分のみ）
                use num_bigint::ToBigInt;
                let target_value_bigint = target_value.to_bigint().ok_or_else(|| {
                    anyhow::anyhow!("Failed to convert target value BigDecimal to BigInt")
                })?;
                let target_amount_u128 =
                    target_value_bigint
                        .to_string()
                        .parse::<u128>()
                        .map_err(|e| {
                            anyhow::anyhow!(
                                "Failed to convert target value to u128: {} (value: {})",
                                e,
                                target_value_bigint.to_string()
                            )
                        })?;

                // 現在の保有量と目標量の差を計算
                if target_amount_u128 > current_balance {
                    // ポジション増加が必要
                    let buy_amount = target_amount_u128 - current_balance;
                    info!(log, "rebalancing: buying token";
                        "token" => token,
                        "target_weight" => target_weight,
                        "current_balance" => current_balance,
                        "target_amount" => target_amount_u128,
                        "buy_amount" => buy_amount
                    );

                    // 最小交換額以上の場合のみ実行
                    if buy_amount >= 1000000000000000000000000 {
                        // 1 NEAR以上
                        let wrap_near = &crate::ref_finance::token_account::WNEAR_TOKEN;
                        if token != &wrap_near.to_string() {
                            swap::execute_direct_swap(
                                client,
                                wallet,
                                &wrap_near.to_string(),
                                token,
                                recorder,
                            )
                            .await?;
                        }
                    }
                } else if current_balance > target_amount_u128 {
                    // ポジション削減が必要
                    let sell_amount = current_balance - target_amount_u128;
                    info!(log, "rebalancing: selling token";
                        "token" => token,
                        "target_weight" => target_weight,
                        "current_balance" => current_balance,
                        "target_amount" => target_amount_u128,
                        "sell_amount" => sell_amount
                    );

                    // 最小交換額以上の場合のみ実行
                    if sell_amount >= 1000000000000000000000000 {
                        // 1 NEAR以上
                        let wrap_near = &crate::ref_finance::token_account::WNEAR_TOKEN;
                        if token != &wrap_near.to_string() {
                            swap::execute_direct_swap(
                                client,
                                wallet,
                                token,
                                &wrap_near.to_string(),
                                recorder,
                            )
                            .await?;
                        }
                    }
                } else {
                    info!(log, "rebalancing: no action needed";
                        "token" => token,
                        "target_weight" => target_weight,
                        "current_balance" => current_balance,
                        "target_amount" => target_amount_u128
                    );
                }
            }

            info!(log, "rebalance completed");
            Ok(())
        }
        TradingAction::AddPosition { token, weight } => {
            // ポジション追加
            info!(log, "adding position"; "token" => token, "weight" => weight);

            // wrap.near → token へのswap
            let wrap_near = &crate::ref_finance::token_account::WNEAR_TOKEN;
            if token != &wrap_near.to_string() {
                swap::execute_direct_swap(client, wallet, &wrap_near.to_string(), token, recorder)
                    .await?;
            }

            info!(log, "position added"; "token" => token, "weight" => weight);
            Ok(())
        }
        TradingAction::ReducePosition { token, weight } => {
            // ポジション削減
            info!(log, "reducing position"; "token" => token, "weight" => weight);

            // token → wrap.near へのswap
            let wrap_near = &crate::ref_finance::token_account::WNEAR_TOKEN;
            if token != &wrap_near.to_string() {
                swap::execute_direct_swap(client, wallet, token, &wrap_near.to_string(), recorder)
                    .await?;
            }

            info!(log, "position reduced"; "token" => token, "weight" => weight);
            Ok(())
        }
    }
}

/// 実行サマリー
struct ExecutionSummary {
    #[allow(dead_code)]
    total: usize,
    success_count: usize,
    failed_count: usize,
    #[allow(dead_code)]
    skipped_count: usize,
}

/// ハーベスト判定と実行
async fn check_and_harvest(initial_amount: u128) -> Result<()> {
    // 実際のハーベスト機能を呼び出す
    crate::trade::harvest::check_and_harvest(initial_amount).await
}

/// 評価期間のチェックと管理
///
/// 戻り値: (period_id, is_new_period, selected_tokens)
async fn manage_evaluation_period(available_funds: u128) -> Result<(String, bool, Vec<String>)> {
    let log = DEFAULT.new(o!("function" => "manage_evaluation_period"));

    const EVALUATION_PERIOD_DAYS: i64 = 10;

    // 最新の評価期間を取得
    let latest_period = EvaluationPeriod::get_latest_async().await?;

    match latest_period {
        Some(period) => {
            let now = Utc::now().naive_utc();
            let period_duration = now.signed_duration_since(period.start_time);

            if period_duration.num_days() >= EVALUATION_PERIOD_DAYS {
                // 評価期間終了: 全トークンを売却して新規期間を開始
                info!(log, "evaluation period ended, starting new period";
                    "previous_period_id" => %period.period_id,
                    "days_elapsed" => period_duration.num_days()
                );

                // 全トークンをwrap.nearに売却
                let final_value = liquidate_all_positions().await?;
                info!(log, "liquidated all positions"; "final_value" => %final_value);

                // 新規評価期間を作成
                let new_period = NewEvaluationPeriod::new(final_value.clone(), vec![]);
                let created_period = new_period.insert_async().await?;

                info!(log, "created new evaluation period";
                    "period_id" => %created_period.period_id,
                    "initial_value" => %created_period.initial_value
                );

                Ok((created_period.period_id, true, vec![]))
            } else {
                // 評価期間中: 既存の選定トークンを返す
                info!(log, "continuing evaluation period";
                    "period_id" => %period.period_id,
                    "days_remaining" => EVALUATION_PERIOD_DAYS - period_duration.num_days()
                );

                let selected_tokens = period
                    .selected_tokens
                    .unwrap_or_default()
                    .into_iter()
                    .flatten()
                    .collect();

                Ok((period.period_id, false, selected_tokens))
            }
        }
        None => {
            // 初回起動: 新規評価期間を作成
            info!(log, "no evaluation period found, creating first period");

            let initial_value = BigDecimal::from(available_funds);
            let new_period = NewEvaluationPeriod::new(initial_value.clone(), vec![]);
            let created_period = new_period.insert_async().await?;

            info!(log, "created first evaluation period";
                "period_id" => %created_period.period_id,
                "initial_value" => %created_period.initial_value
            );

            Ok((created_period.period_id, true, vec![]))
        }
    }
}

/// 全保有トークンをwrap.nearに売却
///
/// 戻り値: 売却後のwrap.near総額 (yoctoNEAR)
async fn liquidate_all_positions() -> Result<BigDecimal> {
    let log = DEFAULT.new(o!("function" => "liquidate_all_positions"));

    // 最新の評価期間を取得して選定トークンを確認
    let latest_period = EvaluationPeriod::get_latest_async().await?;
    let tokens_to_liquidate: Vec<String> = match latest_period {
        Some(period) => period
            .selected_tokens
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .collect(),
        None => {
            info!(log, "no evaluation period found, nothing to liquidate");
            return Ok(BigDecimal::from(0));
        }
    };

    if tokens_to_liquidate.is_empty() {
        info!(log, "no tokens to liquidate");
        // wrap.nearの残高を返す
        let client = crate::jsonrpc::new_client();
        let wallet = crate::wallet::new_wallet();
        let wrap_near = &crate::ref_finance::token_account::WNEAR_TOKEN;
        let balance =
            crate::ref_finance::balances::start(&client, &wallet, wrap_near, None).await?;
        return Ok(BigDecimal::from(balance));
    }

    info!(log, "liquidating positions"; "token_count" => tokens_to_liquidate.len());

    // クライアントとウォレットを取得
    let client = crate::jsonrpc::new_client();
    let wallet = crate::wallet::new_wallet();
    let recorder = TradeRecorder::new();

    // 各トークンをwrap.nearに変換
    let wrap_near_str = crate::ref_finance::token_account::WNEAR_TOKEN.to_string();

    for token in &tokens_to_liquidate {
        if token == &wrap_near_str {
            // wrap.nearは変換不要
            continue;
        }

        info!(log, "liquidating token"; "token" => token);

        // トークンの残高を確認
        let token_account: crate::ref_finance::token_account::TokenAccount = token
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid token: {}", e))?;

        let balance =
            crate::ref_finance::balances::start(&client, &wallet, &token_account, None).await?;

        if balance == 0 {
            info!(log, "token has zero balance, skipping"; "token" => token);
            continue;
        }

        // token → wrap.near にスワップ
        match swap::execute_direct_swap(&client, &wallet, token, &wrap_near_str, &recorder).await {
            Ok(_) => {
                info!(log, "successfully liquidated token"; "token" => token);
            }
            Err(e) => {
                error!(log, "failed to liquidate token"; "token" => token, "error" => ?e);
                // エラーが発生しても他のトークンの売却は継続
            }
        }
    }

    // 最終的なwrap.near残高を取得
    let wrap_near = &crate::ref_finance::token_account::WNEAR_TOKEN;
    let final_balance =
        crate::ref_finance::balances::start(&client, &wallet, wrap_near, None).await?;

    info!(log, "liquidation complete"; "final_wrap_near_balance" => final_balance);
    Ok(BigDecimal::from(final_balance))
}

/// 価格履歴からボラティリティを計算
fn calculate_volatility_from_history(history: &PriceHistory) -> Result<BigDecimal> {
    if history.prices.len() < 2 {
        return Err(anyhow::anyhow!(
            "Insufficient price data for volatility calculation: {} points",
            history.prices.len()
        ));
    }

    // 日次リターンを計算 (BigDecimalを使用)
    let returns: Vec<BigDecimal> = history
        .prices
        .windows(2)
        .filter_map(|window| {
            let prev_price = &window[0].price;
            let curr_price = &window[1].price;

            if prev_price.is_zero() {
                None
            } else {
                let return_rate = (curr_price - prev_price) / prev_price;
                Some(return_rate)
            }
        })
        .collect();

    if returns.is_empty() {
        return Err(anyhow::anyhow!(
            "No valid price returns for volatility calculation"
        ));
    }

    // 平均リターンを計算
    let sum: BigDecimal = returns.iter().sum();
    let count = BigDecimal::from(returns.len() as u64);
    let mean = &sum / &count;

    // 分散を計算
    let variance_sum: BigDecimal = returns
        .iter()
        .map(|r| {
            let diff = r - &mean;
            &diff * &diff
        })
        .sum();

    let variance = &variance_sum / &count;

    // BigDecimalで平方根を計算（Newton法による近似）
    if variance.is_zero() {
        return Ok(BigDecimal::from(0));
    }

    // 負の分散は無効
    if variance < BigDecimal::from(0) {
        return Err(anyhow::anyhow!("Invalid negative variance"));
    }

    // Newton法による平方根計算
    let sqrt_variance = sqrt_bigdecimal(&variance)?;
    Ok(sqrt_variance)
}

/// 拡張された流動性スコアを計算（プール情報 + 取引量ベース）
/// 0.0 - 1.0 の範囲でスコアを返す
async fn calculate_enhanced_liquidity_score<C>(
    client: &C,
    token_id: &str,
    history: &PriceHistory,
) -> f64
where
    C: crate::jsonrpc::ViewContract,
{
    // 1. 基本的な取引量ベーススコア
    let volume_score = calculate_liquidity_score(history);

    // 2. REF Financeプール流動性スコア
    let pool_score = calculate_pool_liquidity_score(client, token_id).await;

    // 3. 両方のスコアを重み付き平均で統合（取引量60%, プール40%）
    let combined_score = volume_score * 0.6 + pool_score * 0.4;
    combined_score.clamp(0.0, 1.0)
}

/// プール流動性スコアを計算
async fn calculate_pool_liquidity_score<C>(client: &C, token_id: &str) -> f64
where
    C: crate::jsonrpc::ViewContract,
{
    use near_sdk::AccountId;

    // REF Finance Exchangeアカウント
    let ref_exchange_account = match "v2.ref-finance.near".parse::<AccountId>() {
        Ok(account) => account,
        Err(_) => return 0.3, // デフォルト値
    };

    // プールで利用可能な流動性を取得
    match get_token_pool_liquidity(client, &ref_exchange_account, token_id).await {
        Ok(liquidity_amount) => {
            // 流動性をスコアに変換（10^25 yoctoNEAR を高流動性の基準とする）
            let high_liquidity_threshold = 10u128.pow(25); // 10 NEAR相当
            let liquidity_ratio = liquidity_amount as f64 / high_liquidity_threshold as f64;

            // シグモイド的変換で 0.0-1.0 にマッピング
            let normalized_score = liquidity_ratio / (1.0 + liquidity_ratio);
            normalized_score.clamp(0.0, 1.0)
        }
        Err(_) => 0.3, // エラー時はデフォルト値
    }
}

/// トークンのプール流動性を取得
async fn get_token_pool_liquidity<C>(
    client: &C,
    ref_exchange_account: &AccountId,
    token_id: &str,
) -> Result<u128>
where
    C: crate::jsonrpc::ViewContract,
{
    use serde_json::Value;

    // ft_balance_of でREF Exchangeでの残高を取得
    let token_account: AccountId = token_id
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid token account ID: {}", e))?;

    let args = serde_json::json!({
        "account_id": ref_exchange_account.to_string()
    });

    let result = client
        .view_contract(&token_account, "ft_balance_of", &args)
        .await?;

    let balance_json: Value = serde_json::from_slice(&result.result)
        .map_err(|e| anyhow::anyhow!("Failed to parse balance result: {}", e))?;

    if let Some(balance_str) = balance_json.as_str() {
        balance_str
            .parse::<u128>()
            .map_err(|e| anyhow::anyhow!("Failed to parse balance: {}", e))
    } else {
        Err(anyhow::anyhow!(
            "Expected string balance, got: {:?}",
            balance_json
        ))
    }
}

/// 基本的な流動性スコアを計算（取引量ベース）
/// 0.0 - 1.0 の範囲でスコアを返す
fn calculate_liquidity_score(history: &PriceHistory) -> f64 {
    // 取引量データがある価格ポイントを集計
    let volumes: Vec<&BigDecimal> = history
        .prices
        .iter()
        .filter_map(|p| p.volume.as_ref())
        .collect();

    if volumes.is_empty() {
        // 取引量データがない場合は中間値を返す
        return 0.5;
    }

    // 平均取引量を計算
    let sum: BigDecimal = volumes.iter().map(|v| (*v).clone()).sum();
    let count = BigDecimal::from(volumes.len() as u64);
    let avg_volume = &sum / &count;

    // 取引量を正規化（簡易版：10^24 yoctoNEAR を基準）
    let base_volume = BigDecimal::from(10u128.pow(24));
    let normalized = &avg_volume / &base_volume;

    // 0.0 - 1.0 の範囲に収める（シグモイド的な変換）
    let score = normalized
        .to_string()
        .parse::<f64>()
        .unwrap_or(0.5)
        .clamp(0.0, 1.0);

    // 対数スケールで調整（大きな値を圧縮）
    if score > 0.0 {
        let ln_result = (score.ln() + 10.0) / 20.0;
        ln_result.clamp(0.0, 1.0) // 0-1の範囲に制限
    } else {
        0.1 // 最小値
    }
}

/// 市場規模を推定（実際の発行量データを取得）
async fn estimate_market_cap_async<C>(client: &C, token_id: &str, current_price_yocto: u128) -> f64
where
    C: crate::jsonrpc::ViewContract,
{
    // 実際の発行量データを取得
    let total_supply = get_token_total_supply(client, token_id)
        .await
        .unwrap_or(1_000_000u128); // 取得失敗時は100万トークンと仮定

    // yoctoNEARから通常の単位に変換
    let price_in_near = BigDecimal::from(current_price_yocto) / BigDecimal::from(10u128.pow(24));

    // 市場規模 = 価格 × 発行量
    let market_cap = price_in_near * BigDecimal::from(total_supply);

    market_cap.to_string().parse::<f64>().unwrap_or(10000.0)
}

/// トークンの総発行量を取得
async fn get_token_total_supply<C>(client: &C, token_id: &str) -> Result<u128>
where
    C: crate::jsonrpc::ViewContract,
{
    use near_sdk::AccountId;
    use serde_json::Value;

    let account_id: AccountId = token_id
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid token account ID: {}", e))?;

    let args = serde_json::json!({});
    let result = client
        .view_contract(&account_id, "ft_total_supply", &args)
        .await?;

    // resultフィールドからJSONデータを取得してパース
    let json_value: Value = serde_json::from_slice(&result.result)
        .map_err(|e| anyhow::anyhow!("Failed to parse result as JSON: {}", e))?;

    // total_supplyは通常文字列として返される
    if let Some(total_supply_str) = json_value.as_str() {
        total_supply_str
            .parse::<u128>()
            .map_err(|e| anyhow::anyhow!("Failed to parse total supply: {}", e))
    } else {
        Err(anyhow::anyhow!(
            "Expected string value for total supply, got: {:?}",
            json_value
        ))
    }
}

/// BigDecimalで平方根を計算（Newton法による近似）
fn sqrt_bigdecimal(value: &BigDecimal) -> Result<BigDecimal> {
    if value.is_zero() {
        return Ok(BigDecimal::from(0));
    }

    if *value < BigDecimal::from(0) {
        return Err(anyhow::anyhow!(
            "Cannot calculate square root of negative number"
        ));
    }

    // Newton法での近似計算
    let two = BigDecimal::from(2);
    // 精度を BigDecimal で直接設定 (1e-10 相当)
    let precision = BigDecimal::from(1) / BigDecimal::from(10000000000u64); // 1e-10

    // 初期推定値（入力値の半分）
    let mut x = value / &two;

    for _iteration in 0..50 {
        // 最大50回の反復
        let next_x = (&x + (value / &x)) / &two;

        // 収束判定
        let diff = if next_x > x {
            &next_x - &x
        } else {
            &x - &next_x
        };
        if diff < precision {
            return Ok(next_x);
        }

        x = next_x;
    }

    // 収束しなかった場合でも現在の近似値を返す
    Ok(x)
}

#[allow(dead_code)]
async fn forcast_rates(
    range: &TimeRange,
    period: Duration,
    target: NaiveDateTime,
) -> Result<HashMap<TokenOutAccount, BigDecimal>> {
    let log = DEFAULT.new(o!("function" => "trade::forcast_rates"));
    info!(log, "start");
    let quote = get_top_quote_token(range).await?;
    let bases = get_base_tokens(range, &quote).await?;
    let ps = bases.iter().map(|base| async {
        let rates = SameBaseTokenRates::load(&quote, base, range).await?;
        let result = rates.forcast(period, target).await?;
        Ok((base.clone(), result))
    });
    let rates_by_base = join_all(ps).await;
    info!(log, "success");
    rates_by_base.into_iter().collect()
}

#[allow(dead_code)]
async fn get_top_quote_token(range: &TimeRange) -> Result<TokenInAccount> {
    let log = DEFAULT.new(o!("function" => "trade::get_top_quote_token"));

    let quotes = TokenRate::get_quotes_in_time_range(range).await?;
    let (quote, _) = quotes
        .iter()
        .max_by_key(|(_, c)| *c)
        .ok_or_else(|| anyhow::anyhow!("No quote tokens found in time range"))?;

    info!(log, "success");
    Ok(quote.clone())
}

#[allow(dead_code)]
async fn get_base_tokens(
    range: &TimeRange,
    quote: &TokenInAccount,
) -> Result<Vec<TokenOutAccount>> {
    let log = DEFAULT.new(o!("function" => "trade::get_base_tokens"));

    let bases = TokenRate::get_bases_in_time_range(range, quote).await?;
    let max_count = bases
        .iter()
        .max_by_key(|(_, c)| *c)
        .ok_or_else(|| anyhow::anyhow!("No base tokens found in time range"))?
        .1;
    let limit = max_count / 2;
    let tokens = bases
        .iter()
        .filter(|(_, c)| *c > limit)
        .map(|(t, _)| t.clone())
        .collect();

    info!(log, "success");
    Ok(tokens)
}

impl SameBaseTokenRates {
    pub async fn load(
        quote: &TokenInAccount,
        base: &TokenOutAccount,
        range: &TimeRange,
    ) -> Result<Self> {
        let log = DEFAULT.new(o!(
            "function" => "SameBaseTokenRates::load",
            "base" => base.to_string(),
            "quote" => quote.to_string(),
            "start" => format!("{:?}", range.start),
            "end" => format!("{:?}", range.end),
        ));
        info!(log, "start");
        match TokenRate::get_rates_in_time_range(range, base, quote).await {
            Ok(rates) => {
                info!(log, "loaded rates"; "rates_count" => rates.len());
                let points = rates
                    .iter()
                    .map(|r| Point {
                        rate: r.rate.clone(),
                        timestamp: r.timestamp,
                    })
                    .collect();
                Ok(SameBaseTokenRates {
                    base: base.clone(),
                    quote: quote.clone(),
                    points,
                })
            }
            Err(e) => {
                error!(log, "Failed to get rates"; "error" => ?e);
                Err(e)
            }
        }
    }

    #[allow(dead_code)]
    async fn forcast(&self, period: Duration, target: NaiveDateTime) -> Result<BigDecimal> {
        let log = DEFAULT.new(o!(
            "function" => "SameBaseTokenRates::forcast",
            "period" => format!("{}", period),
            "target" => format!("{:?}", target),
        ));
        info!(log, "start");

        let stats = self.aggregate(period);
        let _descs = stats.describes();

        // arima モジュールの予測関数を使用して将来の値を予測
        let result = arima::predict_future_rate(&self.points, target)?;

        info!(log, "success"; "predicted_rate" => %result);
        Ok(result)
    }

    pub fn aggregate(&self, period: Duration) -> ListStatsInPeriod<BigDecimal> {
        let log = DEFAULT.new(o!(
            "function" => "SameBaseTokenRates::aggregate",
            "rates_count" => self.points.len(),
            "period" => format!("{}", period),
        ));
        info!(log, "start");

        if self.points.is_empty() {
            return ListStatsInPeriod(Vec::new());
        }

        // タイムスタンプの最小値と最大値を取得
        let min_time = self
            .points
            .first()
            .expect("Points vector is not empty")
            .timestamp;
        let max_time = self
            .points
            .last()
            .expect("Points vector is not empty")
            .timestamp;

        // 期間ごとに統計を計算
        let mut stats = Vec::new();
        let mut current_start = min_time;

        while current_start <= max_time {
            let current_end = current_start + period;
            let rates_in_period: Vec<_> = self
                .points
                .iter()
                .skip_while(|rate| rate.timestamp < current_start)
                .take_while(|rate| rate.timestamp < current_end)
                .collect();

            if !rates_in_period.is_empty() {
                let start = rates_in_period
                    .first()
                    .expect("Rates in period is not empty")
                    .rate
                    .clone();
                let end = rates_in_period
                    .last()
                    .expect("Rates in period is not empty")
                    .rate
                    .clone();
                let values: Vec<_> = rates_in_period.iter().map(|tr| tr.rate.clone()).collect();
                let sum: BigDecimal = values.iter().sum();
                let count = BigDecimal::from(values.len() as i64);
                let average = &sum / &count;
                let max = values
                    .iter()
                    .max()
                    .expect("Values vector is not empty")
                    .clone();
                let min = values
                    .iter()
                    .min()
                    .expect("Values vector is not empty")
                    .clone();

                stats.push(StatsInPeriod {
                    timestamp: current_start,
                    period,
                    start,
                    end,
                    average,
                    max,
                    min,
                });
            }

            current_start = current_end;
        }

        info!(log, "success"; "stats_count" => stats.len());
        ListStatsInPeriod(stats)
    }
}

impl<U> ListStatsInPeriod<U>
where
    U: Clone + Display,
    U: Add<Output = U> + Sub<Output = U> + Mul<Output = U> + Div<Output = U>,
    U: Zero + PartialOrd + From<i64>,
{
    fn format_decimal(value: U) -> String {
        let s = value.to_string();
        if s.contains('.') {
            // 小数点以下の末尾の0を削除し、最大9桁まで表示
            let parts: Vec<&str> = s.split('.').collect();
            if parts.len() == 2 {
                let integer_part = parts[0];
                let mut decimal_part = parts[1];

                // 小数点以下が全て0の場合は整数表示
                if decimal_part.chars().all(|c| c == '0') {
                    return integer_part.to_string();
                }

                // 末尾の0を削除
                decimal_part = decimal_part.trim_end_matches('0');

                // 小数点以下が9桁を超える場合は9桁までに制限
                if decimal_part.len() > 9 {
                    decimal_part = &decimal_part[..9];
                }

                // 小数点以下が空になった場合は整数のみ返す
                if decimal_part.is_empty() {
                    return integer_part.to_string();
                }

                format!("{}.{}", integer_part, decimal_part)
            } else {
                s
            }
        } else {
            s
        }
    }

    pub fn describes(&self) -> Vec<String> {
        let log = DEFAULT.new(o!(
            "function" => "ListStatsInPeriod::describes",
            "stats_count" => self.0.len(),
        ));
        info!(log, "start");
        let mut lines = Vec::new();
        let mut prev = None;
        for stat in self.0.iter() {
            let date = stat.timestamp.to_string();
            let changes = prev
                .map(|p: &StatsInPeriod<U>| {
                    let prev = format!(
                        "from the previous {m} minutes",
                        m = stat.period.num_minutes()
                    );
                    let diff = stat.end.clone() - p.end.clone();
                    if diff.is_zero() {
                        return format!(", no change {prev}");
                    }
                    let dw = if diff < U::zero() {
                        "decrease"
                    } else {
                        "increase"
                    };
                    let change = (diff / p.end.clone()) * 100_i64.into();
                    let change_str = Self::format_decimal(change);
                    format!(", marking a {change_str} % {dw} {prev}")
                })
                .unwrap_or_default();
            let summary = format!(
                "opened at {start}, closed at {end}, with a high of {max}, a low of {min}, and an average of {ave}",
                start = Self::format_decimal(stat.start.clone()),
                end = Self::format_decimal(stat.end.clone()),
                max = Self::format_decimal(stat.max.clone()),
                min = Self::format_decimal(stat.min.clone()),
                ave = Self::format_decimal(stat.average.clone()),
            );
            let line = format!("{date}, {summary}{changes}");
            debug!(log, "added line";
                "line" => &line,
            );
            lines.push(line);
            prev = Some(stat);
        }
        info!(log, "success";
           "lines_count" => lines.len(),
        );
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ref_finance::token_account::TokenAccount;
    use std::str::FromStr;

    #[test]
    fn test_describes() {
        let stats: ListStatsInPeriod<BigDecimal> = ListStatsInPeriod(vec![]);
        assert!(stats.describes().is_empty());
    }

    #[test]
    fn test_describes_increase() {
        let stats: ListStatsInPeriod<BigDecimal> = ListStatsInPeriod(vec![
            StatsInPeriod {
                timestamp: NaiveDateTime::parse_from_str(
                    "2025-03-26 11:37:48.195977",
                    "%Y-%m-%d %H:%M:%S%.f",
                )
                .unwrap(),
                period: Duration::minutes(1),
                start: BigDecimal::from(101),
                end: BigDecimal::from(100),
                max: BigDecimal::from(102),
                min: BigDecimal::from(90),
                average: BigDecimal::from(95),
            },
            StatsInPeriod {
                timestamp: NaiveDateTime::parse_from_str(
                    "2025-03-27 11:37:48.196150",
                    "%Y-%m-%d %H:%M:%S%.f",
                )
                .unwrap(),
                period: Duration::minutes(1),
                start: BigDecimal::from(100),
                end: BigDecimal::from(150),
                max: BigDecimal::from(155),
                min: BigDecimal::from(140),
                average: BigDecimal::from(147),
            },
        ]);
        let descriptions = stats.describes();
        assert_eq!(descriptions.len(), 2);
        assert!(descriptions[1].contains("increase"));
        assert!(descriptions[1].contains("50 %"));
        assert_eq!(
            descriptions,
            vec![
                "2025-03-26 11:37:48.195977, opened at 101, closed at 100, with a high of 102, a low of 90, and an average of 95",
                "2025-03-27 11:37:48.196150, opened at 100, closed at 150, with a high of 155, a low of 140, and an average of 147, marking a 50 % increase from the previous 1 minutes"
            ]
        );
    }

    #[test]
    fn test_describes_decrease() {
        let stats: ListStatsInPeriod<BigDecimal> = ListStatsInPeriod(vec![
            StatsInPeriod {
                timestamp: NaiveDateTime::parse_from_str(
                    "2025-03-26 11:37:48.195977",
                    "%Y-%m-%d %H:%M:%S%.f",
                )
                .unwrap(),
                period: Duration::minutes(1),
                start: BigDecimal::from(100),
                end: BigDecimal::from(100),
                max: BigDecimal::from(100),
                min: BigDecimal::from(100),
                average: BigDecimal::from(100),
            },
            StatsInPeriod {
                timestamp: NaiveDateTime::parse_from_str(
                    "2025-03-27 11:37:48.196150",
                    "%Y-%m-%d %H:%M:%S%.f",
                )
                .unwrap(),
                period: Duration::minutes(1),
                start: BigDecimal::from(100),
                end: BigDecimal::from(50),
                max: BigDecimal::from(50),
                min: BigDecimal::from(50),
                average: BigDecimal::from(50),
            },
        ]);
        let descriptions = stats.describes();
        assert_eq!(descriptions.len(), 2);
        assert!(descriptions[1].contains("decrease"));
        assert!(descriptions[1].contains("50 %"));
        assert_eq!(
            descriptions,
            vec![
                "2025-03-26 11:37:48.195977, opened at 100, closed at 100, with a high of 100, a low of 100, and an average of 100",
                "2025-03-27 11:37:48.196150, opened at 100, closed at 50, with a high of 50, a low of 50, and an average of 50, marking a -50 % decrease from the previous 1 minutes"
            ]
        );
    }

    #[test]
    fn test_describes_no_change() {
        let stats: ListStatsInPeriod<BigDecimal> = ListStatsInPeriod(vec![
            StatsInPeriod {
                timestamp: NaiveDateTime::parse_from_str(
                    "2025-03-26 11:37:48.195977",
                    "%Y-%m-%d %H:%M:%S%.f",
                )
                .unwrap(),
                period: Duration::minutes(1),
                start: BigDecimal::from_str("100.123456789").unwrap(),
                end: BigDecimal::from_str("100.123456789").unwrap(),
                max: BigDecimal::from_str("100.123456789").unwrap(),
                min: BigDecimal::from_str("100.123456789").unwrap(),
                average: BigDecimal::from_str("100.123456789").unwrap(),
            },
            StatsInPeriod {
                timestamp: NaiveDateTime::parse_from_str(
                    "2025-03-27 11:37:48.196150",
                    "%Y-%m-%d %H:%M:%S%.f",
                )
                .unwrap(),
                period: Duration::minutes(1),
                start: BigDecimal::from_str("100.123456789").unwrap(),
                end: BigDecimal::from_str("100.123456789").unwrap(),
                max: BigDecimal::from_str("100.123456789").unwrap(),
                min: BigDecimal::from_str("100.123456789").unwrap(),
                average: BigDecimal::from_str("100.123456789").unwrap(),
            },
        ]);
        let descriptions = stats.describes();
        assert_eq!(descriptions.len(), 2);
        assert!(descriptions[1].contains("no change"));
        assert_eq!(
            descriptions,
            vec![
                "2025-03-26 11:37:48.195977, opened at 100.123456789, closed at 100.123456789, with a high of 100.123456789, a low of 100.123456789, and an average of 100.123456789",
                "2025-03-27 11:37:48.196150, opened at 100.123456789, closed at 100.123456789, with a high of 100.123456789, a low of 100.123456789, and an average of 100.123456789, no change from the previous 1 minutes"
            ]
        );
    }

    #[test]
    fn test_stats_empty() {
        // 空のポイントリストを持つSameBaseTokenRatesを作成
        let rates = SameBaseTokenRates {
            points: Vec::new(),
            base: "wrap.near".parse::<TokenAccount>().unwrap().into(),
            quote: "usdt.tether-token.near"
                .parse::<TokenAccount>()
                .unwrap()
                .into(),
        };

        // 1分間の期間で統計を計算
        let stats = rates.aggregate(Duration::minutes(1));

        // 結果が空のベクターであることを確認
        assert!(stats.0.is_empty());
    }

    #[test]
    fn test_stats_single_period() {
        // 1つの期間内に複数のポイントを持つSameBaseTokenRatesを作成
        let base_time =
            NaiveDateTime::parse_from_str("2025-03-26 10:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let points = vec![
            Point {
                timestamp: base_time,
                rate: BigDecimal::from(100),
            },
            Point {
                timestamp: base_time + Duration::seconds(20),
                rate: BigDecimal::from(110),
            },
            Point {
                timestamp: base_time + Duration::seconds(40),
                rate: BigDecimal::from(90),
            },
        ];

        let rates = SameBaseTokenRates {
            points,
            base: "wrap.near".parse::<TokenAccount>().unwrap().into(),
            quote: "usdt.tether-token.near"
                .parse::<TokenAccount>()
                .unwrap()
                .into(),
        };

        // 1分間の期間で統計を計算
        let stats = rates.aggregate(Duration::minutes(1));

        // 結果を検証
        assert_eq!(stats.0.len(), 1);
        let stat = &stats.0[0];

        assert_eq!(stat.timestamp, base_time);
        assert_eq!(stat.period, Duration::minutes(1));
        assert_eq!(stat.start, BigDecimal::from(100));
        assert_eq!(stat.end, BigDecimal::from(90));
        assert_eq!(stat.max, BigDecimal::from(110));
        assert_eq!(stat.min, BigDecimal::from(90));

        // 平均値の検証 (100 + 110 + 90) / 3 = 100
        assert_eq!(stat.average, BigDecimal::from(100));
    }

    #[test]
    fn test_stats_multiple_periods() {
        // 複数の期間にまたがるポイントを持つSameBaseTokenRatesを作成
        let base_time =
            NaiveDateTime::parse_from_str("2025-03-26 10:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let points = vec![
            // 最初の期間 (10:00:00 - 10:01:00)
            Point {
                timestamp: base_time,
                rate: BigDecimal::from(100),
            },
            Point {
                timestamp: base_time + Duration::seconds(30),
                rate: BigDecimal::from(110),
            },
            // 2番目の期間 (10:01:00 - 10:02:00)
            Point {
                timestamp: base_time + Duration::minutes(1),
                rate: BigDecimal::from(120),
            },
            Point {
                timestamp: base_time + Duration::minutes(1) + Duration::seconds(30),
                rate: BigDecimal::from(130),
            },
            // 3番目の期間 (10:02:00 - 10:03:00)
            Point {
                timestamp: base_time + Duration::minutes(2),
                rate: BigDecimal::from(140),
            },
            Point {
                timestamp: base_time + Duration::minutes(2) + Duration::seconds(30),
                rate: BigDecimal::from(150),
            },
        ];

        let rates = SameBaseTokenRates {
            points,
            base: "wrap.near".parse::<TokenAccount>().unwrap().into(),
            quote: "usdt.tether-token.near"
                .parse::<TokenAccount>()
                .unwrap()
                .into(),
        };

        // 1分間の期間で統計を計算
        let stats = rates.aggregate(Duration::minutes(1));

        // 結果を検証
        assert_eq!(stats.0.len(), 3);

        // 最初の期間の検証
        {
            let stat = &stats.0[0];
            assert_eq!(stat.timestamp, base_time);
            assert_eq!(stat.period, Duration::minutes(1));
            assert_eq!(stat.start, BigDecimal::from(100));
            assert_eq!(stat.end, BigDecimal::from(110));
            assert_eq!(stat.max, BigDecimal::from(110));
            assert_eq!(stat.min, BigDecimal::from(100));
            assert_eq!(stat.average, BigDecimal::from(105)); // (100 + 110) / 2 = 105
        }

        // 2番目の期間の検証
        {
            let stat = &stats.0[1];
            assert_eq!(stat.timestamp, base_time + Duration::minutes(1));
            assert_eq!(stat.period, Duration::minutes(1));
            assert_eq!(stat.start, BigDecimal::from(120));
            assert_eq!(stat.end, BigDecimal::from(130));
            assert_eq!(stat.max, BigDecimal::from(130));
            assert_eq!(stat.min, BigDecimal::from(120));
            assert_eq!(stat.average, BigDecimal::from(125)); // (120 + 130) / 2 = 125
        }

        // 3番目の期間の検証
        {
            let stat = &stats.0[2];
            assert_eq!(stat.timestamp, base_time + Duration::minutes(2));
            assert_eq!(stat.period, Duration::minutes(1));
            assert_eq!(stat.start, BigDecimal::from(140));
            assert_eq!(stat.end, BigDecimal::from(150));
            assert_eq!(stat.max, BigDecimal::from(150));
            assert_eq!(stat.min, BigDecimal::from(140));
            assert_eq!(stat.average, BigDecimal::from(145)); // (140 + 150) / 2 = 145
        }
    }

    #[test]
    fn test_stats_period_boundary() {
        // 期間の境界値をテストするためのポイントを持つSameBaseTokenRatesを作成
        let base_time =
            NaiveDateTime::parse_from_str("2025-03-26 10:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let points = vec![
            // 最初の期間 (10:00:00 - 10:05:00)
            Point {
                timestamp: base_time,
                rate: BigDecimal::from(100),
            },
            // 境界値ちょうど (10:05:00) - 次の期間に含まれる
            Point {
                timestamp: base_time + Duration::minutes(5),
                rate: BigDecimal::from(200),
            },
            // 2番目の期間 (10:05:00 - 10:10:00)
            Point {
                timestamp: base_time + Duration::minutes(7),
                rate: BigDecimal::from(300),
            },
        ];

        let rates = SameBaseTokenRates {
            points,
            base: "wrap.near".parse::<TokenAccount>().unwrap().into(),
            quote: "usdt.tether-token.near"
                .parse::<TokenAccount>()
                .unwrap()
                .into(),
        };

        // 5分間の期間で統計を計算
        let stats = rates.aggregate(Duration::minutes(5));

        // 結果を検証
        assert_eq!(stats.0.len(), 2);

        // 最初の期間の検証
        {
            let stat = &stats.0[0];
            assert_eq!(stat.timestamp, base_time);
            assert_eq!(stat.period, Duration::minutes(5));
            assert_eq!(stat.start, BigDecimal::from(100));
            assert_eq!(stat.end, BigDecimal::from(100));
            assert_eq!(stat.max, BigDecimal::from(100));
            assert_eq!(stat.min, BigDecimal::from(100));
            assert_eq!(stat.average, BigDecimal::from(100));
        }

        // 2番目の期間の検証 (境界値を含む)
        {
            let stat = &stats.0[1];
            assert_eq!(stat.timestamp, base_time + Duration::minutes(5));
            assert_eq!(stat.period, Duration::minutes(5));
            assert_eq!(stat.start, BigDecimal::from(200));
            assert_eq!(stat.end, BigDecimal::from(300));
            assert_eq!(stat.max, BigDecimal::from(300));
            assert_eq!(stat.min, BigDecimal::from(200));
            assert_eq!(stat.average, BigDecimal::from(250)); // (200 + 300) / 2 = 250
        }
    }

    #[test]
    fn test_calculate_liquidity_score() {
        use chrono::Utc;
        use zaciraci_common::algorithm::types::{PriceHistory, PricePoint};

        // ケース1: 取引量データなし
        let history_no_volume = PriceHistory {
            token: "test.token".to_string(),
            quote_token: "wrap.near".to_string(),
            prices: vec![
                PricePoint {
                    timestamp: Utc::now(),
                    price: BigDecimal::from(100),
                    volume: None,
                },
                PricePoint {
                    timestamp: Utc::now(),
                    price: BigDecimal::from(110),
                    volume: None,
                },
            ],
        };
        let score = calculate_liquidity_score(&history_no_volume);
        assert_eq!(score, 0.5, "No volume data should return 0.5");

        // ケース2: 小さい取引量
        let history_small_volume = PriceHistory {
            token: "test.token".to_string(),
            quote_token: "wrap.near".to_string(),
            prices: vec![
                PricePoint {
                    timestamp: Utc::now(),
                    price: BigDecimal::from(100),
                    volume: Some(BigDecimal::from(1000)),
                },
                PricePoint {
                    timestamp: Utc::now(),
                    price: BigDecimal::from(110),
                    volume: Some(BigDecimal::from(2000)),
                },
            ],
        };
        let score = calculate_liquidity_score(&history_small_volume);
        assert!(
            (0.0..=0.5).contains(&score),
            "Small volume should return low score, got: {}",
            score
        );

        // ケース3: 大きい取引量
        let history_large_volume = PriceHistory {
            token: "test.token".to_string(),
            quote_token: "wrap.near".to_string(),
            prices: vec![
                PricePoint {
                    timestamp: Utc::now(),
                    price: BigDecimal::from(100),
                    volume: Some(BigDecimal::from(10u128.pow(25))), // 10 NEAR相当
                },
                PricePoint {
                    timestamp: Utc::now(),
                    price: BigDecimal::from(110),
                    volume: Some(BigDecimal::from(10u128.pow(25))),
                },
            ],
        };
        let score = calculate_liquidity_score(&history_large_volume);
        assert!(score > 0.4, "Large volume should return higher score");
    }

    #[tokio::test]
    async fn test_estimate_market_cap_async() {
        // モック実装を作成してテスト
        struct MockClient;
        impl crate::jsonrpc::ViewContract for MockClient {
            async fn view_contract<T>(
                &self,
                _receiver: &near_sdk::AccountId,
                _method_name: &str,
                _args: &T,
            ) -> crate::Result<near_primitives::views::CallResult>
            where
                T: ?Sized + serde::Serialize + Sync,
            {
                let total_supply = "1000000"; // 1M tokens
                Ok(near_primitives::views::CallResult {
                    result: serde_json::to_vec(total_supply).unwrap(),
                    logs: vec![],
                })
            }
        }

        let client = MockClient;

        // ケース1: 1 NEAR価格
        let price_1_near = 10u128.pow(24);
        let market_cap = estimate_market_cap_async(&client, "test.token", price_1_near).await;
        assert_eq!(
            market_cap, 1_000_000.0,
            "1 NEAR * 1M tokens = 1M market cap"
        );

        // ケース2: 0.1 NEAR価格
        let price_01_near = 10u128.pow(23);
        let market_cap = estimate_market_cap_async(&client, "test.token", price_01_near).await;
        assert_eq!(
            market_cap, 100_000.0,
            "0.1 NEAR * 1M tokens = 100k market cap"
        );
    }

    #[tokio::test]
    async fn test_get_token_total_supply() {
        // モック実装を作成してテスト
        struct MockClient;
        impl crate::jsonrpc::ViewContract for MockClient {
            async fn view_contract<T>(
                &self,
                _receiver: &near_sdk::AccountId,
                method_name: &str,
                _args: &T,
            ) -> crate::Result<near_primitives::views::CallResult>
            where
                T: ?Sized + serde::Serialize + Sync,
            {
                match method_name {
                    "ft_total_supply" => {
                        let total_supply = "1000000000000000000000000"; // 1M tokens with 18 decimals
                        Ok(near_primitives::views::CallResult {
                            result: serde_json::to_vec(total_supply).unwrap(),
                            logs: vec![],
                        })
                    }
                    _ => Err(anyhow::anyhow!("Unexpected method: {}", method_name)),
                }
            }
        }

        let client = MockClient;
        let result = get_token_total_supply(&client, "test.token").await.unwrap();
        assert_eq!(result, 1_000_000_000_000_000_000_000_000u128);
    }

    #[tokio::test]
    async fn test_calculate_enhanced_liquidity_score() {
        // 拡張流動性スコアのテスト
        struct MockClient;
        impl crate::jsonrpc::ViewContract for MockClient {
            async fn view_contract<T>(
                &self,
                _receiver: &near_sdk::AccountId,
                method_name: &str,
                _args: &T,
            ) -> crate::Result<near_primitives::views::CallResult>
            where
                T: ?Sized + serde::Serialize + Sync,
            {
                match method_name {
                    "ft_balance_of" => {
                        // 高い流動性を模擬（100 NEAR相当のプール残高）
                        let balance = (100u128 * 10u128.pow(24)).to_string(); // 100 NEAR
                        Ok(near_primitives::views::CallResult {
                            result: serde_json::to_vec(&balance).unwrap(),
                            logs: vec![],
                        })
                    }
                    _ => Err(anyhow::anyhow!("Unexpected method: {}", method_name)),
                }
            }
        }

        let client = MockClient;

        // テスト用の取引履歴（中程度の取引量）
        let history = zaciraci_common::algorithm::types::PriceHistory {
            token: "test.token".to_string(),
            quote_token: "wrap.near".to_string(),
            prices: vec![zaciraci_common::algorithm::types::PricePoint {
                timestamp: chrono::Utc::now(),
                price: BigDecimal::from(100),
                volume: Some(BigDecimal::from(5u128 * 10u128.pow(24))), // 5 NEAR相当の取引量
            }],
        };

        let score = calculate_enhanced_liquidity_score(&client, "test.token", &history).await;

        // プール流動性が高いため、スコアは0.5以上になるはず
        assert!(
            score >= 0.5,
            "Enhanced liquidity score should be >= 0.5 with high pool liquidity, got: {}",
            score
        );
        assert!(
            score <= 1.0,
            "Enhanced liquidity score should be <= 1.0, got: {}",
            score
        );
    }

    #[tokio::test]
    async fn test_get_token_pool_liquidity() {
        // プール流動性取得のテスト
        struct MockClient;
        impl crate::jsonrpc::ViewContract for MockClient {
            async fn view_contract<T>(
                &self,
                receiver: &near_sdk::AccountId,
                method_name: &str,
                _args: &T,
            ) -> crate::Result<near_primitives::views::CallResult>
            where
                T: ?Sized + serde::Serialize + Sync,
            {
                match method_name {
                    "ft_balance_of" => {
                        // テスト用の残高（50 NEAR相当）
                        let balance = (50u128 * 10u128.pow(24)).to_string();
                        Ok(near_primitives::views::CallResult {
                            result: serde_json::to_vec(&balance).unwrap(),
                            logs: vec![],
                        })
                    }
                    _ => Err(anyhow::anyhow!(
                        "Unexpected method {} for {}",
                        method_name,
                        receiver
                    )),
                }
            }
        }

        let client = MockClient;
        let ref_account = "v2.ref-finance.near"
            .parse::<near_sdk::AccountId>()
            .unwrap();

        let result = get_token_pool_liquidity(&client, &ref_account, "test.token")
            .await
            .unwrap();

        assert_eq!(result, 50u128 * 10u128.pow(24)); // 50 NEAR
    }

    #[test]
    fn test_sqrt_bigdecimal() {
        use std::str::FromStr;

        // ケース1: 完全平方数
        let value = BigDecimal::from(4);
        let result = sqrt_bigdecimal(&value).unwrap();
        let expected = BigDecimal::from(2);
        assert!((result - expected).abs() < BigDecimal::from_str("0.000001").unwrap());

        // ケース2: 非完全平方数
        let value = BigDecimal::from(2);
        let result = sqrt_bigdecimal(&value).unwrap();
        let expected = BigDecimal::from_str("1.41421356").unwrap();
        assert!((result - expected).abs() < BigDecimal::from_str("0.00001").unwrap());

        // ケース3: 小数
        let value = BigDecimal::from_str("0.25").unwrap();
        let result = sqrt_bigdecimal(&value).unwrap();
        let expected = BigDecimal::from_str("0.5").unwrap();
        assert!((result - expected).abs() < BigDecimal::from_str("0.000001").unwrap());

        // ケース4: ゼロ
        let value = BigDecimal::from(0);
        let result = sqrt_bigdecimal(&value).unwrap();
        assert_eq!(result, BigDecimal::from(0));

        // ケース5: 負の数（エラーケース）
        let value = BigDecimal::from(-1);
        let result = sqrt_bigdecimal(&value);
        assert!(result.is_err());
    }

    #[test]
    fn test_calculate_volatility_from_history() {
        use chrono::Utc;
        use zaciraci_common::algorithm::types::{PriceHistory, PricePoint};

        // ケース1: データポイントが不足
        let history_insufficient = PriceHistory {
            token: "test.token".to_string(),
            quote_token: "wrap.near".to_string(),
            prices: vec![PricePoint {
                timestamp: Utc::now(),
                price: BigDecimal::from(100),
                volume: None,
            }],
        };
        let result = calculate_volatility_from_history(&history_insufficient);
        assert!(result.is_err(), "Should error with insufficient data");

        // ケース2: 価格変動なし
        let history_no_change = PriceHistory {
            token: "test.token".to_string(),
            quote_token: "wrap.near".to_string(),
            prices: vec![
                PricePoint {
                    timestamp: Utc::now(),
                    price: BigDecimal::from(100),
                    volume: None,
                },
                PricePoint {
                    timestamp: Utc::now(),
                    price: BigDecimal::from(100),
                    volume: None,
                },
                PricePoint {
                    timestamp: Utc::now(),
                    price: BigDecimal::from(100),
                    volume: None,
                },
            ],
        };
        let volatility = calculate_volatility_from_history(&history_no_change).unwrap();
        assert_eq!(
            volatility,
            BigDecimal::from(0),
            "No price change should have 0 volatility"
        );

        // ケース3: 価格変動あり
        let history_with_change = PriceHistory {
            token: "test.token".to_string(),
            quote_token: "wrap.near".to_string(),
            prices: vec![
                PricePoint {
                    timestamp: Utc::now(),
                    price: BigDecimal::from(100),
                    volume: None,
                },
                PricePoint {
                    timestamp: Utc::now(),
                    price: BigDecimal::from(110),
                    volume: None,
                },
                PricePoint {
                    timestamp: Utc::now(),
                    price: BigDecimal::from(105),
                    volume: None,
                },
            ],
        };
        let volatility = calculate_volatility_from_history(&history_with_change).unwrap();
        assert!(
            volatility > BigDecimal::from(0),
            "Price changes should result in positive volatility"
        );

        // ケース4: ゼロ価格を含む（スキップされるべき）
        let history_with_zero = PriceHistory {
            token: "test.token".to_string(),
            quote_token: "wrap.near".to_string(),
            prices: vec![
                PricePoint {
                    timestamp: Utc::now(),
                    price: BigDecimal::from(100),
                    volume: None,
                },
                PricePoint {
                    timestamp: Utc::now(),
                    price: BigDecimal::from(0),
                    volume: None,
                },
                PricePoint {
                    timestamp: Utc::now(),
                    price: BigDecimal::from(110),
                    volume: None,
                },
            ],
        };
        let volatility = calculate_volatility_from_history(&history_with_zero).unwrap();
        assert!(
            volatility >= BigDecimal::from(0),
            "Should calculate volatility skipping zero prices, got: {}",
            volatility
        );
    }

    #[test]
    fn test_format_decimal_digits() {
        // 整数値のテスト
        assert_eq!(
            "100",
            ListStatsInPeriod::<BigDecimal>::format_decimal(BigDecimal::from(100))
        );

        // 小数点以下が全て0の値
        let with_zeros = BigDecimal::from(100) + BigDecimal::from_str("0.000000000").unwrap();
        assert_eq!(
            "100",
            ListStatsInPeriod::<BigDecimal>::format_decimal(with_zeros)
        );

        // 小数点以下が1桁の値
        assert_eq!(
            "0.1",
            ListStatsInPeriod::<BigDecimal>::format_decimal(BigDecimal::from_str("0.1").unwrap())
        );

        // 小数点以下が2桁の値
        assert_eq!(
            "0.12",
            ListStatsInPeriod::<BigDecimal>::format_decimal(BigDecimal::from_str("0.12").unwrap())
        );

        // 小数点以下が3桁の値
        assert_eq!(
            "0.123",
            ListStatsInPeriod::<BigDecimal>::format_decimal(BigDecimal::from_str("0.123").unwrap())
        );

        // 小数点以下が4桁の値
        assert_eq!(
            "0.1234",
            ListStatsInPeriod::<BigDecimal>::format_decimal(
                BigDecimal::from_str("0.1234").unwrap()
            )
        );

        // 小数点以下が5桁の値
        assert_eq!(
            "0.12345",
            ListStatsInPeriod::<BigDecimal>::format_decimal(
                BigDecimal::from_str("0.12345").unwrap()
            )
        );

        // 小数点以下が6桁の値
        assert_eq!(
            "0.123456",
            ListStatsInPeriod::<BigDecimal>::format_decimal(
                BigDecimal::from_str("0.123456").unwrap()
            )
        );

        // 小数点以下が7桁の値
        assert_eq!(
            "0.1234567",
            ListStatsInPeriod::<BigDecimal>::format_decimal(
                BigDecimal::from_str("0.1234567").unwrap()
            )
        );

        // 小数点以下が8桁の値
        assert_eq!(
            "0.12345678",
            ListStatsInPeriod::<BigDecimal>::format_decimal(
                BigDecimal::from_str("0.12345678").unwrap()
            )
        );

        // 小数点以下が9桁の値
        assert_eq!(
            "0.123456789",
            ListStatsInPeriod::<BigDecimal>::format_decimal(
                BigDecimal::from_str("0.123456789").unwrap()
            )
        );

        // 小数点以下が10桁の値（9桁までに制限される）
        assert_eq!(
            "0.123456789",
            ListStatsInPeriod::<BigDecimal>::format_decimal(
                BigDecimal::from_str("0.1234567891").unwrap()
            )
        );

        // 末尾に0がある場合（末尾の0は削除される）
        assert_eq!(
            "0.12345",
            ListStatsInPeriod::<BigDecimal>::format_decimal(
                BigDecimal::from_str("0.12345000").unwrap()
            )
        );

        // 整数部分あり、小数点以下4桁の値
        assert_eq!(
            "123.4567",
            ListStatsInPeriod::<BigDecimal>::format_decimal(
                BigDecimal::from_str("123.4567").unwrap()
            )
        );
    }
}
