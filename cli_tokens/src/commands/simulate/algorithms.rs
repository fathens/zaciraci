use super::data::{
    calculate_gap_impact, fetch_price_data, get_last_known_prices_for_evaluation,
    get_prices_at_time, get_prices_at_time_optional, log_data_gap_event,
};
use super::types::*;
use crate::api::backend::BackendClient;
use anyhow::Result;
#[allow(unused_imports)]
use bigdecimal::{BigDecimal, FromPrimitive};
use chrono::{DateTime, Utc};
use common::types::{ExchangeRate, NearValue, TokenOutAccount, TokenPrice};
use std::collections::{BTreeMap, HashMap};

/// Run momentum simulation
pub async fn run_momentum_simulation(config: &SimulationConfig) -> Result<SimulationResult> {
    if config.verbose {
        println!(
            "📈 Running momentum simulation for tokens: {:?}",
            config.target_tokens
        );
    }

    let backend_client = BackendClient::new();

    // 1. 価格データを取得（キャッシュ対応）
    let price_data = fetch_price_data(&backend_client, config).await?;

    if price_data.is_empty() {
        return Err(anyhow::anyhow!(
            "No price data available for simulation period. Please check your backend connection and ensure price data exists for the specified tokens and time period."
        ));
    }

    // 2. Momentumアルゴリズムでシミュレーション実行（commonクレート使用）
    run_momentum_timestep_simulation(config, &price_data).await
}

/// Run portfolio optimization simulation
pub async fn run_portfolio_simulation(config: &SimulationConfig) -> Result<SimulationResult> {
    if config.verbose {
        println!("📊 Running portfolio optimization simulation");
        println!(
            "🔧 Optimizing portfolio for tokens: {:?}",
            config.target_tokens
        );
    }

    let backend_client = BackendClient::new();

    // 1. 価格データを取得（キャッシュ対応）
    let price_data = fetch_price_data(&backend_client, config).await?;

    if price_data.is_empty() {
        return Err(anyhow::anyhow!(
            "No price data available for simulation period. Please check your backend connection and ensure price data exists for the specified tokens and time period."
        ));
    }

    // 2. Portfolio最適化アルゴリズムでシミュレーション実行（commonクレート使用）
    run_portfolio_optimization_simulation(config, &price_data).await
}

/// Run momentum timestep simulation using common crate algorithm
pub(crate) async fn run_momentum_timestep_simulation(
    config: &SimulationConfig,
    price_data: &HashMap<String, Vec<common::stats::ValueAtTime>>,
) -> Result<SimulationResult> {
    use super::metrics::calculate_performance_metrics;
    use super::trading::{TradeContext, execute_trading_action, generate_api_predictions};
    use common::algorithm::momentum::execute_momentum_strategy;
    use common::algorithm::{TokenHolding, TradingAction};

    let duration = config.end_date - config.start_date;
    let duration_days = duration.num_days();
    // initial_value: NEAR単位（ユーザー入力）
    let initial_value = NearValueF64::from_near(
        config
            .initial_capital
            .to_string()
            .parse::<f64>()
            .unwrap_or(1000.0),
    );

    // タイムステップ設定
    let time_step = config.rebalance_interval.as_duration();

    let mut current_time = config.start_date;
    let mut portfolio_values = Vec::new();
    let mut trades = Vec::new();
    let mut current_holdings: HashMap<String, TokenAmountF64> = HashMap::new();
    let mut total_costs = NearValueF64::zero();

    // 初期ポートフォリオ設定（均等分散）
    let tokens_count = config.target_tokens.len() as f64;
    let initial_per_token = initial_value / tokens_count;

    // 初期価格データを取得（無次元比率: yoctoNEAR/smallest_unit = NEAR/token）
    let initial_prices: HashMap<String, TokenPriceF64> =
        get_prices_at_time(price_data, config.start_date)?;

    for token in &config.target_tokens {
        if let Some(&initial_price) = initial_prices.get(token) {
            // initial_per_token: NEAR単位 (NearValueF64)
            // initial_price: 無次元比率 (yoctoNEAR/smallest_unit = NEAR/token)
            // → トークン数量を計算: NEAR / (NEAR/token) = token
            // ただし price は yoctoNEAR/smallest_unit 単位なので、
            // amount (smallest_unit) = value (yoctoNEAR) / price (yoctoNEAR/smallest_unit)
            // value を yoctoNEAR に変換してから計算
            let initial_value_yocto = initial_per_token.to_yocto();
            let token_amount = initial_value_yocto / initial_price;
            current_holdings.insert(token.clone(), token_amount);
        } else {
            return Err(anyhow::anyhow!(
                "No price data found for token: {} at start date",
                token
            ));
        }
    }

    let mut step_count = 0;
    let max_steps = 1000;
    let mut gap_events = Vec::new();
    let mut total_timesteps = 0;
    let mut skipped_timesteps = 0;
    let mut last_successful_time: Option<DateTime<Utc>> = None;

    while current_time <= config.end_date && step_count < max_steps {
        step_count += 1;
        total_timesteps += 1;

        // 価格データ取得を試行（yoctoNEAR/token単位）
        match get_prices_at_time_optional(price_data, current_time) {
            Some(current_prices) => {
                // データがある場合：通常の取引処理
                last_successful_time = Some(current_time);

                // 予測データを生成（モックまたはAPI）
                let backend_client = crate::api::backend::BackendClient::new();
                let predictions = generate_api_predictions(
                    &backend_client,
                    &config.target_tokens,
                    &config.quote_token,
                    current_time,
                    config.historical_days,
                    config.prediction_horizon,
                    config.model.clone(),
                    config.verbose,
                )
                .await?;

                // TokenHoldingに変換（型安全な変換メソッドを使用）
                let mut token_holdings = Vec::new();
                for (token, amount) in &current_holdings {
                    if let Some(&price) = current_prices.get(token) {
                        // String を TokenOutAccount に変換
                        let token_out: TokenOutAccount = match token.parse() {
                            Ok(t) => t,
                            Err(_) => continue,
                        };
                        // amount: TokenAmountF64 -> TokenAmount
                        // amount.decimals() と price は同じ decimals を使用
                        token_holdings.push(TokenHolding {
                            token: token_out,
                            amount: amount.to_bigdecimal(),
                            current_rate: ExchangeRate::from_price(
                                &price.to_bigdecimal(),
                                amount.decimals(),
                            ),
                        });
                    }
                }

                // Momentum戦略を実行
                if !token_holdings.is_empty() && !predictions.is_empty() {
                    // f64 を NearValue に変換（NEAR 単位として解釈）
                    let min_trade_value = NearValue::from_near(
                        BigDecimal::from_f64(config.momentum_min_trade_amount).unwrap_or_default(),
                    );

                    let execution_report = execute_momentum_strategy(
                        token_holdings,
                        &predictions,
                        config.momentum_min_profit_threshold,
                        config.momentum_switch_multiplier,
                        &min_trade_value,
                    )
                    .await?;

                    // 取引アクションを実行
                    for action in execution_report.actions {
                        match action {
                            TradingAction::Sell {
                                ref token,
                                ref target,
                            } => {
                                let token_str = token.to_string();
                                let target_str = target.to_string();
                                if let (
                                    Some(&current_amount),
                                    Some(&current_price),
                                    Some(&_target_price),
                                ) = (
                                    current_holdings.get(&token_str),
                                    current_prices.get(&token_str),
                                    current_prices.get(&target_str),
                                ) {
                                    let mut trade_ctx = TradeContext {
                                        current_token: &token_str,
                                        current_amount,
                                        current_price,
                                        all_prices: &current_prices,
                                        holdings: &mut current_holdings,
                                        timestamp: current_time,
                                        config,
                                    };

                                    if let Ok(Some(trade)) = execute_trading_action(
                                        TradingAction::Sell {
                                            token: token.clone(),
                                            target: target.clone(),
                                        },
                                        &mut trade_ctx,
                                    ) {
                                        let cost_f64 = trade
                                            .cost
                                            .total
                                            .to_string()
                                            .parse::<f64>()
                                            .unwrap_or(0.0);
                                        // 型安全な加算を使用: NearValueF64 + f64 = NearValueF64
                                        total_costs = total_costs + cost_f64;
                                        trades.push(trade);
                                    }
                                }
                            }
                            TradingAction::Switch { ref from, ref to } => {
                                let from_str = from.to_string();
                                let to_str = to.to_string();
                                if let (
                                    Some(&current_amount),
                                    Some(&from_price),
                                    Some(&_to_price),
                                ) = (
                                    current_holdings.get(&from_str),
                                    current_prices.get(&from_str),
                                    current_prices.get(&to_str),
                                ) {
                                    let mut trade_ctx = TradeContext {
                                        current_token: &from_str,
                                        current_amount,
                                        current_price: from_price,
                                        all_prices: &current_prices,
                                        holdings: &mut current_holdings,
                                        timestamp: current_time,
                                        config,
                                    };

                                    if let Ok(Some(trade)) = execute_trading_action(
                                        TradingAction::Switch {
                                            from: from.clone(),
                                            to: to.clone(),
                                        },
                                        &mut trade_ctx,
                                    ) {
                                        let cost_f64 = trade
                                            .cost
                                            .total
                                            .to_string()
                                            .parse::<f64>()
                                            .unwrap_or(0.0);
                                        // 型安全な加算を使用: NearValueF64 + f64 = NearValueF64
                                        total_costs = total_costs + cost_f64;
                                        trades.push(trade);
                                    }
                                }
                            }
                            _ => {} // Hold or other actions
                        }
                    }
                }

                // ポートフォリオ価値を計算
                let mut total_value = NearValueF64::zero();
                let mut holdings_value: HashMap<String, NearValueF64> = HashMap::new();

                for (token, amount) in &current_holdings {
                    if let Some(&price) = current_prices.get(token) {
                        // amount: TokenAmountF64 (smallest unit), price: TokenPriceF64 (無次元比率)
                        // amount * price = YoctoValueF64, then .to_near() = NearValueF64
                        let value_yocto = *amount * price;
                        let value_near = value_yocto.to_near();
                        holdings_value.insert(token.clone(), value_near);
                        total_value = total_value + value_near;
                    }
                }

                // ポートフォリオ記録
                portfolio_values.push(PortfolioValue {
                    timestamp: current_time,
                    total_value,
                    holdings: holdings_value,
                    cash_balance: NearValueF64::zero(),
                    unrealized_pnl: total_value - initial_value,
                });
            }
            None => {
                // データが不足している場合：スキップ
                skipped_timesteps += 1;

                // スキップイベントを記録
                let gap_event = DataGapEvent {
                    timestamp: current_time,
                    event_type: DataGapEventType::TradingSkipped,
                    affected_tokens: config.target_tokens.clone(),
                    reason: "No price data found within 1 hour of target time".to_string(),
                    impact: calculate_gap_impact(
                        last_successful_time,
                        current_time,
                        price_data,
                        &config.target_tokens,
                    ),
                };

                // ログに即座に出力（verbose無関係）
                log_data_gap_event(&gap_event);
                gap_events.push(gap_event);

                // ポートフォリオ評価は最後に取得できた価格で継続
                if let Some(evaluation_prices) =
                    get_last_known_prices_for_evaluation(price_data, current_time)
                {
                    // 評価のみ実行（取引はしない）
                    let mut total_value = NearValueF64::zero();
                    let mut holdings_value: HashMap<String, NearValueF64> = HashMap::new();

                    for (token, amount) in &current_holdings {
                        if let Some(&price) = evaluation_prices.get(token) {
                            let value_yocto = *amount * price;
                            let value_near = value_yocto.to_near();
                            holdings_value.insert(token.clone(), value_near);
                            total_value = total_value + value_near;
                        }
                    }

                    portfolio_values.push(PortfolioValue {
                        timestamp: current_time,
                        total_value,
                        holdings: holdings_value,
                        cash_balance: NearValueF64::zero(),
                        unrealized_pnl: total_value - initial_value,
                    });
                }
            }
        }

        // 次のステップへ
        current_time += time_step;
    }

    let final_value = portfolio_values
        .last()
        .map(|pv| pv.total_value)
        .unwrap_or(initial_value);

    // パフォーマンス指標を計算
    let performance = calculate_performance_metrics(
        initial_value,
        final_value,
        &portfolio_values,
        &trades,
        total_costs,
        config.start_date,
        config.end_date,
    )?;

    let config_summary = SimulationSummary {
        start_date: config.start_date,
        end_date: config.end_date,
        algorithm: AlgorithmType::Momentum,
        initial_capital: initial_value,
        final_value,
        total_return: final_value - initial_value,
        duration_days,
    };

    let execution_summary = ExecutionSummary {
        total_trades: trades.len(),
        successful_trades: trades.iter().filter(|t| t.success).count(),
        failed_trades: trades.iter().filter(|t| !t.success).count(),
        success_rate: if !trades.is_empty() {
            trades.iter().filter(|t| t.success).count() as f64 / trades.len() as f64 * 100.0
        } else {
            0.0
        },
        total_cost: total_costs,
        avg_cost_per_trade: if !trades.is_empty() {
            total_costs / trades.len() as f64
        } else {
            NearValueF64::zero()
        },
    };

    // データ品質統計を計算
    let data_coverage_percentage = if total_timesteps > 0 {
        ((total_timesteps - skipped_timesteps) as f64 / total_timesteps as f64) * 100.0
    } else {
        0.0
    };

    let longest_gap_hours = gap_events
        .iter()
        .map(|e| e.impact.duration_hours)
        .max()
        .unwrap_or(0);

    let data_quality = DataQualityStats {
        total_timesteps,
        skipped_timesteps,
        data_coverage_percentage,
        longest_gap_hours,
        gap_events,
    };

    Ok(SimulationResult {
        config: config_summary,
        performance,
        trades,
        portfolio_values,
        execution_summary,
        data_quality,
    })
}

/// Run portfolio optimization simulation using common crate algorithm
pub(crate) async fn run_portfolio_optimization_simulation(
    config: &SimulationConfig,
    price_data: &HashMap<String, Vec<common::stats::ValueAtTime>>,
) -> Result<SimulationResult> {
    use super::metrics::calculate_performance_metrics;
    use super::trading::generate_api_predictions;
    use bigdecimal::{BigDecimal, FromPrimitive};
    use common::algorithm::portfolio::{PortfolioData, execute_portfolio_optimization};
    use common::algorithm::{PriceHistory, PricePoint, TokenData, TradingAction, WalletInfo};
    use common::types::NearValue;

    let duration = config.end_date - config.start_date;
    let duration_days = duration.num_days();
    // initial_value: NEAR単位（ユーザー入力）
    let initial_value = NearValueF64::from_near(
        config
            .initial_capital
            .to_string()
            .parse::<f64>()
            .unwrap_or(1000.0),
    );

    // タイムステップ設定
    let time_step = config.rebalance_interval.as_duration();

    // Token decimals のローカルキャッシュ（リバランス時に遅延取得）
    let mut decimals_cache = super::token_decimals_cache::TokenDecimalsCache::new();

    let mut current_time = config.start_date;
    let mut portfolio_values = Vec::new();
    let mut trades = Vec::new();
    let mut current_holdings: HashMap<String, TokenAmountF64> = HashMap::new();
    let mut total_costs = NearValueF64::zero();
    let mut last_rebalance_time = config.start_date;

    // 初期ポートフォリオ設定（均等分散）
    let tokens_count = config.target_tokens.len() as f64;
    let initial_per_token = initial_value / tokens_count;

    // 初期価格データを取得（無次元比率: yoctoNEAR/smallest_unit = NEAR/token）
    let initial_prices: HashMap<String, TokenPriceF64> =
        get_prices_at_time(price_data, config.start_date)?;

    for token in &config.target_tokens {
        if let Some(&initial_price) = initial_prices.get(token) {
            // initial_per_token: NEAR単位 (NearValueF64)
            // initial_price: 無次元比率 (yoctoNEAR/smallest_unit = NEAR/token)
            // → トークン数量を計算
            let initial_value_yocto = initial_per_token.to_yocto();
            let token_amount = initial_value_yocto / initial_price;
            current_holdings.insert(token.clone(), token_amount);
        } else {
            return Err(anyhow::anyhow!(
                "No price data found for token: {} at start date",
                token
            ));
        }
    }

    let mut step_count = 0;
    let max_steps = 1000;
    let mut gap_events = Vec::new();
    let mut total_timesteps = 0;
    let mut skipped_timesteps = 0;
    let mut last_successful_time: Option<DateTime<Utc>> = None;

    while current_time <= config.end_date && step_count < max_steps {
        step_count += 1;
        total_timesteps += 1;

        // 価格データ取得を試行（yoctoNEAR/token単位）
        match get_prices_at_time_optional(price_data, current_time) {
            Some(current_prices) => {
                // データがある場合：通常の処理
                last_successful_time = Some(current_time);

                // リバランスが必要かどうかチェック（設定された期間に基づく）
                let portfolio_rebalance_duration =
                    config.portfolio_rebalance_interval.as_duration();
                let should_rebalance =
                    current_time >= last_rebalance_time + portfolio_rebalance_duration;

                if should_rebalance && !current_holdings.is_empty() {
                    // 予測データを生成
                    let backend_client = crate::api::backend::BackendClient::new();
                    let predictions = generate_api_predictions(
                        &backend_client,
                        &config.target_tokens,
                        &config.quote_token,
                        current_time,
                        config.historical_days,
                        config.prediction_horizon,
                        config.model.clone(),
                        config.verbose,
                    )
                    .await?;

                    // TokenDataに変換（型安全な変換メソッドを使用）
                    let mut token_data = Vec::new();
                    for token in &config.target_tokens {
                        if let Some(&current_price) = current_prices.get(token) {
                            let token_out: TokenOutAccount = match token.parse() {
                                Ok(t) => t,
                                Err(_) => continue,
                            };
                            let decimals = decimals_cache.resolve(&backend_client, token).await?;
                            token_data.push(TokenData {
                                symbol: token_out,
                                current_rate: ExchangeRate::from_price(
                                    &current_price.to_bigdecimal(),
                                    decimals,
                                ),
                                historical_volatility: 0.2, // デフォルト値
                                liquidity_score: Some(0.8),
                                market_cap: None,
                            });
                        }
                    }

                    // ポートフォリオデータを構築（TokenPrice で型安全）
                    // TokenOutAccount をキーとして使用
                    let mut predictions_map: HashMap<TokenOutAccount, TokenPrice> = HashMap::new();
                    for pred in predictions {
                        // PredictionData.token は既に TokenOutAccount
                        predictions_map.insert(pred.token.clone(), pred.predicted_price_24h);
                    }

                    // 履歴価格データを構築（簡略版）
                    let historical_prices: Vec<PriceHistory> = config
                        .target_tokens
                        .iter()
                        .filter_map(|token| {
                            let prices = if let Some(data) = price_data.get(token) {
                                data.iter()
                                    .take(30)
                                    .map(|point| PricePoint {
                                        timestamp: chrono::DateTime::from_naive_utc_and_offset(
                                            point.time,
                                            chrono::Utc,
                                        ),
                                        price: point.value.clone(),
                                        volume: None,
                                    })
                                    .collect()
                            } else {
                                Vec::new()
                            };

                            Some(PriceHistory {
                                token: token.parse().ok()?,
                                quote_token: config.quote_token.parse().ok()?,
                                prices,
                            })
                        })
                        .collect();

                    let portfolio_data = PortfolioData {
                        tokens: token_data,
                        predictions: predictions_map.into_iter().collect(),
                        historical_prices,
                        prediction_confidence: None,
                    };

                    // 現在のホールディングをWalletInfoに変換（TokenAmount）
                    let mut holdings_for_wallet = BTreeMap::new();
                    for (token, amount) in &current_holdings {
                        // String → TokenOutAccount、TokenAmountF64 → TokenAmount
                        if let Ok(token_out) = token.parse::<TokenOutAccount>() {
                            holdings_for_wallet.insert(token_out, amount.to_bigdecimal());
                        }
                    }

                    // 総価値を計算（NEAR単位、BigDecimal精度）
                    let total_value_near: NearValue = current_holdings
                        .iter()
                        .map(|(token, amount)| {
                            if let Some(&price) = current_prices.get(token) {
                                // f64で計算してからBigDecimalに変換
                                let value_yocto = *amount * price;
                                let value_near_f64 = value_yocto.to_near().as_f64();
                                NearValue::from_near(
                                    BigDecimal::from_f64(value_near_f64).unwrap_or_default(),
                                )
                            } else {
                                NearValue::zero()
                            }
                        })
                        .fold(NearValue::zero(), |acc, v| acc + v);

                    let wallet_info = WalletInfo {
                        holdings: holdings_for_wallet,
                        total_value: total_value_near,
                        cash_balance: NearValue::zero(),
                    };

                    // ポートフォリオ最適化を実行
                    {
                        if let Ok(execution_report) = execute_portfolio_optimization(
                            &wallet_info,
                            portfolio_data,
                            config.portfolio_rebalance_threshold,
                        )
                        .await
                        {
                            // リバランスアクションを実行
                            for action in execution_report.actions {
                                if let TradingAction::Rebalance { target_weights } = action {
                                    // 現在の総価値を型安全に計算（NEAR単位）
                                    let mut total_portfolio_value = NearValueF64::zero();
                                    for (token, amount) in &current_holdings {
                                        if let Some(&price) = current_prices.get(token) {
                                            let value_yocto = *amount * price;
                                            let value_near = value_yocto.to_near();
                                            total_portfolio_value =
                                                total_portfolio_value + value_near;
                                        }
                                    }

                                    // 目標配分に基づいてリバランス
                                    for (token, target_weight) in target_weights {
                                        // TokenOutAccount → String for HashMap access
                                        let token_str = token.to_string();
                                        if let Some(&current_price) = current_prices.get(&token_str)
                                        {
                                            // 目標価値を計算（NEAR単位）
                                            // 型安全な乗算を使用: NearValueF64 * f64 = NearValueF64
                                            let target_value_near =
                                                total_portfolio_value * target_weight;
                                            // 目標価値をyoctoNEARに変換して数量を計算
                                            let target_value_yocto = target_value_near.to_yocto();
                                            let target_amount = target_value_yocto / current_price;

                                            // 現実的な数量制限を適用
                                            let decimals = decimals_cache
                                                .resolve(&backend_client, &token_str)
                                                .await?;
                                            // 最大 1000 whole tokens
                                            let max_reasonable_amount =
                                                TokenAmountF64::from_whole_tokens(1000.0, decimals);
                                            let target_amount_limited =
                                                if target_amount > max_reasonable_amount {
                                                    max_reasonable_amount
                                                } else {
                                                    target_amount
                                                };

                                            // 現在の保有量と目標量の差を計算
                                            let current_amount = current_holdings
                                                .get(&token_str)
                                                .copied()
                                                .unwrap_or(TokenAmountF64::zero(decimals));
                                            // 型安全な演算子を使用
                                            let diff_amount =
                                                target_amount_limited - current_amount;
                                            let diff_abs = diff_amount.abs();

                                            // 相対的な閾値: 現在保有量の1%以上の差でリバランス
                                            let relative_threshold = current_amount * 0.01;
                                            // 最小絶対閾値（0.001 whole tokens）
                                            let min_threshold =
                                                TokenAmountF64::from_whole_tokens(0.001, decimals);
                                            let effective_threshold =
                                                if relative_threshold > min_threshold {
                                                    relative_threshold
                                                } else {
                                                    min_threshold
                                                };

                                            if diff_abs > effective_threshold {
                                                // 保有量の1%以上の差がある場合のみリバランス
                                                current_holdings.insert(
                                                    token_str.clone(),
                                                    target_amount_limited,
                                                );
                                                // 絶対値でコスト計算（yoctoNEAR）
                                                let diff_value_yocto = diff_abs * current_price;
                                                let diff_value_near = diff_value_yocto.to_near();
                                                // 型安全な乗算・加算を使用
                                                let trade_cost = diff_value_near * 0.003; // 0.3%手数料
                                                total_costs = total_costs + trade_cost;

                                                // ガスコストをyoctoNEARに変換
                                                let gas_cost_yocto = NearValueF64::from_near(
                                                    config
                                                        .gas_cost
                                                        .to_string()
                                                        .parse::<f64>()
                                                        .unwrap_or(0.01),
                                                )
                                                .to_yocto();

                                                // TradingCost を YoctoValueF64 で構築
                                                let trade_cost_yocto = trade_cost.to_yocto();
                                                let trading_cost = TradingCost {
                                                    protocol_fee: trade_cost_yocto * 0.7,
                                                    slippage: trade_cost_yocto * 0.2,
                                                    gas_fee: gas_cost_yocto,
                                                    total: trade_cost_yocto,
                                                };

                                                // TradeExecutionを記録
                                                trades.push(TradeExecution {
                                                    timestamp: current_time,
                                                    from_token: config.quote_token.clone(),
                                                    to_token: token_str.clone(),
                                                    amount: diff_abs,
                                                    executed_price: current_price,
                                                    cost: trading_cost,
                                                    portfolio_value_before: total_portfolio_value,
                                                    // 型安全な減算を使用: NearValueF64 - NearValueF64
                                                    portfolio_value_after: total_portfolio_value
                                                        - trade_cost,
                                                    success: true,
                                                    reason: format!(
                                                        "Portfolio rebalancing: {} -> {:.1}%",
                                                        token,
                                                        target_weight * 100.0
                                                    ),
                                                });
                                            }
                                        }
                                    }
                                }
                            }

                            last_rebalance_time = current_time;
                        }
                    }
                }

                // ポートフォリオ価値を計算
                let mut total_value = NearValueF64::zero();
                let mut holdings_value: HashMap<String, NearValueF64> = HashMap::new();

                for (token, amount) in &current_holdings {
                    if let Some(&price) = current_prices.get(token) {
                        // 型安全な計算
                        let value_yocto = *amount * price;
                        let value_near = value_yocto.to_near();
                        holdings_value.insert(token.clone(), value_near);
                        total_value = total_value + value_near;
                    }
                }

                // ポートフォリオ記録
                portfolio_values.push(PortfolioValue {
                    timestamp: current_time,
                    total_value,
                    holdings: holdings_value,
                    cash_balance: NearValueF64::zero(),
                    unrealized_pnl: total_value - initial_value,
                });
            }
            None => {
                // データが不足している場合：スキップ
                skipped_timesteps += 1;

                // スキップイベントを記録
                let gap_event = DataGapEvent {
                    timestamp: current_time,
                    event_type: DataGapEventType::RebalanceSkipped,
                    affected_tokens: config.target_tokens.clone(),
                    reason: "No price data found within 1 hour of target time".to_string(),
                    impact: calculate_gap_impact(
                        last_successful_time,
                        current_time,
                        price_data,
                        &config.target_tokens,
                    ),
                };

                // ログに即座に出力
                log_data_gap_event(&gap_event);
                gap_events.push(gap_event);

                // ポートフォリオ評価は最後に取得できた価格で継続
                if let Some(evaluation_prices) =
                    get_last_known_prices_for_evaluation(price_data, current_time)
                {
                    let mut total_value = NearValueF64::zero();
                    let mut holdings_value: HashMap<String, NearValueF64> = HashMap::new();

                    for (token, amount) in &current_holdings {
                        if let Some(&price) = evaluation_prices.get(token) {
                            let value_yocto = *amount * price;
                            let value_near = value_yocto.to_near();
                            holdings_value.insert(token.clone(), value_near);
                            total_value = total_value + value_near;
                        }
                    }

                    portfolio_values.push(PortfolioValue {
                        timestamp: current_time,
                        total_value,
                        holdings: holdings_value,
                        cash_balance: NearValueF64::zero(),
                        unrealized_pnl: total_value - initial_value,
                    });
                }
            }
        }

        // 次のステップへ
        current_time += time_step;
    }

    let final_value = portfolio_values
        .last()
        .map(|pv| pv.total_value)
        .unwrap_or(initial_value);

    // パフォーマンス指標を計算
    let performance = calculate_performance_metrics(
        initial_value,
        final_value,
        &portfolio_values,
        &trades,
        total_costs,
        config.start_date,
        config.end_date,
    )?;

    let config_summary = SimulationSummary {
        start_date: config.start_date,
        end_date: config.end_date,
        algorithm: AlgorithmType::Portfolio,
        initial_capital: initial_value,
        final_value,
        total_return: final_value - initial_value,
        duration_days,
    };

    let execution_summary = ExecutionSummary {
        total_trades: trades.len(),
        successful_trades: trades.iter().filter(|t| t.success).count(),
        failed_trades: trades.iter().filter(|t| !t.success).count(),
        success_rate: if !trades.is_empty() {
            trades.iter().filter(|t| t.success).count() as f64 / trades.len() as f64 * 100.0
        } else {
            0.0
        },
        total_cost: total_costs,
        avg_cost_per_trade: if !trades.is_empty() {
            total_costs / trades.len() as f64
        } else {
            NearValueF64::zero()
        },
    };

    // データ品質統計を計算
    let data_coverage_percentage = if total_timesteps > 0 {
        ((total_timesteps - skipped_timesteps) as f64 / total_timesteps as f64) * 100.0
    } else {
        0.0
    };

    let longest_gap_hours = gap_events
        .iter()
        .map(|e| e.impact.duration_hours)
        .max()
        .unwrap_or(0);

    let data_quality = DataQualityStats {
        total_timesteps,
        skipped_timesteps,
        data_coverage_percentage,
        longest_gap_hours,
        gap_events,
    };

    Ok(SimulationResult {
        config: config_summary,
        performance,
        trades,
        portfolio_values,
        execution_summary,
        data_quality,
    })
}

#[cfg(test)]
mod tests;
