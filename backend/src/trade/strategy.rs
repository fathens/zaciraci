//! 取引戦略オーケストレータモジュール
//!
//! ポートフォリオベースの取引戦略のエントリポイント。
//! 資金準備、トークン選定、ポートフォリオ最適化、取引実行の
//! ワークフロー全体を統括する。
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
//! - NEAR → yoctoNEAR: `NearValue::from_near(bd).to_yocto().as_bigdecimal()`
//! - yoctoNEAR → NEAR: `YoctoValue::from_yocto(bd).to_near().as_bigdecimal()`

use crate::Result;
use crate::config;
use crate::logging::*;
use crate::persistence::evaluation_period::EvaluationPeriod;
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use crate::trade::predict::PredictionService;
use crate::trade::swap;
use crate::wallet::Wallet;
use bigdecimal::BigDecimal;
use chrono::Utc;
use near_sdk::AccountId;
use std::collections::BTreeMap;
use zaciraci_common::algorithm::{
    portfolio::{PortfolioData, execute_portfolio_optimization},
    types::{TokenData, TradingAction, WalletInfo},
};
use zaciraci_common::types::{
    ExchangeRate, NearAmount, NearValue, TokenPrice, YoctoAmount, YoctoValue,
};

use super::execution::{
    execute_trading_actions, liquidate_all_positions, manage_evaluation_period,
};
use super::market_data::{
    calculate_enhanced_liquidity_score, calculate_volatility_from_history,
    estimate_market_cap_async,
};

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
        manage_evaluation_period(YoctoAmount::zero()).await?;
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
    let available_funds: YoctoAmount = if is_new_period {
        if let Some(balance) = liquidated_balance {
            // 清算が行われた場合: 清算後の残高をそのまま使用
            debug!(log, "Using liquidated balance for new period"; "available_funds" => %balance);
            if balance.is_zero() {
                info!(log, "no funds available after liquidation");
                return Ok(());
            }
            balance
        } else {
            // 初回起動: NEAR -> wrap.near 変換
            let funds = prepare_funds().await?;
            debug!(log, "Prepared funds for new period"; "available_funds" => %funds);

            if funds.is_zero() {
                info!(log, "no funds available for trading");
                return Ok(());
            }

            funds
        }
    } else {
        // 評価期間中: 既存トークンを継続使用、追加の資金準備は不要
        debug!(log, "continuing evaluation period, skipping prepare_funds");
        YoctoAmount::zero() // available_funds は使用されない
    };

    // Step 3: PredictionServiceの初期化
    let prediction_service = PredictionService::new();

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
                    debug!(log, "updated selected tokens in database"; "count" => tokens.len());
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

    debug!(log, "Selected tokens"; "count" => selected_tokens.len(), "is_new_period" => is_new_period);

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

    debug!(log, "ensuring REF Finance storage setup"; "token_count" => token_accounts.len());
    crate::ref_finance::storage::ensure_ref_storage_setup(&client, &wallet, &token_accounts)
        .await?;
    debug!(log, "REF Finance storage setup completed");

    // Step 5: 投資額全額を REF Finance にデポジット (新規期間のみ)
    if is_new_period {
        debug!(log, "depositing initial investment to REF Finance"; "amount" => %available_funds);
        crate::ref_finance::balances::deposit_wrap_near_to_ref(
            &client,
            &wallet,
            available_funds.to_u128(),
        )
        .await?;
        debug!(log, "initial investment deposited to REF Finance");
    }

    // Step 6: ポートフォリオ戦略決定と実行
    // 新規期間も評価期間中も予測ベースの最適化を実行
    debug!(log, "executing portfolio optimization";
        "is_new_period" => is_new_period,
        "token_count" => selected_tokens.len()
    );

    let report = match execute_portfolio_strategy(
        &prediction_service,
        &selected_tokens,
        available_funds.to_u128(),
        is_new_period,
        &period_id,
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
        execute_trading_actions(&report, available_funds.to_u128(), period_id.clone()).await?;
    info!(log, "trades executed"; "success" => executed_actions.success_count, "failed" => executed_actions.failed_count);

    // Step 7: ハーベスト判定と実行
    // YoctoAmount → YoctoValue（NEAR は数量=価値）
    check_and_harvest(available_funds.to_value()).await?;

    info!(log, "success");
    Ok(())
}

/// 資金準備 (NEAR -> wrap.near 変換)
async fn prepare_funds() -> Result<YoctoAmount> {
    let log = DEFAULT.new(o!("function" => "prepare_funds"));

    // JSONRPCクライアントとウォレットを取得
    let client = crate::jsonrpc::new_client();
    let wallet = crate::wallet::new_wallet();

    // 初期投資額の設定値を取得（NEAR単位で入力、yoctoNEARに変換）
    let target_investment: YoctoAmount = config::get("TRADE_INITIAL_INVESTMENT")
        .ok()
        .and_then(|v| v.parse::<NearAmount>().ok())
        .unwrap_or_else(|| "100".parse().unwrap())
        .to_yocto();

    // 必要な wrap.near 残高として投資額を設定（NEAR -> wrap.near変換）
    // アカウントには10 NEARを残し、それ以外を wrap.near に変換
    let required_balance = target_investment.to_u128();
    let account_id = wallet.account_id();
    let balance = crate::ref_finance::balances::start(
        &client,
        &wallet,
        &crate::ref_finance::token_account::WNEAR_TOKEN,
        Some(required_balance),
    )
    .await?;

    // wrap.near の全額が投資可能
    // 設定された投資額と実際の残高の小さい方を使用
    let balance_amount = YoctoAmount::from_u128(balance);
    let available_funds = if balance_amount < target_investment {
        balance_amount
    } else {
        target_investment
    };

    if available_funds.is_zero() {
        return Err(anyhow::anyhow!(
            "Insufficient wrap.near balance for trading: {} yoctoNEAR",
            balance
        ));
    }

    debug!(log, "prepared funds";
        "account" => %account_id,
        "wrap_near_balance" => balance,
        "available_funds" => %available_funds
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

    // ボラティリティトークンを全て取得（DBから）
    let volatility_days = config::get("TRADE_VOLATILITY_DAYS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(7);
    let end_date = Utc::now();
    let start_date = end_date - chrono::Duration::days(volatility_days);

    // 型安全な quote_token を準備
    let quote_token: crate::ref_finance::token_account::TokenInAccount =
        crate::ref_finance::token_account::WNEAR_TOKEN.to_in();

    match prediction_service
        .get_tokens_by_volatility(start_date, end_date, &quote_token)
        .await
    {
        Ok(top_tokens) => {
            // TopTokenInfo を AccountId に変換
            let tokens: Vec<AccountId> = top_tokens
                .into_iter()
                .map(|token| token.token.into())
                .collect();

            if tokens.is_empty() {
                return Err(anyhow::anyhow!(
                    "No volatility tokens returned from prediction service"
                ));
            }

            debug!(log, "selected tokens from prediction service"; "count" => tokens.len());

            // 流動性フィルタリング: REF Finance で現在取引可能なトークンのみを選択
            let pools = crate::ref_finance::pool_info::PoolInfoList::read_from_db(None).await?;
            let graph = crate::ref_finance::path::graph::TokenGraph::new(pools);
            let wnear_token: crate::ref_finance::token_account::TokenInAccount =
                crate::ref_finance::token_account::WNEAR_TOKEN.to_in();

            // 購入方向のパスを確認（wrap.near → token）
            let buyable_tokens = match graph.update_graph(&wnear_token) {
                Ok(goals) => {
                    let token_ids: std::collections::HashSet<_> = goals
                        .iter()
                        .map(|t| t.as_account_id().to_string())
                        .collect();
                    trace!(log, "buyable tokens (wrap.near → token)";
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

            debug!(log, "tokens after buyability filtering";
                "original_count" => original_count,
                "buyable_count" => buyable_filtered.len(),
            );

            // 売却方向のパスも確認（token → wrap.near）
            let wnear_out: crate::ref_finance::token_account::TokenOutAccount =
                crate::ref_finance::token_account::WNEAR_TOKEN.to_out();
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
                            .any(|g| g.as_account_id() == wnear_out.as_account_id())
                        {
                            filtered_tokens.push(token);

                            // 必要な数に達したら即座に終了
                            if filtered_tokens.len() >= limit {
                                trace!(log, "reached required token count, stopping early"; "count" => limit);
                                break;
                            }
                        } else {
                            trace!(log, "token not sellable to wrap.near, skipping"; "token" => %token);
                        }
                    }
                    Err(_) => {
                        trace!(log, "failed to check sellability, skipping"; "token" => %token);
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

            debug!(log, "tokens after liquidity filtering";
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
    period_id: &str,
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
    let mut predictions: BTreeMap<zaciraci_common::types::TokenOutAccount, TokenPrice> =
        BTreeMap::new();
    let mut historical_prices = Vec::new();

    // 型安全な quote_token をループ外で事前に準備（最適化）
    let quote_token_in: TokenInAccount = crate::ref_finance::token_account::WNEAR_TOKEN.to_in();

    // 過去の予測を Chronos 待ちの間に並行評価
    let eval_handle =
        tokio::spawn(async { super::prediction_accuracy::evaluate_pending_predictions().await });

    for token in tokens {
        let token_str = token.to_string();

        // PredictionServiceを使用して価格履歴と予測を取得
        let price_history_days = config::get("TRADE_PRICE_HISTORY_DAYS")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(30);
        let end_date = Utc::now();
        let start_date = end_date - chrono::Duration::days(price_history_days);

        // 型安全なトークンに変換
        let token_out: TokenOutAccount = token.clone().into();

        // 価格履歴の取得
        let history = match prediction_service
            .get_price_history(&token_out, &quote_token_in, start_date, end_date)
            .await
        {
            Ok(hist) => {
                // PredictionServiceのTokenPriceHistoryをcommonのPriceHistoryに変換
                // backend の Token*Account → common の Token*Account に変換
                zaciraci_common::algorithm::types::PriceHistory {
                    token: hist
                        .token
                        .to_string()
                        .parse()
                        .map_err(|e| anyhow::anyhow!("Invalid token in price history: {}", e))?,
                    quote_token: hist.quote_token.to_string().parse().map_err(|e| {
                        anyhow::anyhow!("Invalid quote token in price history: {}", e)
                    })?,
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
        // common の Token*Account → backend の Token*Account に変換
        let predict_token: TokenOutAccount = history
            .token
            .to_string()
            .parse::<TokenAccount>()
            .map_err(|e| anyhow::anyhow!("Invalid token: {}", e))?
            .into();
        let predict_quote: TokenInAccount = history
            .quote_token
            .to_string()
            .parse::<TokenAccount>()
            .map_err(|e| anyhow::anyhow!("Invalid quote token: {}", e))?
            .into();
        let predict_history = crate::trade::predict::TokenPriceHistory {
            token: predict_token,
            quote_token: predict_quote,
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
        // history.token は既に TokenOutAccount なので直接使用
        predictions.insert(history.token.clone(), predicted_price.clone());

        // 相対リターンの計算（expected_return メソッドを使用）
        // price 形式なので、予測価格 > 現在価格 = 正のリターン
        let expected_price_return_pct = current_price.expected_return(&predicted_price) * 100.0;

        trace!(log, "token prediction";
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

        // TokenData 用に symbol を先に取得（history の所有権移動前）
        let symbol_for_token_data = history.token.clone();

        historical_prices.push(history);

        // BigDecimalをf64に変換（外部構造体の制約のため）
        let volatility_f64 = volatility
            .to_string()
            .parse::<f64>()
            .map_err(|e| anyhow::anyhow!("Failed to convert volatility to f64: {}", e))?;

        // トークンの decimals を取得（キャッシュ経由）
        let decimals =
            crate::trade::token_cache::get_token_decimals_cached(client, &token_str).await?;

        // 市場規模の推定（実際の発行量データを取得）
        let market_cap =
            estimate_market_cap_async(client, &token_str, &current_price, decimals).await;

        token_data.push(TokenData {
            symbol: symbol_for_token_data,
            current_rate: ExchangeRate::from_price(&current_price, decimals),
            historical_volatility: volatility_f64,
            liquidity_score: Some(liquidity_score),
            market_cap: Some(market_cap),
        });
    }

    // 評価タスクの結果を取得し prediction_confidence に変換
    let prediction_confidence: Option<f64> = match eval_handle.await {
        Ok(Ok(Some(mape))) => {
            let confidence = super::prediction_accuracy::mape_to_confidence(mape);
            info!(log, "prediction accuracy";
                "rolling_mape" => format!("{:.2}%", mape),
                "prediction_confidence" => format!("{:.3}", confidence)
            );
            Some(confidence)
        }
        Ok(Ok(None)) => {
            debug!(log, "prediction accuracy: insufficient data");
            None
        }
        Ok(Err(e)) => {
            warn!(log, "prediction evaluation failed"; "error" => ?e);
            None
        }
        Err(e) => {
            warn!(log, "prediction evaluation task panicked"; "error" => ?e);
            None
        }
    };

    // 今回の予測を DB に記録（失敗しても取引は続行）
    if let Err(e) =
        super::prediction_accuracy::record_predictions(period_id, &predictions, "wrap.near").await
    {
        warn!(log, "failed to record predictions"; "error" => ?e);
    }

    let portfolio_data = PortfolioData {
        tokens: token_data,
        predictions,
        historical_prices,
        prediction_confidence,
    };

    // 既存ポジションの取得と WalletInfo の構築
    let wallet_info = if is_new_period {
        // 新規期間: ポジションなし、available_funds を総価値として使用
        debug!(log, "new evaluation period, starting with empty holdings");
        let total_value_near = YoctoValue::from_yocto(BigDecimal::from(available_funds)).to_near();
        WalletInfo {
            holdings: BTreeMap::new(),
            total_value: total_value_near.clone(),
            cash_balance: total_value_near,
        }
    } else {
        // 評価期間中: 既存のポジションを取得し、実際のポートフォリオ価値を計算
        debug!(
            log,
            "continuing evaluation period, loading current holdings"
        );
        // wrap.near を含めて全残高を取得
        let wnear_str = crate::ref_finance::token_account::WNEAR_TOKEN.to_string();
        let mut token_strs: Vec<String> = tokens.iter().map(|t| t.to_string()).collect();
        if !token_strs.contains(&wnear_str) {
            token_strs.push(wnear_str.clone());
        }
        let current_balances =
            swap::get_current_portfolio_balances(client, wallet, &token_strs).await?;

        // 実際のポートフォリオ総価値を計算
        let total_value_near =
            swap::calculate_total_portfolio_value(client, wallet, &current_balances).await?;

        // wrap.near の残高を cash_balance として使用
        let cash_balance_near = current_balances
            .get(&wnear_str)
            .map(|amount| {
                let rate = ExchangeRate::wnear();
                amount / &rate
            })
            .unwrap_or_else(NearValue::zero);

        debug!(log, "portfolio value calculated";
            "total_value" => %total_value_near, "cash_balance" => %cash_balance_near);

        // holdings には投資対象トークンのみ（wrap.near は除外）
        let mut holdings_typed = BTreeMap::new();
        for (token, amount) in &current_balances {
            if token == &wnear_str {
                continue;
            }
            if !amount.is_zero() {
                trace!(log, "loaded existing position"; "token" => token, "amount" => %amount);
                if let Ok(token_out) = token.parse::<zaciraci_common::types::TokenOutAccount>() {
                    holdings_typed.insert(token_out, amount.clone());
                }
            }
        }

        WalletInfo {
            holdings: holdings_typed,
            total_value: total_value_near,
            cash_balance: cash_balance_near,
        }
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

    info!(log, "portfolio metrics";
        "daily_return" => execution_report.expected_metrics.daily_return,
        "volatility" => execution_report.expected_metrics.volatility,
        "sharpe_ratio" => execution_report.expected_metrics.sharpe_ratio,
        "sortino_ratio" => execution_report.expected_metrics.sortino_ratio,
        "max_drawdown" => execution_report.expected_metrics.max_drawdown,
        "calmar_ratio" => execution_report.expected_metrics.calmar_ratio,
        "turnover_rate" => execution_report.expected_metrics.turnover_rate
    );

    for (token, weight) in &execution_report.optimal_weights.weights {
        trace!(log, "optimal weight";
            "token" => %token,
            "weight" => weight,
            "percentage" => format!("{:.2}%", weight * 100.0)
        );
    }

    Ok(execution_report.actions)
}

/// ハーベスト判定と実行
async fn check_and_harvest(current_portfolio_value: YoctoValue) -> Result<()> {
    // 実際のハーベスト機能を呼び出す
    // 注: 評価期間中は available_funds = 0 が渡されるため、ハーベスト判定はスキップされる
    // 評価期間終了時（清算後）のみ、liquidated_balance が渡され、ハーベスト判定が実行される
    crate::trade::harvest::check_and_harvest(current_portfolio_value).await
}
