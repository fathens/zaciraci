use super::types::*;
use super::utils::calculate_trading_cost_by_value_yocto_bd;
use anyhow::Result;
use bigdecimal::{BigDecimal, FromPrimitive};
use chrono::{DateTime, Utc};
use common::algorithm::{PredictionData, TradingAction};
use common::types::TokenPrice;
use std::collections::HashMap;

// Import cache-related modules
use crate::models::prediction::{
    PredictionFileData, PredictionMetadata, PredictionPoint as CachePredictionPoint,
    PredictionResults,
};
use crate::utils::cache::{
    PredictionCacheParams, check_prediction_cache, load_prediction_data, save_prediction_result,
};
use common::cache::CacheOutput;

#[allow(clippy::too_many_arguments)]
/// Try to load prediction from cache
async fn try_load_from_cache(
    token: &str,
    quote_token: &str,
    model_name: &str,
    hist_start: DateTime<Utc>,
    hist_end: DateTime<Utc>,
    pred_start: DateTime<Utc>,
    pred_end: DateTime<Utc>,
) -> Result<Option<PredictionData>> {
    let cache_params = PredictionCacheParams {
        model_name,
        quote_token,
        base_token: token,
        hist_start,
        hist_end,
        pred_start,
        pred_end,
    };

    if let Some(cache_path) = check_prediction_cache(&cache_params).await? {
        let cached_data = load_prediction_data(&cache_path).await?;

        // Convert cached prediction to PredictionData
        if let Some(last_prediction) = cached_data.prediction_results.predictions.last() {
            // Get current price from first prediction point (assuming it represents recent price)
            let current_price = cached_data
                .prediction_results
                .predictions
                .first()
                .map(|p| p.price.clone())
                .unwrap_or(last_prediction.price.clone());

            // キャッシュには TokenPrice として保存されているのでそのまま使用
            return Ok(Some(PredictionData {
                token: token.to_string(),
                current_price,
                predicted_price_24h: last_prediction.price.clone(),
                timestamp: pred_start,
                confidence: last_prediction.confidence.clone(),
            }));
        }
    }

    Ok(None)
}

/// Save prediction result to cache
async fn save_to_cache(
    cache_params: &PredictionCacheParams<'_>,
    forecast_data: &common::prediction::ChronosPredictionResponse,
) -> Result<()> {
    // Convert forecast data to cache format
    // NOTE: Chronos は入力データと同じスケールで値を返す
    //       CLI は Backend API から price (NEAR/token) を取得して Chronos に送信するため、
    //       Chronos の予測値も price 形式になっている
    let cache_predictions: Vec<CachePredictionPoint> = forecast_data
        .forecast_timestamp
        .iter()
        .zip(forecast_data.forecast_values.iter())
        .map(|(timestamp, price_value)| {
            // price_value は price 形式（NEAR/token）
            CachePredictionPoint {
                timestamp: *timestamp,
                price: TokenPrice::from_near_per_token(price_value.clone()),
                confidence: None, // Could extract from confidence intervals if available
            }
        })
        .collect();

    let prediction_file_data = PredictionFileData {
        metadata: PredictionMetadata {
            generated_at: Utc::now(),
            model_name: cache_params.model_name.to_string(),
            base_token: cache_params.base_token.to_string(),
            quote_token: cache_params.quote_token.to_string(),
            history_start: cache_params.hist_start.format("%Y-%m-%d").to_string(),
            history_end: cache_params.hist_end.format("%Y-%m-%d").to_string(),
            prediction_start: cache_params.pred_start.format("%Y-%m-%d").to_string(),
            prediction_end: cache_params.pred_end.format("%Y-%m-%d").to_string(),
        },
        prediction_results: PredictionResults {
            predictions: cache_predictions,
            model_metrics: forecast_data
                .metrics
                .as_ref()
                .map(|metrics| serde_json::to_value(metrics).unwrap_or(serde_json::Value::Null)),
        },
    };

    save_prediction_result(cache_params, &prediction_file_data).await?;
    CacheOutput::prediction_cached(
        cache_params.base_token,
        prediction_file_data.prediction_results.predictions.len(),
    );

    Ok(())
}

#[allow(clippy::too_many_arguments)]
/// Generate predictions using Chronos API
pub async fn generate_api_predictions(
    backend_client: &crate::api::backend::BackendClient,
    target_tokens: &[String],
    quote_token: &str,
    current_time: DateTime<Utc>,
    historical_days: i64,
    prediction_horizon: chrono::Duration,
    model: Option<String>,
    verbose: bool,
) -> Result<Vec<PredictionData>> {
    use common::api::chronos::ChronosApiClient;
    use common::prediction::ZeroShotPredictionRequest;
    use std::env;

    let mut predictions = Vec::new();

    // Check for Chronos URL configuration
    let chronos_url =
        env::var("CHRONOS_URL").unwrap_or_else(|_| "http://localhost:8000".to_string());

    if verbose {
        println!("🔮 Using Chronos prediction service at {}", chronos_url);
    }

    let chronos_client = ChronosApiClient::new(chronos_url);

    for token in target_tokens {
        // Calculate prediction window
        let pred_start = current_time;
        let pred_end = current_time + prediction_horizon;
        let hist_start = current_time - chrono::Duration::days(historical_days);
        let hist_end = current_time;

        let model_name = model.as_deref().unwrap_or("chronos_default");

        // Try to load from cache first
        match try_load_from_cache(
            token,
            quote_token,
            model_name,
            hist_start,
            hist_end,
            pred_start,
            pred_end,
        )
        .await
        {
            Ok(Some(cached_prediction)) => {
                CacheOutput::prediction_cache_hit(token);
                predictions.push(cached_prediction);
                continue; // Move to next token
            }
            Ok(None) => {
                CacheOutput::prediction_cache_miss(token);
            }
            Err(e) => {
                if verbose {
                    println!("⚠️ Failed to check cache for {}: {}", token, e);
                }
                // Continue with API fetch
            }
        }

        // Get historical price data for prediction
        match get_historical_price_data(
            backend_client,
            token,
            quote_token,
            historical_days,
            current_time,
        )
        .await
        {
            Ok((timestamps, values, current_price)) => {
                if timestamps.len() < 10 {
                    if verbose {
                        println!(
                            "⚠️ Insufficient historical data for {}: {} points",
                            token,
                            timestamps.len()
                        );
                    }
                    return Err(anyhow::anyhow!(
                        "Insufficient historical data for token {}: {} points (minimum 10 required)",
                        token,
                        timestamps.len()
                    ));
                }

                // Prepare prediction request with historical data
                let forecast_until = current_time + prediction_horizon;
                let prediction_request = ZeroShotPredictionRequest {
                    timestamp: timestamps,
                    values,
                    forecast_until,
                    model_name: model.clone(),
                    model_params: None,
                };

                // Submit prediction and wait for completion
                match chronos_client.predict_zero_shot(prediction_request).await {
                    Ok(async_response) => {
                        if verbose {
                            println!(
                                "📝 Submitted prediction for {}: {}",
                                token, async_response.task_id
                            );
                        }

                        // Poll for completion
                        match chronos_client
                            .poll_prediction_until_complete(&async_response.task_id)
                            .await
                        {
                            Ok(result) => {
                                // Extract prediction from result
                                if let Some(chronos_result) = &result.result {
                                    if let Some(predicted_value) =
                                        chronos_result.forecast_values.last()
                                    {
                                        // Save to cache before creating PredictionData
                                        let cache_params = PredictionCacheParams {
                                            model_name,
                                            quote_token,
                                            base_token: token,
                                            hist_start,
                                            hist_end,
                                            pred_start,
                                            pred_end,
                                        };

                                        if let Err(e) =
                                            save_to_cache(&cache_params, chronos_result).await
                                        {
                                            println!(
                                                "⚠️ Failed to save prediction to cache: {}",
                                                e
                                            );
                                            // Continue anyway, don't fail the simulation
                                        }

                                        // Chronos の予測値は price 形式（NEAR/token）
                                        predictions.push(PredictionData {
                                            token: token.clone(),
                                            current_price: TokenPrice::from_near_per_token(
                                                current_price.clone(),
                                            ),
                                            predicted_price_24h: TokenPrice::from_near_per_token(
                                                predicted_value.clone(),
                                            ),
                                            timestamp: current_time,
                                            confidence: chronos_result
                                                .metrics
                                                .as_ref()
                                                .and_then(|m| m.get("confidence"))
                                                .cloned(),
                                        });
                                        if verbose {
                                            println!(
                                                "✅ Got prediction for {}: {:.4} -> {:.4}",
                                                token, current_price, predicted_value
                                            );
                                        }
                                    } else {
                                        if verbose {
                                            println!(
                                                "⚠️ No forecast values returned for {}",
                                                token
                                            );
                                        }
                                        return Err(anyhow::anyhow!(
                                            "No forecast values returned for token {}",
                                            token
                                        ));
                                    }
                                } else {
                                    if verbose {
                                        println!("⚠️ No prediction result returned for {}", token);
                                    }
                                    return Err(anyhow::anyhow!(
                                        "No prediction result returned for token {}",
                                        token
                                    ));
                                }
                            }
                            Err(e) => {
                                if verbose {
                                    println!("❌ Prediction failed for {}: {}", token, e);
                                }
                                return Err(anyhow::anyhow!(
                                    "Prediction failed for token {}: {}",
                                    token,
                                    e
                                ));
                            }
                        }
                    }
                    Err(e) => {
                        if verbose {
                            println!("❌ Failed to submit prediction for {}: {}", token, e);
                        }
                        return Err(anyhow::anyhow!(
                            "Failed to submit prediction for token {}: {}",
                            token,
                            e
                        ));
                    }
                }
            }
            Err(e) => {
                if verbose {
                    println!("⚠️ Failed to get historical data for {}: {}", token, e);
                }
                return Err(anyhow::anyhow!(
                    "Failed to get historical data for token {}: {}",
                    token,
                    e
                ));
            }
        }
    }

    Ok(predictions)
}

/// Get historical price data for prediction
async fn get_historical_price_data(
    backend_client: &crate::api::backend::BackendClient,
    token: &str,
    quote_token: &str,
    historical_days: i64,
    current_simulation_time: DateTime<Utc>,
) -> Result<(Vec<DateTime<Utc>>, Vec<BigDecimal>, BigDecimal)> {
    let end_time = current_simulation_time.naive_utc();
    let start_time = end_time - chrono::Duration::days(historical_days);

    let prices = backend_client
        .get_price_history(quote_token, token, start_time, end_time)
        .await?;

    if prices.is_empty() {
        return Err(anyhow::anyhow!(
            "No historical price data found for {}",
            token
        ));
    }

    let timestamps: Vec<DateTime<Utc>> = prices
        .iter()
        .map(|p| DateTime::from_naive_utc_and_offset(p.time, Utc))
        .collect();

    let values: Vec<BigDecimal> = prices.iter().map(|p| p.value.clone()).collect();
    let current_price = prices.last().unwrap().value.clone();

    Ok((timestamps, values, current_price))
}

/// Trading context for managing mutable state during trade execution
pub struct TradeContext<'a> {
    pub current_token: &'a str,
    /// 現在保有量（smallest_unit）
    pub current_amount: TokenAmountF64,
    /// 現在価格（無次元比率: yoctoNEAR/smallest_unit = NEAR/token）
    pub current_price: TokenPriceF64,
    /// 全トークンの価格（無次元比率）
    pub all_prices: &'a HashMap<String, TokenPriceF64>,
    /// 保有量（smallest_unit）
    pub holdings: &'a mut HashMap<String, TokenAmountF64>,
    pub timestamp: DateTime<Utc>,
    pub config: &'a SimulationConfig,
}

/// Execute a trading action and return the trade execution details
pub fn execute_trading_action(
    action: TradingAction,
    ctx: &mut TradeContext,
) -> Result<Option<TradeExecution>> {
    match action {
        TradingAction::Hold => Ok(None),

        TradingAction::Sell { token: _, target } => {
            let target_price = ctx
                .all_prices
                .get(&target)
                .copied()
                .unwrap_or(TokenPriceF64::zero());
            if target_price.is_zero() {
                return Ok(None);
            }

            // 取引価値を計算（yoctoNEAR建て、BigDecimal精度保持）
            // 型安全な変換メソッドを使用
            let current_amount_bd = ctx.current_amount.to_bigdecimal().smallest_units().clone();
            let current_price_bd = ctx.current_price.to_bigdecimal().into_bigdecimal();
            let trade_value_yocto_bd = &current_amount_bd * &current_price_bd;

            // ガスコストをBigDecimalで計算（型安全な変換、精度損失なし）
            let gas_cost_yocto_bd = NearValue::from_near(ctx.config.gas_cost.clone())
                .to_yocto()
                .into_bigdecimal();

            // スリッパレートをBigDecimalで計算（無次元の比率なのでそのまま変換）
            let slippage_rate_bd =
                BigDecimal::from_f64(ctx.config.slippage_rate).unwrap_or_default();

            // 取引コストをyoctoNEAR価値ベースで計算（BigDecimal精度）
            let trade_cost_value_yocto_bd = calculate_trading_cost_by_value_yocto_bd(
                &trade_value_yocto_bd,
                &ctx.config.fee_model,
                &slippage_rate_bd,
                &gas_cost_yocto_bd,
            );

            // コストをトークン数量で表現（BigDecimal精度保持）
            // current_amount と同じ decimals を使用
            let decimals = ctx.current_amount.decimals();
            let trade_cost = if !ctx.current_price.is_zero() {
                TokenAmountF64::from_smallest_units(
                    (&trade_cost_value_yocto_bd / &current_price_bd)
                        .to_string()
                        .parse::<f64>()
                        .unwrap_or(0.0),
                    decimals,
                )
            } else {
                TokenAmountF64::zero(decimals)
            };

            // SELLアクション: 現在のトークンを売却してtarget（quote_token）を取得
            let net_amount = ctx.current_amount - trade_cost; // 取引後に残るトークン数量
            let sell_value_yocto = net_amount * ctx.current_price; // 売却価値（yoctoNEAR）
            let new_amount = sell_value_yocto / target_price; // targetトークン数量を計算

            // ポートフォリオ更新
            ctx.holdings.remove(ctx.current_token);
            ctx.holdings.insert(target.clone(), new_amount);

            // ポートフォリオ価値をNEAR単位で計算（型安全な変換を使用）
            let portfolio_before_yocto = ctx.current_amount * ctx.current_price;
            let portfolio_after_yocto = new_amount * target_price;
            let portfolio_before = portfolio_before_yocto.to_near();
            let portfolio_after = portfolio_after_yocto.to_near();

            // 約定価格（無次元比率なので変換不要）
            let executed_price = target_price;

            Ok(Some(TradeExecution {
                timestamp: ctx.timestamp,
                from_token: ctx.current_token.to_string(),
                to_token: target,
                amount: new_amount, // 購入するトークン数量
                executed_price,     // 購入トークンの価格（無次元比率）
                cost: TradingCost {
                    protocol_fee: &trade_cost_value_yocto_bd
                        * BigDecimal::from_f64(0.7).unwrap_or_default(),
                    slippage: &trade_cost_value_yocto_bd
                        * BigDecimal::from_f64(0.2).unwrap_or_default(),
                    gas_fee: gas_cost_yocto_bd.clone(),
                    total: trade_cost_value_yocto_bd.clone(),
                },
                portfolio_value_before: portfolio_before,
                portfolio_value_after: portfolio_after,
                success: true,
                reason: "Momentum sell executed".to_string(),
            }))
        }

        TradingAction::Switch { from: _, to } => {
            let target_price = ctx
                .all_prices
                .get(&to)
                .copied()
                .unwrap_or(TokenPriceF64::zero());
            if target_price.is_zero() {
                return Ok(None);
            }

            // 取引価値を計算（yoctoNEAR建て、BigDecimal精度保持）
            // 型安全な変換メソッドを使用
            let current_amount_bd = ctx.current_amount.to_bigdecimal().smallest_units().clone();
            let current_price_bd = ctx.current_price.to_bigdecimal().into_bigdecimal();
            let trade_value_yocto_bd = &current_amount_bd * &current_price_bd;

            // ガスコストをBigDecimalで計算（型安全な変換、精度損失なし）
            let gas_cost_yocto_bd = NearValue::from_near(ctx.config.gas_cost.clone())
                .to_yocto()
                .into_bigdecimal();

            // スリッパレートをBigDecimalで計算（無次元の比率なのでそのまま変換）
            let slippage_rate_bd =
                BigDecimal::from_f64(ctx.config.slippage_rate).unwrap_or_default();

            // 取引コストをyoctoNEAR価値ベースで計算（BigDecimal精度）
            let trade_cost_value_yocto_bd = calculate_trading_cost_by_value_yocto_bd(
                &trade_value_yocto_bd,
                &ctx.config.fee_model,
                &slippage_rate_bd,
                &gas_cost_yocto_bd,
            );

            // コストをトークン数量で表現（BigDecimal精度保持）
            // current_amount と同じ decimals を使用
            let decimals = ctx.current_amount.decimals();
            let trade_cost = if !ctx.current_price.is_zero() {
                TokenAmountF64::from_smallest_units(
                    (&trade_cost_value_yocto_bd / &current_price_bd)
                        .to_string()
                        .parse::<f64>()
                        .unwrap_or(0.0),
                    decimals,
                )
            } else {
                TokenAmountF64::zero(decimals)
            };

            // SWITCHアクション: fromトークンをtoトークンに交換
            let net_amount = ctx.current_amount - trade_cost; // 取引後に残るトークン数量
            let switch_value_yocto = net_amount * ctx.current_price; // 交換価値（yoctoNEAR）
            let new_amount = switch_value_yocto / target_price; // toトークン数量を計算

            // ポートフォリオ更新
            ctx.holdings.remove(ctx.current_token);
            ctx.holdings.insert(to.clone(), new_amount);

            // ポートフォリオ価値をNEAR単位で計算（型安全な変換を使用）
            let portfolio_before_yocto = ctx.current_amount * ctx.current_price;
            let portfolio_after_yocto = new_amount * target_price;
            let portfolio_before = portfolio_before_yocto.to_near();
            let portfolio_after = portfolio_after_yocto.to_near();

            // 約定価格（無次元比率なので変換不要）
            let executed_price = target_price;

            Ok(Some(TradeExecution {
                timestamp: ctx.timestamp,
                from_token: ctx.current_token.to_string(),
                to_token: to,
                amount: new_amount, // 購入するトークン数量
                executed_price,     // 購入トークンの価格（無次元比率）
                cost: TradingCost {
                    protocol_fee: &trade_cost_value_yocto_bd
                        * BigDecimal::from_f64(0.7).unwrap_or_default(),
                    slippage: &trade_cost_value_yocto_bd
                        * BigDecimal::from_f64(0.2).unwrap_or_default(),
                    gas_fee: gas_cost_yocto_bd.clone(),
                    total: trade_cost_value_yocto_bd.clone(),
                },
                portfolio_value_before: portfolio_before,
                portfolio_value_after: portfolio_after,
                success: true,
                reason: "Momentum switch executed".to_string(),
            }))
        }

        // 新しいアクションタイプの処理（今回はプレースホルダーとして）
        TradingAction::Rebalance { .. } => {
            // ポートフォリオリバランスの処理（将来実装）
            Ok(None)
        }
        TradingAction::AddPosition { .. } => {
            // ポジション追加の処理（将来実装）
            Ok(None)
        }
        TradingAction::ReducePosition { .. } => {
            // ポジション削減の処理（将来実装）
            Ok(None)
        }
    }
}

/// Immutable portfolio operations for functional trading
impl ImmutablePortfolio {
    /// Create a new portfolio with initial capital in a specific token
    ///
    /// # Arguments
    /// * `initial_capital` - 初期資金（smallest_unit）
    /// * `initial_token` - 初期トークン名
    pub fn new(initial_capital: TokenAmountF64, initial_token: &str) -> Self {
        let mut holdings = HashMap::new();
        holdings.insert(initial_token.to_string(), initial_capital);

        Self {
            holdings,
            cash_balance: NearValueF64::zero(),
            timestamp: Utc::now(),
        }
    }

    /// Calculate total portfolio value using market prices
    ///
    /// # Returns
    /// ポートフォリオ総価値（yoctoNEAR単位）
    ///
    /// 注意: market.prices は無次元比率（yoctoNEAR/smallest_unit）なので、
    /// amount * price = yoctoNEAR 単位となる
    pub fn total_value(&self, market: &MarketSnapshot) -> YoctoValueF64 {
        let mut total = self.cash_balance.to_yocto();

        for (token, &amount) in &self.holdings {
            if let Some(&price) = market.prices.get(token) {
                // price: 無次元比率（yoctoNEAR/smallest_unit）
                // amount: smallest_unit
                // → 結果は yoctoNEAR 単位
                let value = amount * price;
                total = total + value;
            }
        }

        total
    }

    /// Apply a trading decision and return the portfolio transition
    pub fn apply_trade(
        &self,
        decision: &TradingDecision,
        market: &MarketSnapshot,
        _config: &TradingConfig,
    ) -> Result<PortfolioTransition> {
        let mut new_holdings = self.holdings.clone();
        let mut cost = 0.0;

        let new_portfolio = match decision {
            TradingDecision::Hold => ImmutablePortfolio {
                holdings: new_holdings,
                cash_balance: self.cash_balance,
                timestamp: market.timestamp,
            },
            TradingDecision::Sell { target_token } => {
                // Sell current holding to target token
                if let Some((current_token, &current_amount)) = new_holdings.iter().next() {
                    let current_token = current_token.clone();

                    new_holdings.remove(&current_token);

                    // 両方のトークン価格を取得して正しく変換
                    if let (Some(&current_price), Some(&target_price)) = (
                        market.prices.get(&current_token),
                        market.prices.get(target_token),
                    ) {
                        // 現在の価値を計算（yoctoNEAR単位）
                        // 型安全な演算子を使用: YoctoValueF64 * f64, YoctoValueF64 - f64
                        let current_value = current_amount * current_price;
                        let fee = current_value * 0.006; // Simple fee calculation (yoctoNEAR単位)
                        cost = fee.as_f64();
                        let net_value = current_value - cost;
                        let target_amount = net_value / target_price;

                        new_holdings.insert(target_token.clone(), target_amount);
                    }
                }

                ImmutablePortfolio {
                    holdings: new_holdings,
                    cash_balance: self.cash_balance,
                    timestamp: market.timestamp,
                }
            }
            TradingDecision::Switch { from, to } => {
                if let Some(&from_amount) = new_holdings.get(from) {
                    new_holdings.remove(from);

                    if let (Some(&from_price), Some(&to_price)) =
                        (market.prices.get(from), market.prices.get(to))
                    {
                        // 型安全な演算子を使用: YoctoValueF64 * f64, YoctoValueF64 - f64
                        let from_value = from_amount * from_price;
                        let fee = from_value * 0.006; // Simple fee calculation
                        cost = fee.as_f64();
                        let net_value = from_value - cost;
                        let to_amount = net_value / to_price;

                        new_holdings.insert(to.clone(), to_amount);
                    }
                }

                ImmutablePortfolio {
                    holdings: new_holdings,
                    cash_balance: self.cash_balance,
                    timestamp: market.timestamp,
                }
            }
        };

        Ok(PortfolioTransition {
            from: self.clone(),
            to: new_portfolio,
            action: decision.clone(),
            cost,
            reason: format!("Trade executed: {:?}", decision),
        })
    }

    /// Get the dominant token in the portfolio (token with highest value)
    pub fn get_dominant_token(&self, market: &MarketSnapshot) -> Option<String> {
        let mut max_value = YoctoValueF64::zero();
        let mut dominant_token = None;

        for (token, &amount) in &self.holdings {
            if let Some(&price) = market.prices.get(token) {
                let value = amount * price;
                if value > max_value {
                    max_value = value;
                    dominant_token = Some(token.clone());
                }
            }
        }

        dominant_token
    }

    /// Check if portfolio has exposure to a specific token
    pub fn has_token(&self, token: &str) -> bool {
        self.holdings
            .get(token)
            .map(|amount| amount.is_positive())
            .unwrap_or(false)
    }

    /// Get allocation percentage for each token
    pub fn get_allocations(&self, market: &MarketSnapshot) -> HashMap<String, f64> {
        let total_value = self.total_value(market);
        let mut allocations = HashMap::new();

        // 型安全なメソッドを使用: YoctoValueF64.is_positive()
        if total_value.is_positive() {
            for (token, &amount) in &self.holdings {
                if let Some(&price) = market.prices.get(token) {
                    let token_value = amount * price;
                    // 型安全な除算を使用: YoctoValueF64 / YoctoValueF64 = f64
                    let allocation = (token_value / total_value) * 100.0;
                    allocations.insert(token.clone(), allocation);
                }
            }
        }

        // Add cash allocation
        if self.cash_balance.is_positive() {
            // 型安全な除算を使用: YoctoValueF64 / YoctoValueF64 = f64
            let cash_allocation = (self.cash_balance.to_yocto() / total_value) * 100.0;
            allocations.insert("cash".to_string(), cash_allocation);
        }

        allocations
    }

    /// Calculate portfolio risk metrics
    pub fn calculate_portfolio_risk(&self, market: &MarketSnapshot) -> PortfolioRisk {
        let allocations = self.get_allocations(market);
        let num_positions = self.holdings.len();

        // Calculate concentration risk (Herfindahl index)
        let concentration_index: f64 = allocations
            .values()
            .map(|&allocation| (allocation / 100.0).powi(2))
            .sum();

        // Calculate diversification score (1 - concentration index)
        let diversification_score = 1.0 - concentration_index;

        // Risk level based on concentration
        let risk_level = if concentration_index > 0.8 {
            RiskLevel::High
        } else if concentration_index > 0.5 {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        };

        PortfolioRisk {
            concentration_index,
            diversification_score,
            num_positions,
            risk_level,
            largest_position_pct: allocations.values().fold(0.0f64, |a, &b| a.max(b)),
        }
    }
}

/// Portfolio risk assessment
#[derive(Debug, Clone)]
pub struct PortfolioRisk {
    pub concentration_index: f64,
    pub diversification_score: f64,
    pub num_positions: usize,
    pub risk_level: RiskLevel,
    pub largest_position_pct: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

/// Trading strategy implementations
impl TradingStrategy for MomentumStrategy {
    fn name(&self) -> &'static str {
        "Momentum"
    }

    fn make_decision(
        &self,
        portfolio: &ImmutablePortfolio,
        market: &MarketSnapshot,
        opportunities: &[TokenOpportunity],
        config: &TradingConfig,
    ) -> Result<TradingDecision> {
        // Filter opportunities by minimum confidence
        let high_confidence_opportunities: Vec<&TokenOpportunity> = opportunities
            .iter()
            .filter(|opp| opp.confidence.unwrap_or(0.0) >= self.min_confidence)
            .collect();

        if high_confidence_opportunities.is_empty() {
            return Ok(TradingDecision::Hold);
        }

        // Sort by confidence-adjusted expected return
        let mut sorted_opportunities = high_confidence_opportunities;
        sorted_opportunities.sort_by(|a, b| {
            let score_a = a.expected_return * a.confidence.unwrap_or(0.5);
            let score_b = b.expected_return * b.confidence.unwrap_or(0.5);
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let best_opportunity = sorted_opportunities.first().unwrap();

        // Check if we should switch to the best opportunity
        if let Some(current_token) = portfolio.get_dominant_token(market) {
            if current_token != best_opportunity.token {
                let current_opportunity =
                    opportunities.iter().find(|opp| opp.token == current_token);

                if let Some(current_opp) = current_opportunity {
                    let current_score =
                        current_opp.expected_return * current_opp.confidence.unwrap_or(0.5);
                    let best_score = best_opportunity.expected_return
                        * best_opportunity.confidence.unwrap_or(0.5);

                    if best_score > current_score * config.switch_multiplier {
                        return Ok(TradingDecision::Switch {
                            from: current_token,
                            to: best_opportunity.token.clone(),
                        });
                    }
                }
            }
        } else if best_opportunity.expected_return > config.min_profit_threshold {
            // No current position, enter best opportunity if profitable enough
            return Ok(TradingDecision::Sell {
                target_token: best_opportunity.token.clone(),
            });
        }

        Ok(TradingDecision::Hold)
    }

    fn should_rebalance(&self, _portfolio: &ImmutablePortfolio, _market: &MarketSnapshot) -> bool {
        // Momentum strategy rebalances based on lookback periods
        true // Simplified implementation
    }
}

impl TradingStrategy for PortfolioStrategy {
    fn name(&self) -> &'static str {
        "Portfolio"
    }

    fn make_decision(
        &self,
        portfolio: &ImmutablePortfolio,
        market: &MarketSnapshot,
        opportunities: &[TokenOpportunity],
        _config: &TradingConfig,
    ) -> Result<TradingDecision> {
        let current_positions = portfolio.holdings.len();

        // If we have fewer positions than max, consider adding
        if current_positions < self.max_positions {
            // Find the best opportunity not currently held
            let best_new_opportunity = opportunities
                .iter()
                .filter(|opp| !portfolio.has_token(&opp.token))
                .max_by(|a, b| {
                    a.expected_return
                        .partial_cmp(&b.expected_return)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

            if let Some(opp) = best_new_opportunity
                && opp.expected_return > 0.05
            {
                // 5% minimum expected return
                return Ok(TradingDecision::Sell {
                    target_token: opp.token.clone(),
                });
            }
        }

        // Check if we should rebalance existing positions
        if self.should_rebalance(portfolio, market) {
            // Find the worst performing position to potentially replace
            let worst_position = opportunities
                .iter()
                .filter(|opp| portfolio.has_token(&opp.token))
                .min_by(|a, b| {
                    a.expected_return
                        .partial_cmp(&b.expected_return)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

            let best_opportunity = opportunities
                .iter()
                .filter(|opp| !portfolio.has_token(&opp.token))
                .max_by(|a, b| {
                    a.expected_return
                        .partial_cmp(&b.expected_return)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

            if let (Some(worst), Some(best)) = (worst_position, best_opportunity)
                && best.expected_return > worst.expected_return * 1.2
            {
                // 20% improvement threshold
                return Ok(TradingDecision::Switch {
                    from: worst.token.clone(),
                    to: best.token.clone(),
                });
            }
        }

        Ok(TradingDecision::Hold)
    }

    fn should_rebalance(&self, portfolio: &ImmutablePortfolio, market: &MarketSnapshot) -> bool {
        let allocations = portfolio.get_allocations(market);
        let target_allocation = 100.0 / self.max_positions as f64;

        // Check if any allocation deviates significantly from target
        allocations
            .values()
            .any(|&allocation| (allocation - target_allocation).abs() > self.rebalance_threshold)
    }
}

impl TradingStrategy for TrendFollowingStrategy {
    fn name(&self) -> &'static str {
        "TrendFollowing"
    }

    fn make_decision(
        &self,
        portfolio: &ImmutablePortfolio,
        market: &MarketSnapshot,
        opportunities: &[TokenOpportunity],
        config: &TradingConfig,
    ) -> Result<TradingDecision> {
        // Filter opportunities with strong trends and low volatility
        let trend_opportunities: Vec<&TokenOpportunity> = opportunities
            .iter()
            .filter(|opp| {
                opp.expected_return > 0.02 && // Minimum 2% expected return for trend
                opp.confidence.unwrap_or(0.0) > 0.6 // High confidence in trend
            })
            .collect();

        if trend_opportunities.is_empty() {
            return Ok(TradingDecision::Hold);
        }

        // Sort by trend strength (expected return weighted by confidence)
        let mut sorted_trends = trend_opportunities;
        sorted_trends.sort_by(|a, b| {
            let strength_a = a.expected_return * a.confidence.unwrap_or(0.5);
            let strength_b = b.expected_return * b.confidence.unwrap_or(0.5);
            strength_b
                .partial_cmp(&strength_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let strongest_trend = sorted_trends.first().unwrap();

        // Trend following typically uses concentrated positions
        if let Some(current_token) = portfolio.get_dominant_token(market) {
            if current_token != strongest_trend.token {
                let trend_strength =
                    strongest_trend.expected_return * strongest_trend.confidence.unwrap_or(0.5);

                if trend_strength > config.min_profit_threshold * 2.0 {
                    return Ok(TradingDecision::Switch {
                        from: current_token,
                        to: strongest_trend.token.clone(),
                    });
                }
            }
        } else {
            let trend_strength =
                strongest_trend.expected_return * strongest_trend.confidence.unwrap_or(0.5);

            if trend_strength > config.min_profit_threshold {
                return Ok(TradingDecision::Sell {
                    target_token: strongest_trend.token.clone(),
                });
            }
        }

        Ok(TradingDecision::Hold)
    }

    fn should_rebalance(&self, _portfolio: &ImmutablePortfolio, _market: &MarketSnapshot) -> bool {
        // Trend following typically holds positions longer
        false // Simplified implementation - only rebalance on signal changes
    }
}

/// Strategy context for managing different trading strategies
impl StrategyContext {
    /// Create a new strategy context with the specified strategy
    pub fn new(strategy: Box<dyn TradingStrategy>) -> Self {
        Self { strategy }
    }

    /// Execute the strategy's decision-making process
    pub fn execute_strategy(
        &self,
        portfolio: &ImmutablePortfolio,
        market: &MarketSnapshot,
        opportunities: &[TokenOpportunity],
        config: &TradingConfig,
    ) -> Result<TradingDecision> {
        self.strategy
            .make_decision(portfolio, market, opportunities, config)
    }

    /// Get the name of the current strategy
    pub fn strategy_name(&self) -> &'static str {
        self.strategy.name()
    }

    /// Check if the strategy recommends rebalancing
    pub fn should_rebalance(
        &self,
        portfolio: &ImmutablePortfolio,
        market: &MarketSnapshot,
    ) -> bool {
        self.strategy.should_rebalance(portfolio, market)
    }
}

/// Convert PredictionData to TokenOpportunity for strategy use
impl From<&PredictionData> for TokenOpportunity {
    fn from(prediction: &PredictionData) -> Self {
        // PredictionData は既に TokenPrice を持っているので直接使用
        let expected_return =
            if !prediction.current_price.is_zero() && !prediction.predicted_price_24h.is_zero() {
                prediction
                    .current_price
                    .expected_return(&prediction.predicted_price_24h)
            } else {
                0.0
            };

        TokenOpportunity {
            token: prediction.token.clone(),
            expected_return,
            confidence: prediction
                .confidence
                .as_ref()
                .map(|c| c.to_string().parse::<f64>().unwrap_or(0.0)),
        }
    }
}

#[cfg(test)]
mod tests;
