//! 価格統計・取引処理モジュール
//!
//! ## 単位の規約
//!
//! このモジュールでは以下の単位規約を使用しています：
//!
//! - **rate_yocto**: yocto tokens per 1 NEAR（DB に保存される形式）
//! - **predictions (BTreeMap<String, TokenPrice>)**: 予測価格（NEAR/token 単位、型安全）
//! - **volatility**: 比率（単位なし）
//!
//! ## yocto スケールの利点
//!
//! DBには `rate_yocto = tokens_yocto / NEAR` を保存。
//! これにより使用時のスケーリング（× 10^24）が不要になり効率的。
//!
//! ## 単位変換（型安全）
//!
//! - NEAR → yoctoNEAR: `NearValue::from_near(bd).to_yocto().into_bigdecimal()`
//! - yoctoNEAR → NEAR: `YoctoValue::from_yocto(bd).to_near().into_bigdecimal()`

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
use crate::wallet::Wallet;
use bigdecimal::BigDecimal;
use chrono::{Duration, NaiveDateTime, Utc};
use futures_util::future::join_all;
use near_primitives::types::Balance;
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
use zaciraci_common::types::{
    ExchangeRate, NearAmount, NearValue, TokenAmount, TokenPrice, YoctoValue,
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

    // TRADE_ENABLED のチェック
    let trade_enabled = config::get("TRADE_ENABLED")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false);

    // Step 1: 評価期間のチェックと管理（清算が必要な場合は先に実行）
    // 初回起動時は available_funds=0 で呼び出し、後で prepare_funds() で資金準備
    let (period_id, is_new_period, existing_tokens, liquidated_balance) =
        manage_evaluation_period(0).await?;
    info!(log, "evaluation period status";
        "period_id" => %period_id,
        "is_new_period" => is_new_period,
        "existing_tokens_count" => existing_tokens.len(),
        "liquidated_balance" => ?liquidated_balance,
        "trade_enabled" => trade_enabled
    );

    // period_id が空の場合は清算のみで終了（manage_evaluation_period で停止された）
    if period_id.is_empty() {
        info!(log, "trade stopped after liquidation (TRADE_ENABLED=false)");
        return Ok(());
    }

    // 取引が無効化されている場合
    if !trade_enabled {
        if is_new_period {
            info!(log, "trade disabled, skipping new period");
            return Ok(());
        } else {
            // 評価期間中: 清算して終了
            info!(log, "trade disabled, liquidating positions");
            let _ = liquidate_all_positions().await?;
            return Ok(());
        }
    }

    // Step 2: 資金準備（新規期間で清算がなかった場合のみ）
    let available_funds = if is_new_period {
        if let Some(balance) = liquidated_balance {
            // 清算が行われた場合: 清算後の残高をそのまま使用（Option A）
            info!(log, "Using liquidated balance for new period"; "available_funds" => balance);
            if balance == 0 {
                info!(log, "no funds available after liquidation");
                return Ok(());
            }
            balance
        } else {
            // 初回起動: NEAR -> wrap.near 変換
            let funds = prepare_funds().await?;
            info!(log, "Prepared funds for new period"; "available_funds" => funds);

            if funds == 0 {
                info!(log, "no funds available for trading");
                return Ok(());
            }

            funds
        }
    } else {
        // 評価期間中: 既存トークンを継続使用、追加の資金準備は不要
        info!(log, "continuing evaluation period, skipping prepare_funds");
        0 // available_funds は使用されない
    };

    // Step 3: PredictionServiceの初期化
    let chronos_url =
        std::env::var("CHRONOS_URL").unwrap_or_else(|_| "http://localhost:8000".to_string());

    let prediction_service = PredictionService::new(chronos_url);

    // Step 4: トークン選定 (評価期間に応じて処理を分岐)
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

    // Step 4.5: REF Finance のストレージセットアップを確認・実行
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

    // Step 5: 投資額全額を REF Finance にデポジット (新規期間のみ)
    if is_new_period {
        info!(log, "depositing initial investment to REF Finance"; "amount" => available_funds);
        crate::ref_finance::balances::deposit_wrap_near_to_ref(&client, &wallet, available_funds)
            .await?;
        info!(log, "initial investment deposited to REF Finance");
    }

    // Step 6: ポートフォリオ戦略決定と実行
    // 新規期間も評価期間中も予測ベースの最適化を実行
    info!(log, "executing portfolio optimization";
        "is_new_period" => is_new_period,
        "token_count" => selected_tokens.len()
    );

    let report = match execute_portfolio_strategy(
        &prediction_service,
        &selected_tokens,
        available_funds,
        is_new_period,
        &client,
        &wallet,
    )
    .await
    {
        Ok(actions) => actions,
        Err(e) => {
            error!(log, "failed to execute portfolio strategy"; "error" => ?e);
            return Err(e);
        }
    };

    info!(log, "portfolio optimization completed";
        "action_count" => report.len()
    );

    // 実際の取引実行
    let executed_actions =
        execute_trading_actions(&report, available_funds, period_id.clone()).await?;
    info!(log, "trades executed"; "success" => executed_actions.success_count, "failed" => executed_actions.failed_count);

    // Step 7: ハーベスト判定と実行
    let portfolio_value = YoctoValue::from_yocto(BigDecimal::from(available_funds));
    check_and_harvest(portfolio_value).await?;

    info!(log, "success");
    Ok(())
}

/// 資金準備 (NEAR -> wrap.near 変換)
async fn prepare_funds() -> Result<u128> {
    let log = DEFAULT.new(o!("function" => "prepare_funds"));

    // JSONRPCクライアントとウォレットを取得
    let client = crate::jsonrpc::new_client();
    let wallet = crate::wallet::new_wallet();

    // 初期投資額の設定値を取得（NEAR単位で入力、yoctoNEARに変換）
    let target_investment: u128 = config::get("TRADE_INITIAL_INVESTMENT")
        .ok()
        .and_then(|v| v.parse::<NearAmount>().ok())
        .unwrap_or_else(|| "100".parse().unwrap())
        .to_yocto()
        .to_u128();

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

    // 過去7日間のボラティリティトークンを全て取得（DBから）
    let end_date = Utc::now();
    let start_date = end_date - chrono::Duration::days(7);

    match prediction_service
        .get_tokens_by_volatility(start_date, end_date, "wrap.near")
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

            // 流動性フィルタリング: REF Finance で現在取引可能なトークンのみを選択
            let pools = crate::ref_finance::pool_info::PoolInfoList::read_from_db(None).await?;
            let graph = crate::ref_finance::path::graph::TokenGraph::new(pools);
            let wnear_token: crate::ref_finance::token_account::TokenInAccount =
                crate::ref_finance::token_account::WNEAR_TOKEN
                    .clone()
                    .into();

            // 購入方向のパスを確認（wrap.near → token）
            let buyable_tokens = match graph.update_graph(&wnear_token) {
                Ok(goals) => {
                    let token_ids: std::collections::HashSet<_> =
                        goals.iter().map(|t| t.as_id().to_string()).collect();
                    info!(log, "buyable tokens (wrap.near → token)";
                        "count" => token_ids.len(),
                    );
                    token_ids
                }
                Err(e) => {
                    warn!(log, "failed to get buyable tokens, using all volatility tokens";
                        "error" => ?e,
                    );
                    // フィルタリング失敗時は全トークンを返す
                    return Ok(tokens);
                }
            };

            // volatility トークンを購入可能性でフィルタ
            let original_count = tokens.len();
            let buyable_filtered: Vec<AccountId> = tokens
                .into_iter()
                .filter(|token| buyable_tokens.contains(&token.to_string()))
                .collect();

            info!(log, "tokens after buyability filtering";
                "original_count" => original_count,
                "buyable_count" => buyable_filtered.len(),
            );

            // 売却方向のパスも確認（token → wrap.near）
            let wnear_out: crate::ref_finance::token_account::TokenOutAccount =
                crate::ref_finance::token_account::WNEAR_TOKEN
                    .clone()
                    .into();
            let mut filtered_tokens: Vec<AccountId> = Vec::new();
            for token in buyable_filtered {
                let token_account: crate::ref_finance::token_account::TokenAccount =
                    match token.to_string().parse() {
                        Ok(t) => t,
                        Err(_) => continue,
                    };
                let token_in: crate::ref_finance::token_account::TokenInAccount =
                    token_account.into();

                // token から wrap.near へのパスが存在するか確認
                match graph.update_graph(&token_in) {
                    Ok(sellable_goals) => {
                        if sellable_goals
                            .iter()
                            .any(|g| g.as_id() == wnear_out.as_id())
                        {
                            filtered_tokens.push(token);

                            // 必要な数に達したら即座に終了
                            if filtered_tokens.len() >= limit {
                                info!(log, "reached required token count, stopping early"; "count" => limit);
                                break;
                            }
                        } else {
                            info!(log, "token not sellable to wrap.near, skipping"; "token" => %token);
                        }
                    }
                    Err(_) => {
                        info!(log, "failed to check sellability, skipping"; "token" => %token);
                    }
                }
            }

            if filtered_tokens.is_empty() {
                return Err(anyhow::anyhow!(
                    "No tokens with sufficient liquidity after filtering {} volatility tokens",
                    original_count
                ));
            }

            // 要求された数に制限（フィルタ後の上位 limit 個を返す）
            if filtered_tokens.len() > limit {
                filtered_tokens.truncate(limit);
            }

            if filtered_tokens.len() < limit {
                warn!(log, "insufficient tokens after liquidity filtering";
                    "required" => limit,
                    "available" => filtered_tokens.len(),
                    "fetched" => original_count,
                );
            }

            info!(log, "tokens after liquidity filtering";
                "original_count" => original_count,
                "filtered_count" => filtered_tokens.len(),
                "required_count" => limit,
            );

            Ok(filtered_tokens)
        }
        Err(e) => {
            error!(log, "failed to get tokens from prediction service"; "error" => ?e);
            Err(anyhow::anyhow!("Failed to get volatility tokens: {}", e))
        }
    }
}

/// ポートフォリオ戦略の実行
///
/// # 引数
/// * `prediction_service` - 価格予測サービス
/// * `tokens` - 対象トークンのアカウントID
/// * `available_funds` - 利用可能資金（yoctoNEAR単位）
/// * `is_new_period` - 新しい評価期間かどうか
/// * `client` - RPCクライアント
/// * `wallet` - ウォレット
///
/// # 内部の単位
/// * 価格: Price型（無次元比率）をスケーリング（× 10^24）してu128に格納
/// * 予測: 同じスケーリング済みf64値
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

        // 現在価格を履歴から取得（TokenPrice: NEAR/token 単位）
        let current_price = if let Some(latest_price) = history.prices.last() {
            latest_price.price.clone()
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

        // 予測の取得（TokenPrice: NEAR/token 単位）
        let predicted_price = match prediction_service.predict_price(&predict_history, 24).await {
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

        // 予測価格を TokenPrice で保存（型安全）
        predictions.insert(token.to_string(), predicted_price.clone());

        // 相対リターンの計算（expected_return メソッドを使用）
        // price 形式なので、予測価格 > 現在価格 = 正のリターン
        let expected_price_return_pct = current_price.expected_return(&predicted_price) * 100.0;

        info!(log, "token prediction";
            "token" => %token,
            "current_price" => %current_price,
            "predicted_price" => %predicted_price,
            "expected_price_return_pct" => format!("{:.2}%", expected_price_return_pct)
        );

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

        // トークンの decimals を取得
        let decimals = get_token_decimals(client, &token_str).await;

        // 市場規模の推定（実際の発行量データを取得）
        let market_cap =
            estimate_market_cap_async(client, &token_str, &current_price, decimals).await;

        token_data.push(TokenData {
            symbol: token.to_string(),
            current_rate: ExchangeRate::from_price(&current_price, decimals),
            historical_volatility: volatility_f64,
            liquidity_score: Some(liquidity_score),
            market_cap: Some(market_cap),
        });
    }

    let portfolio_data = PortfolioData {
        tokens: token_data,
        predictions,
        historical_prices,
        correlation_matrix: None,
    };

    // yoctoNEARからNEARに変換（型安全、BigDecimal精度維持）
    let total_value_near = YoctoValue::from_yocto(BigDecimal::from(available_funds)).to_near();

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

        // get_current_portfolio_balances は TokenAmount を返すので、ゼロを除外するだけ
        let mut holdings_typed = BTreeMap::new();
        for (token, amount) in current_balances {
            if !amount.is_zero() {
                info!(log, "loaded existing position"; "token" => &token, "amount" => %amount);
                holdings_typed.insert(token, amount);
            }
        }
        holdings_typed
    };

    let wallet_info = WalletInfo {
        holdings,
        total_value: total_value_near.clone(),
        cash_balance: total_value_near,
    };

    // ポートフォリオ最適化の実行
    let execution_report = execute_portfolio_optimization(
        &wallet_info,
        portfolio_data,
        0.1, // rebalance threshold
    )
    .await?;

    info!(log, "portfolio optimization completed";
        "actions" => execution_report.actions.len(),
        "rebalance_needed" => execution_report.rebalance_needed,
        "expected_return" => execution_report.optimal_weights.expected_return,
        "expected_volatility" => execution_report.optimal_weights.expected_volatility,
        "sharpe_ratio" => execution_report.optimal_weights.sharpe_ratio
    );

    for (token, weight) in &execution_report.optimal_weights.weights {
        info!(log, "optimal weight";
            "token" => token,
            "weight" => weight,
            "percentage" => format!("{:.2}%", weight * 100.0)
        );
    }

    Ok(execution_report.actions)
}

/// 取引アクションを実際に実行
async fn execute_trading_actions(
    actions: &[TradingAction],
    _available_funds: u128,
    period_id: String,
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
    let recorder = TradeRecorder::new(period_id.clone());
    info!(log, "created trade recorder";
        "batch_id" => recorder.get_batch_id(),
        "period_id" => %period_id
    );

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
                swap::execute_direct_swap(
                    client,
                    wallet,
                    token,
                    &wrap_near.to_string(),
                    None,
                    recorder,
                )
                .await?;
            }

            // Step 2: wrap.near → target
            if target != &wrap_near.to_string() {
                swap::execute_direct_swap(
                    client,
                    wallet,
                    &wrap_near.to_string(),
                    target,
                    None,
                    recorder,
                )
                .await?;
            }

            info!(log, "sell completed"; "from" => token, "to" => target);
            Ok(())
        }
        TradingAction::Switch { from, to } => {
            // from から to へ切り替え（直接スワップ）
            info!(log, "executing switch"; "from" => from, "to" => to);

            swap::execute_direct_swap(client, wallet, from, to, None, recorder).await?;

            info!(log, "switch completed"; "from" => from, "to" => to);
            Ok(())
        }
        TradingAction::Rebalance { target_weights } => {
            // ポートフォリオのリバランス
            info!(log, "executing rebalance"; "weights" => ?target_weights);

            // 現在の保有量を取得（wrap.nearを明示的に追加）
            let mut tokens: Vec<String> = target_weights.keys().cloned().collect();
            let wrap_near = crate::ref_finance::token_account::WNEAR_TOKEN.to_string();
            if !tokens.contains(&wrap_near) {
                tokens.push(wrap_near.clone());
                info!(
                    log,
                    "added wrap.near to balance query for total value calculation"
                );
            }
            info!(log, "tokens list for balance query"; "tokens" => ?tokens, "count" => tokens.len());

            let current_balances =
                crate::trade::swap::get_current_portfolio_balances(client, wallet, &tokens).await?;

            // 総ポートフォリオ価値を計算
            let total_portfolio_value = crate::trade::swap::calculate_total_portfolio_value(
                client,
                wallet,
                &current_balances,
            )
            .await?;

            // Phase 1と2に分けてリバランスを実行
            // まず各トークンの差分（wrap.near換算）を計算
            use num_bigint::ToBigInt;

            let mut sell_operations: Vec<(String, NearValue, ExchangeRate)> = Vec::new();
            let mut buy_operations: Vec<(String, NearValue)> = Vec::new();

            let wrap_near_str = crate::ref_finance::token_account::WNEAR_TOKEN.to_string();

            for (token, target_weight) in target_weights.iter() {
                if token == &wrap_near_str {
                    continue; // wrap.nearは除外
                }

                let current_amount = current_balances.get(token);

                // 現在の価値（wrap.near換算）を計算
                let current_value_wrap_near: NearValue = match current_amount {
                    Some(amount) if !amount.is_zero() => {
                        let token_out: crate::ref_finance::token_account::TokenOutAccount =
                            token.parse::<near_sdk::AccountId>()?.into();
                        let quote_in: crate::ref_finance::token_account::TokenInAccount =
                            wrap_near_str.parse::<near_sdk::AccountId>()?.into();

                        let rate = crate::persistence::token_rate::TokenRate::get_latest(
                            &token_out, &quote_in,
                        )
                        .await?
                        .ok_or_else(|| anyhow::anyhow!("No rate found for token: {}", token))?;

                        // TokenAmount / &ExchangeRate = NearValue トレイトを使用
                        amount / &rate.exchange_rate
                    }
                    _ => NearValue::zero(),
                };

                // 目標価値（wrap.near換算）を計算
                // target_weight は f64 (0.0~1.0)、例: 0.3 = ポートフォリオの30%
                let target_value_wrap_near: NearValue = &total_portfolio_value * *target_weight;

                // 差分を計算（wrap.near単位）
                let diff_wrap_near: NearValue = &target_value_wrap_near - &current_value_wrap_near;

                info!(log, "rebalancing: token analysis";
                    "token" => token,
                    "current_value_wrap_near" => %current_value_wrap_near,
                    "target_value_wrap_near" => %target_value_wrap_near,
                    "diff_wrap_near" => %diff_wrap_near
                );

                // 最小交換額チェック（1 NEAR以上）
                let min_trade_size = NearValue::one();
                let zero = NearValue::zero();

                if diff_wrap_near < zero && diff_wrap_near.abs() >= min_trade_size {
                    // 売却が必要
                    let token_out: crate::ref_finance::token_account::TokenOutAccount =
                        token.parse::<near_sdk::AccountId>()?.into();
                    let quote_in: crate::ref_finance::token_account::TokenInAccount =
                        wrap_near_str.parse::<near_sdk::AccountId>()?.into();

                    let rate = crate::persistence::token_rate::TokenRate::get_latest(
                        &token_out, &quote_in,
                    )
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("No rate found for token: {}", token))?;

                    sell_operations.push((
                        token.clone(),
                        diff_wrap_near.abs(),
                        rate.exchange_rate.clone(),
                    ));
                } else if diff_wrap_near > zero && diff_wrap_near >= min_trade_size {
                    // 購入が必要
                    buy_operations.push((token.clone(), diff_wrap_near));
                }
            }

            // Phase 1: 全ての売却を実行（token → wrap.near）
            info!(log, "Phase 1: executing sell operations"; "count" => sell_operations.len());
            for (token, wrap_near_value, exchange_rate) in sell_operations {
                // wrap.near価値をトークン数量に変換
                // NearValue * ExchangeRate = TokenAmount
                let token_amount: TokenAmount = &wrap_near_value * &exchange_rate;
                let token_amount_u128 = token_amount
                    .smallest_units()
                    .to_bigint()
                    .ok_or_else(|| anyhow::anyhow!("Failed to convert to BigInt"))?
                    .to_string()
                    .parse::<u128>()
                    .map_err(|e| anyhow::anyhow!("Failed to parse as u128: {}", e))?;

                info!(log, "selling token";
                    "token" => &token,
                    "wrap_near_value" => %wrap_near_value,
                    "token_amount" => token_amount_u128
                );

                swap::execute_direct_swap(
                    client,
                    wallet,
                    &token,
                    &wrap_near_str,
                    Some(token_amount_u128),
                    recorder,
                )
                .await?;
            }

            // Phase 1完了後、利用可能なwrap.nearを確認し、Phase 2の購入額を調整
            let available_wrap_near = {
                let account = wallet.account_id();
                let deposits = crate::ref_finance::deposit::get_deposits(client, account).await?;
                let wrap_near_account: crate::ref_finance::token_account::TokenAccount =
                    wrap_near_str.parse::<near_sdk::AccountId>()?.into();
                deposits
                    .get(&wrap_near_account)
                    .map(|u| u.0)
                    .unwrap_or_default()
            };

            info!(log, "Phase 1 completed, checking available wrap.near";
                "available_wrap_near" => %available_wrap_near
            );

            // available_wrap_near (u128) を NearValue に変換
            let available_wrap_near_value =
                YoctoValue::from_yocto(BigDecimal::from(available_wrap_near)).to_near();

            // Phase 2の購入操作の総額を計算
            let total_buy_value: NearValue = buy_operations
                .iter()
                .map(|(_, value)| value.clone())
                .fold(NearValue::zero(), |acc, v| acc + v);

            info!(log, "Phase 2 purchase amount analysis";
                "total_buy_value" => %total_buy_value,
                "available_wrap_near_value" => %available_wrap_near_value
            );

            // 利用可能残高に基づいて購入額を調整
            let adjusted_buy_operations: Vec<(String, NearValue)> =
                if total_buy_value > available_wrap_near_value {
                    // 比率を計算して調整（型安全な除算演算子を使用）
                    let ratio = &available_wrap_near_value / &total_buy_value;
                    info!(log, "Adjusting purchase amounts to fit available balance";
                        "adjustment_factor" => %ratio
                    );

                    buy_operations
                        .into_iter()
                        .map(|(token, value)| (token, &value * &ratio))
                        .collect()
                } else {
                    buy_operations
                };

            // Phase 2: 全ての購入を実行（wrap.near → token）
            info!(log, "Phase 2: executing buy operations"; "count" => adjusted_buy_operations.len());

            let mut phase2_success = 0;
            let mut phase2_failed = 0;

            for (token, wrap_near_value) in adjusted_buy_operations {
                // NearValue → YoctoValue → YoctoAmount → u128 に変換
                let wrap_near_amount_u128 = wrap_near_value.to_yocto().to_amount().to_u128();

                if wrap_near_amount_u128 == 0 {
                    error!(log, "Failed to convert purchase amount to u128"; "token" => &token);
                    phase2_failed += 1;
                    continue;
                }

                info!(log, "buying token";
                    "token" => &token,
                    "wrap_near_amount" => wrap_near_amount_u128
                );

                match swap::execute_direct_swap(
                    client,
                    wallet,
                    &wrap_near_str,
                    &token,
                    Some(wrap_near_amount_u128),
                    recorder,
                )
                .await
                {
                    Ok(_) => {
                        info!(log, "purchase completed successfully"; "token" => &token);
                        phase2_success += 1;
                    }
                    Err(e) => {
                        error!(log, "purchase failed"; "token" => &token, "error" => %e);
                        phase2_failed += 1;
                    }
                }
            }

            info!(log, "rebalance completed";
                "phase2_success" => phase2_success,
                "phase2_failed" => phase2_failed
            );

            // Phase 2で全ての購入が失敗した場合のみエラーを返す
            if phase2_success == 0 && phase2_failed > 0 {
                return Err(anyhow::anyhow!(
                    "All Phase 2 purchases failed ({} failed)",
                    phase2_failed
                ));
            }

            Ok(())
        }
        TradingAction::AddPosition { token, weight } => {
            // ポジション追加
            info!(log, "adding position"; "token" => token, "weight" => weight);

            // wrap.near → token へのswap
            let wrap_near = &crate::ref_finance::token_account::WNEAR_TOKEN;
            if token != &wrap_near.to_string() {
                swap::execute_direct_swap(
                    client,
                    wallet,
                    &wrap_near.to_string(),
                    token,
                    None,
                    recorder,
                )
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
                swap::execute_direct_swap(
                    client,
                    wallet,
                    token,
                    &wrap_near.to_string(),
                    None,
                    recorder,
                )
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
async fn check_and_harvest(current_portfolio_value: YoctoValue) -> Result<()> {
    // 実際のハーベスト機能を呼び出す
    // 注: 評価期間中は available_funds = 0 が渡されるため、ハーベスト判定はスキップされる
    // 評価期間終了時（清算後）のみ、liquidated_balance が渡され、ハーベスト判定が実行される
    crate::trade::harvest::check_and_harvest(current_portfolio_value).await
}

/// 評価期間のチェックと管理
///
/// 戻り値: (period_id, is_new_period, selected_tokens, liquidated_balance)
/// - liquidated_balance: 清算が行われた場合の最終残高（yoctoNEAR）
async fn manage_evaluation_period(
    available_funds: u128,
) -> Result<(String, bool, Vec<String>, Option<Balance>)> {
    let log = DEFAULT.new(o!("function" => "manage_evaluation_period"));

    // 設定ファイルから評価期間を読み込む（デフォルト: 10日）
    let evaluation_period_days = config::get("TRADE_EVALUATION_DAYS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(10);

    info!(log, "evaluation period configuration"; "days" => evaluation_period_days);

    // 最新の評価期間を取得
    let latest_period = EvaluationPeriod::get_latest_async().await?;

    match latest_period {
        Some(period) => {
            let now = Utc::now().naive_utc();
            let period_duration = now.signed_duration_since(period.start_time);

            if period_duration.num_days() >= evaluation_period_days {
                // 評価期間終了: 全トークンを売却して新規期間を開始
                info!(log, "evaluation period ended, starting new period";
                    "previous_period_id" => %period.period_id,
                    "days_elapsed" => period_duration.num_days()
                );

                // 全トークンをwrap.nearに売却
                let final_balance = liquidate_all_positions().await?;
                info!(log, "liquidated all positions"; "final_balance" => %final_balance);

                // 評価期間のパフォーマンスを計算してログ出力
                let previous_initial_value = period.initial_value.clone();
                let final_value = BigDecimal::from(final_balance);
                let change_amount = &final_value - &previous_initial_value;
                let change_percentage = if previous_initial_value > BigDecimal::from(0) {
                    (&change_amount / &previous_initial_value) * BigDecimal::from(100)
                } else {
                    BigDecimal::from(0)
                };

                info!(log, "evaluation period performance";
                    "period_id" => %period.period_id,
                    "initial_value" => %previous_initial_value,
                    "final_value" => %final_value,
                    "change_amount" => %change_amount,
                    "change_percentage" => %format!("{:.2}%", change_percentage)
                );

                // TRADE_ENABLED をチェック
                let trade_enabled = config::get("TRADE_ENABLED")
                    .map(|v| v.to_lowercase() == "true")
                    .unwrap_or(false);

                if !trade_enabled {
                    info!(log, "trade disabled, not starting new period";
                        "final_balance" => %final_balance
                    );
                    // 空の period_id を返して停止を通知
                    return Ok((String::new(), false, vec![], Some(final_balance)));
                }

                // 新規評価期間を作成
                let new_period = NewEvaluationPeriod::new(final_value.clone(), vec![]);
                let created_period = new_period.insert_async().await?;

                info!(log, "created new evaluation period";
                    "period_id" => %created_period.period_id,
                    "initial_value" => %created_period.initial_value
                );

                Ok((created_period.period_id, true, vec![], Some(final_balance)))
            } else {
                // 評価期間中: トランザクション記録で判定
                info!(log, "checking evaluation period status";
                    "period_id" => %period.period_id,
                    "days_remaining" => evaluation_period_days - period_duration.num_days()
                );

                // トランザクション記録をチェック
                use crate::persistence::trade_transaction::TradeTransaction;
                let transaction_count =
                    TradeTransaction::count_by_evaluation_period_async(period.period_id.clone())
                        .await?;

                info!(log, "transaction count for period";
                    "count" => transaction_count,
                    "period_id" => %period.period_id
                );

                let selected_tokens = period
                    .selected_tokens
                    .unwrap_or_default()
                    .into_iter()
                    .flatten()
                    .collect();

                // トランザクションがゼロなら新規期間として扱う
                let is_new_period = transaction_count == 0;

                if is_new_period {
                    info!(
                        log,
                        "no transactions found in period, treating as new period"
                    );
                } else {
                    info!(log, "continuing evaluation period with existing positions";
                        "transaction_count" => transaction_count
                    );
                }

                Ok((period.period_id, is_new_period, selected_tokens, None))
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

            Ok((created_period.period_id, true, vec![], None))
        }
    }
}

/// REF Financeの残高から清算対象トークンをフィルタリング
///
/// wrap.nearとゼロ残高のトークンを除外し、清算すべきトークンのリストを返す
fn filter_tokens_to_liquidate(
    deposits: &HashMap<crate::ref_finance::token_account::TokenAccount, near_sdk::json_types::U128>,
    wrap_near_token: &crate::ref_finance::token_account::TokenAccount,
) -> Vec<String> {
    deposits
        .iter()
        .filter_map(|(token, amount)| {
            // wrap.nearは除外し、残高があるトークンのみを対象とする
            if token != wrap_near_token && amount.0 > 0 {
                Some(token.to_string())
            } else {
                None
            }
        })
        .collect()
}

/// 全保有トークンをwrap.nearに売却
///
/// 戻り値: 売却後のwrap.near総額 (yoctoNEAR)
async fn liquidate_all_positions() -> Result<Balance> {
    let log = DEFAULT.new(o!("function" => "liquidate_all_positions"));

    // 最新の評価期間を取得
    let latest_period = EvaluationPeriod::get_latest_async().await?;
    let period_id = match latest_period {
        Some(period) => {
            // selected_tokensは履歴として記録（実際の清算には使用しない）
            let selected_tokens = period
                .selected_tokens
                .unwrap_or_default()
                .into_iter()
                .flatten()
                .collect::<Vec<String>>();
            info!(log, "evaluation period selected tokens";
                  "period_id" => &period.period_id,
                  "selected_tokens" => ?selected_tokens);
            period.period_id
        }
        None => {
            info!(log, "no evaluation period found, nothing to liquidate");
            return Ok(0);
        }
    };

    // 実際のREF Finance残高を取得して清算対象を決定
    let client = crate::jsonrpc::new_client();
    let wallet = crate::wallet::new_wallet();
    let account = wallet.account_id();
    let wrap_near_token = &crate::ref_finance::token_account::WNEAR_TOKEN;

    let deposits = crate::ref_finance::deposit::get_deposits(&client, account).await?;
    let tokens_to_liquidate = filter_tokens_to_liquidate(&deposits, wrap_near_token);

    if tokens_to_liquidate.is_empty() {
        info!(log, "no tokens to liquidate");
        // wrap.nearの残高を返す
        let wrap_near = &crate::ref_finance::token_account::WNEAR_TOKEN;
        let balance = deposits.get(wrap_near).map(|u| u.0).unwrap_or_default();
        return Ok(balance);
    }

    info!(log, "liquidating positions"; "token_count" => tokens_to_liquidate.len());

    // トレードレコーダーを作成
    let recorder = TradeRecorder::new(period_id);

    // 各トークンをwrap.nearに変換
    for token in &tokens_to_liquidate {
        info!(log, "liquidating token"; "token" => token);

        // トークンの残高を再確認（取得時点から変更がある可能性を考慮）
        let token_account: crate::ref_finance::token_account::TokenAccount = token
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid token: {}", e))?;

        // トークンの REF Finance 上の残高を取得
        let account = wallet.account_id();
        let deposits = crate::ref_finance::deposit::get_deposits(&client, account).await?;
        let balance = deposits
            .get(&token_account)
            .map(|u| u.0)
            .unwrap_or_default();

        if balance == 0 {
            info!(log, "token balance became zero, skipping"; "token" => token);
            continue;
        }

        // token → wrap.near にスワップ
        let wrap_near_str = wrap_near_token.to_string();
        match swap::execute_direct_swap(&client, &wallet, token, &wrap_near_str, None, &recorder)
            .await
        {
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
    let account = wallet.account_id();
    let deposits = crate::ref_finance::deposit::get_deposits(&client, account).await?;
    let wrap_near = &crate::ref_finance::token_account::WNEAR_TOKEN;
    let final_balance = deposits.get(wrap_near).map(|u| u.0).unwrap_or_default();

    info!(log, "liquidation complete"; "final_wrap_near_balance" => final_balance);
    Ok(final_balance)
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
///
/// # Arguments
/// * `client` - RPC クライアント
/// * `token_id` - トークン ID
/// * `price` - 価格（TokenPrice: NEAR/token）
/// * `decimals` - トークンの decimals
///
/// # Returns
/// * `NearValue` - 時価総額（NEAR 単位）
///
/// # 計算式
/// ```text
/// total_supply (TokenAmount) = get_token_total_supply(client, token_id, decimals)
/// market_cap (NearValue) = total_supply × price
/// ```
async fn estimate_market_cap_async<C>(
    client: &C,
    token_id: &str,
    price: &TokenPrice,
    decimals: u8,
) -> NearValue
where
    C: crate::jsonrpc::ViewContract,
{
    // 実際の発行量データを取得（TokenAmount）
    let total_supply = get_token_total_supply(client, token_id, decimals)
        .await
        .unwrap_or_else(|_| {
            // 取得失敗時は 10^24 smallest units と仮定
            TokenAmount::from_smallest_units(
                BigDecimal::from_str("1000000000000000000000000").unwrap(), // 10^24 smallest units
                decimals,
            )
        });

    if price.is_zero() {
        // デフォルト値: 10,000 NEAR
        return NearValue::from_near(BigDecimal::from(10000));
    }

    // market_cap (NearValue) = TokenAmount × TokenPrice
    &total_supply * price
}

/// トークンの総発行量を取得
///
/// # Arguments
/// * `client` - RPC クライアント
/// * `token_id` - トークン ID
/// * `decimals` - トークンの decimals
///
/// # Returns
/// * `TokenAmount` - 総発行量（smallest_units + decimals）
async fn get_token_total_supply<C>(client: &C, token_id: &str, decimals: u8) -> Result<TokenAmount>
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
        let smallest_units = BigDecimal::from_str(total_supply_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse total supply: {}", e))?;
        Ok(TokenAmount::from_smallest_units(smallest_units, decimals))
    } else {
        Err(anyhow::anyhow!(
            "Expected string value for total supply, got: {:?}",
            json_value
        ))
    }
}

/// トークンメタデータ（NEP-148 準拠）
#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)] // デシリアライズ用に全フィールド必要
pub struct TokenMetadata {
    pub spec: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub reference: Option<String>,
    #[serde(default)]
    pub reference_hash: Option<String>,
}

/// トークンのメタデータを取得（ft_metadata）
pub async fn get_token_metadata<C>(client: &C, token_id: &str) -> Result<TokenMetadata>
where
    C: crate::jsonrpc::ViewContract,
{
    use near_sdk::AccountId;

    let account_id: AccountId = token_id
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid token account ID: {}", e))?;

    let args = serde_json::json!({});
    let result = client
        .view_contract(&account_id, "ft_metadata", &args)
        .await?;

    // resultフィールドからJSONデータを取得してパース
    let metadata: TokenMetadata = serde_json::from_slice(&result.result)
        .map_err(|e| anyhow::anyhow!("Failed to parse token metadata: {}", e))?;

    Ok(metadata)
}

/// トークンの decimals を取得（キャッシュなし、エラー時は 24 を返す）
pub async fn get_token_decimals<C>(client: &C, token_id: &str) -> u8
where
    C: crate::jsonrpc::ViewContract,
{
    match get_token_metadata(client, token_id).await {
        Ok(metadata) => metadata.decimals,
        Err(_) => 24, // デフォルト値（NEAR ネイティブトークンの decimals）
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
                        rate: r.rate().clone(),
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
mod tests;
