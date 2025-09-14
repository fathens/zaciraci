use super::data::{fetch_price_data, get_prices_at_time};
use super::types::*;
use crate::api::backend::BackendClient;
use anyhow::Result;
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

/// Run trend following simulation
pub async fn run_trend_following_simulation(config: &SimulationConfig) -> Result<SimulationResult> {
    if config.verbose {
        println!("📉 Running trend following simulation");
        println!("📊 Following trends for tokens: {:?}", config.target_tokens);
    }

    let backend_client = BackendClient::new();

    // 1. 価格データを取得（キャッシュ対応）
    let price_data = fetch_price_data(&backend_client, config).await?;

    if price_data.is_empty() {
        return Err(anyhow::anyhow!(
            "No price data available for simulation period. Please check your backend connection and ensure price data exists for the specified tokens and time period."
        ));
    }

    // 2. TrendFollowingアルゴリズムでシミュレーション実行（commonクレート使用）
    run_trend_following_optimization_simulation(config, &price_data).await
}

/// Run momentum timestep simulation using common crate algorithm
pub(crate) async fn run_momentum_timestep_simulation(
    config: &SimulationConfig,
    price_data: &HashMap<String, Vec<common::stats::ValueAtTime>>,
) -> Result<SimulationResult> {
    use super::metrics::calculate_performance_metrics;
    use super::trading::{execute_trading_action, generate_api_predictions, TradeContext};
    use bigdecimal::{BigDecimal, FromPrimitive};
    use common::algorithm::momentum::execute_momentum_strategy;
    use common::algorithm::{TokenHolding, TradingAction};

    let duration = config.end_date - config.start_date;
    let duration_days = duration.num_days();
    let initial_value = config
        .initial_capital
        .to_string()
        .parse::<f64>()
        .unwrap_or(1000.0);

    // タイムステップ設定
    let time_step = config.rebalance_interval.as_duration();

    let mut current_time = config.start_date;
    let mut portfolio_values = Vec::new();
    let mut trades = Vec::new();
    let mut current_holdings = HashMap::new();
    let mut total_costs = 0.0;

    // 初期ポートフォリオ設定（均等分散）
    let tokens_count = config.target_tokens.len() as f64;
    let initial_per_token = initial_value / tokens_count;

    // 初期価格データを取得
    let initial_prices = get_prices_at_time(price_data, config.start_date)?;

    for token in &config.target_tokens {
        if let Some(&initial_price_yocto) = initial_prices.get(token) {
            // initial_per_tokenはNEAR単位、initial_price_yoctoはyoctoNEAR単位
            // NEAR単位に変換してから計算
            let initial_price_near =
                common::units::Units::yocto_f64_to_near_f64(initial_price_yocto);
            let token_amount = initial_per_token / initial_price_near;
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

    while current_time <= config.end_date && step_count < max_steps {
        step_count += 1;

        // 現在時点での価格を取得
        let current_prices = get_prices_at_time(price_data, current_time)?;

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

        // TokenHoldingに変換
        let mut token_holdings = Vec::new();
        for (token, amount) in &current_holdings {
            if let Some(&price) = current_prices.get(token) {
                token_holdings.push(TokenHolding {
                    token: token.clone(),
                    amount: BigDecimal::from_f64(*amount).unwrap_or_default(),
                    current_price: BigDecimal::from_f64(price).unwrap_or_default(),
                });
            }
        }

        // Momentum戦略を実行
        if !token_holdings.is_empty() && !predictions.is_empty() {
            let execution_report = execute_momentum_strategy(
                token_holdings,
                predictions,
                config.momentum_min_profit_threshold,
                config.momentum_switch_multiplier,
                config.momentum_min_trade_amount,
            )
            .await?;

            // 取引アクションを実行
            for action in execution_report.actions {
                match action {
                    TradingAction::Sell { token, target } => {
                        if let (Some(&current_amount), Some(&current_price), Some(&_target_price)) = (
                            current_holdings.get(&token),
                            current_prices.get(&token),
                            current_prices.get(&target),
                        ) {
                            let mut trade_ctx = TradeContext {
                                current_token: &token,
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
                                total_costs +=
                                    trade.cost.total.to_string().parse::<f64>().unwrap_or(0.0);
                                trades.push(trade);
                            }
                        }
                    }
                    TradingAction::Switch { from, to } => {
                        if let (Some(&current_amount), Some(&from_price), Some(&_to_price)) = (
                            current_holdings.get(&from),
                            current_prices.get(&from),
                            current_prices.get(&to),
                        ) {
                            let mut trade_ctx = TradeContext {
                                current_token: &from,
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
                                total_costs +=
                                    trade.cost.total.to_string().parse::<f64>().unwrap_or(0.0);
                                trades.push(trade);
                            }
                        }
                    }
                    _ => {} // Hold or other actions
                }
            }
        }

        // ポートフォリオ価値を計算
        let mut total_value = 0.0;
        let mut holdings_value = HashMap::new();

        for (token, amount) in &current_holdings {
            if let Some(&price_yocto) = current_prices.get(token) {
                // priceはyoctoNEAR単位、amountはトークン数量
                // 計算結果をNEAR単位に変換
                let price_near = common::units::Units::yocto_f64_to_near_f64(price_yocto);
                let value = amount * price_near;
                holdings_value.insert(token.clone(), value);
                total_value += value;
            }
        }

        // ポートフォリオ記録
        portfolio_values.push(PortfolioValue {
            timestamp: current_time,
            total_value,
            holdings: holdings_value.into_iter().collect(),
            cash_balance: 0.0,
            unrealized_pnl: total_value - initial_value,
        });

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
            0.0
        },
    };

    Ok(SimulationResult {
        config: config_summary,
        performance,
        trades,
        portfolio_values,
        execution_summary,
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
    use common::algorithm::portfolio::{execute_portfolio_optimization, PortfolioData};
    use common::algorithm::{PriceHistory, PricePoint, TokenData, TradingAction, WalletInfo};

    let duration = config.end_date - config.start_date;
    let duration_days = duration.num_days();
    let initial_value = config
        .initial_capital
        .to_string()
        .parse::<f64>()
        .unwrap_or(1000.0);

    // タイムステップ設定
    let time_step = config.rebalance_interval.as_duration();

    let mut current_time = config.start_date;
    let mut portfolio_values = Vec::new();
    let mut trades = Vec::new();
    let mut current_holdings = HashMap::new();
    let mut total_costs = 0.0;
    let mut last_rebalance_time = config.start_date;

    // 初期ポートフォリオ設定（均等分散）
    let tokens_count = config.target_tokens.len() as f64;
    let initial_per_token = initial_value / tokens_count;

    // 初期価格データを取得
    let initial_prices = get_prices_at_time(price_data, config.start_date)?;

    for token in &config.target_tokens {
        if let Some(&initial_price_yocto) = initial_prices.get(token) {
            // initial_per_tokenはNEAR単位、initial_price_yoctoはyoctoNEAR単位
            // NEAR単位に変換してから計算
            let initial_price_near =
                common::units::Units::yocto_f64_to_near_f64(initial_price_yocto);
            let token_amount = initial_per_token / initial_price_near;
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

    while current_time <= config.end_date && step_count < max_steps {
        step_count += 1;

        // 現在時点での価格を取得
        let current_prices = get_prices_at_time(price_data, current_time)?;

        // リバランスが必要かどうかチェック（設定された期間に基づく）
        let portfolio_rebalance_duration = config.portfolio_rebalance_interval.as_duration();
        let should_rebalance = current_time >= last_rebalance_time + portfolio_rebalance_duration;

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

            // TokenDataに変換
            let mut token_data = Vec::new();
            for token in &config.target_tokens {
                if let Some(&current_price_yocto) = current_prices.get(token) {
                    // current_priceはyoctoNEAR単位として保存
                    token_data.push(TokenData {
                        symbol: token.clone(),
                        current_price: BigDecimal::from_f64(current_price_yocto)
                            .unwrap_or_default(),
                        historical_volatility: 0.2, // デフォルト値
                        liquidity_score: Some(0.8),
                        market_cap: None,
                        decimals: Some(18),
                    });
                }
            }

            // ポートフォリオデータを構築
            let mut predictions_map = HashMap::new();
            for pred in predictions {
                let predicted_price = pred
                    .predicted_price_24h
                    .to_string()
                    .parse::<f64>()
                    .unwrap_or(0.0);
                predictions_map.insert(pred.token, predicted_price);
            }

            // 履歴価格データを構築（簡略版）
            let historical_prices: Vec<PriceHistory> = config
                .target_tokens
                .iter()
                .map(|token| {
                    let prices = if let Some(data) = price_data.get(token) {
                        data.iter()
                            .take(30)
                            .map(|point| PricePoint {
                                timestamp: chrono::DateTime::from_naive_utc_and_offset(
                                    point.time,
                                    chrono::Utc,
                                ),
                                // point.valueはyoctoNEAR単位で保存
                                price: BigDecimal::from_f64(point.value).unwrap_or_default(),
                                volume: None,
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };

                    PriceHistory {
                        token: token.clone(),
                        quote_token: config.quote_token.clone(),
                        prices,
                    }
                })
                .collect();

            let portfolio_data = PortfolioData {
                tokens: token_data,
                predictions: predictions_map.into_iter().collect(),
                historical_prices,
                correlation_matrix: None,
            };

            // 現在のホールディングをWalletInfoに変換
            let mut holdings_for_wallet = BTreeMap::new();
            for (token, amount) in &current_holdings {
                holdings_for_wallet.insert(token.clone(), *amount);
            }

            let wallet_info = WalletInfo {
                holdings: holdings_for_wallet,
                total_value: current_holdings
                    .iter()
                    .map(|(token, amount)| {
                        if let Some(&price_yocto) = current_prices.get(token) {
                            // 価格をNEAR単位に変換してから計算
                            let price_near =
                                common::units::Units::yocto_f64_to_near_f64(price_yocto);
                            amount * price_near
                        } else {
                            0.0
                        }
                    })
                    .sum(),
                cash_balance: 0.0,
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
                            // 現在の総価値を計算（NEAR単位）
                            let mut total_portfolio_value = 0.0;
                            for (token, amount) in &current_holdings {
                                if let Some(&price_yocto) = current_prices.get(token) {
                                    let price_near =
                                        common::units::Units::yocto_f64_to_near_f64(price_yocto);
                                    total_portfolio_value += amount * price_near;
                                }
                            }

                            // 目標配分に基づいてリバランス
                            for (token, target_weight) in target_weights {
                                if let Some(&current_price_yocto) = current_prices.get(&token) {
                                    let current_price_near =
                                        common::units::Units::yocto_f64_to_near_f64(
                                            current_price_yocto,
                                        );
                                    let target_value = total_portfolio_value * target_weight;
                                    let target_amount = target_value / current_price_near;

                                    // 現在の保有量と目標量の差を計算
                                    let current_amount =
                                        current_holdings.get(&token).copied().unwrap_or(0.0);
                                    let diff = target_amount - current_amount;

                                    // 相対的な閾値: 現在保有量の1%以上の差でリバランス
                                    let relative_threshold = current_amount * 0.01;
                                    let min_threshold = 0.001; // 最小絶対閾値
                                    let effective_threshold = relative_threshold.max(min_threshold);

                                    if diff.abs() > effective_threshold {
                                        // 保有量の1%以上の差がある場合のみリバランス
                                        current_holdings.insert(token.clone(), target_amount);

                                        // 簡易的な取引コスト計算（NEAR単位）
                                        let trade_cost = diff.abs() * current_price_near * 0.003; // 0.3%手数料
                                        total_costs += trade_cost;

                                        // TradeExecutionを記録
                                        trades.push(TradeExecution {
                                            timestamp: current_time,
                                            from_token: config.quote_token.clone(),
                                            to_token: token.clone(),
                                            amount: diff.abs(),
                                            executed_price: current_price_near,
                                            cost: TradingCost {
                                                protocol_fee: BigDecimal::from_f64(
                                                    trade_cost * 0.7,
                                                )
                                                .unwrap_or_default(),
                                                slippage: BigDecimal::from_f64(trade_cost * 0.2)
                                                    .unwrap_or_default(),
                                                gas_fee: config.gas_cost.clone(),
                                                total: BigDecimal::from_f64(trade_cost)
                                                    .unwrap_or_default(),
                                            },
                                            portfolio_value_before: total_portfolio_value,
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
        let mut total_value = 0.0;
        let mut holdings_value = HashMap::new();

        for (token, amount) in &current_holdings {
            if let Some(&price_yocto) = current_prices.get(token) {
                // priceはyoctoNEAR単位、amountはトークン数量
                // 計算結果をNEAR単位に変換
                let price_near = common::units::Units::yocto_f64_to_near_f64(price_yocto);
                let value = amount * price_near;
                holdings_value.insert(token.clone(), value);
                total_value += value;
            }
        }

        // ポートフォリオ記録
        portfolio_values.push(PortfolioValue {
            timestamp: current_time,
            total_value,
            holdings: holdings_value.into_iter().collect(),
            cash_balance: 0.0,
            unrealized_pnl: total_value - initial_value,
        });

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
            0.0
        },
    };

    Ok(SimulationResult {
        config: config_summary,
        performance,
        trades,
        portfolio_values,
        execution_summary,
    })
}

/// Run trend following optimization simulation using common crate algorithm
pub(crate) async fn run_trend_following_optimization_simulation(
    config: &SimulationConfig,
    price_data: &HashMap<String, Vec<common::stats::ValueAtTime>>,
) -> Result<SimulationResult> {
    use super::metrics::calculate_performance_metrics;
    use bigdecimal::{BigDecimal, FromPrimitive};
    use common::algorithm::trend_following::{
        execute_trend_following_strategy, TrendFollowingParams, TrendPosition,
    };
    use common::algorithm::TradingAction;

    let duration = config.end_date - config.start_date;
    let duration_days = duration.num_days();
    let initial_value = config
        .initial_capital
        .to_string()
        .parse::<f64>()
        .unwrap_or(1000.0);

    // タイムステップ設定
    let time_step = config.rebalance_interval.as_duration();

    let mut current_time = config.start_date;
    let mut portfolio_values = Vec::new();
    let mut trades = Vec::new();
    let mut current_holdings = HashMap::new();
    let mut total_costs = 0.0;
    let mut current_positions: Vec<TrendPosition> = Vec::new();

    // 初期ポートフォリオ設定（均等分散）
    let tokens_count = config.target_tokens.len() as f64;
    let initial_per_token = initial_value / tokens_count;

    // 初期価格データを取得
    let initial_prices = get_prices_at_time(price_data, config.start_date)?;

    for token in &config.target_tokens {
        if let Some(&initial_price) = initial_prices.get(token) {
            let token_amount = initial_per_token / initial_price;
            current_holdings.insert(token.clone(), token_amount);

            // 初期ポジションを作成
            current_positions.push(TrendPosition {
                token: token.clone(),
                size: token_amount,
                entry_price: BigDecimal::from_f64(initial_price).unwrap_or_default(),
                entry_time: current_time,
                current_price: BigDecimal::from_f64(initial_price).unwrap_or_default(),
                unrealized_pnl: 0.0,
            });
        } else {
            return Err(anyhow::anyhow!(
                "No price data found for token: {} at start date",
                token
            ));
        }
    }

    let mut step_count = 0;
    let max_steps = 1000;
    let mut available_capital = initial_value * 0.2; // 20%を新規投資用に確保

    while current_time <= config.end_date && step_count < max_steps {
        step_count += 1;

        // 現在時点での価格を取得
        let current_prices = get_prices_at_time(price_data, current_time)?;

        // 市場データを構築（トレンドフォロー用）
        let mut market_data = HashMap::new();
        for token in &config.target_tokens {
            if let Some(token_data) = price_data.get(token) {
                // 過去30個のデータポイントを取得
                let end_index = token_data
                    .iter()
                    .position(|point| {
                        let point_time: chrono::DateTime<chrono::Utc> =
                            chrono::DateTime::from_naive_utc_and_offset(point.time, chrono::Utc);
                        point_time >= current_time
                    })
                    .unwrap_or(token_data.len());

                let start_index = end_index.saturating_sub(30);
                let recent_data: Vec<f64> = token_data[start_index..end_index]
                    .iter()
                    .map(|p| p.value)
                    .collect();
                let timestamps: Vec<chrono::DateTime<chrono::Utc>> = token_data
                    [start_index..end_index]
                    .iter()
                    .map(|p| chrono::DateTime::from_naive_utc_and_offset(p.time, chrono::Utc))
                    .collect();

                if recent_data.len() >= 3 {
                    let default_volume = vec![1000.0; timestamps.len()];
                    // MarketDataTuple = (prices, timestamps, volumes, highs, lows)
                    market_data.insert(
                        token.clone(),
                        (
                            recent_data.clone(),
                            timestamps.clone(),
                            default_volume.clone(), // volumes
                            recent_data.clone(),    // highs (using prices as approximation)
                            recent_data,            // lows (using prices as approximation)
                        ),
                    );
                }
            }
        }

        // ポジションの現在価格を更新
        for position in &mut current_positions {
            if let Some(&current_price) = current_prices.get(&position.token) {
                position.current_price = BigDecimal::from_f64(current_price).unwrap_or_default();
                let entry_price_f64 = position
                    .entry_price
                    .to_string()
                    .parse::<f64>()
                    .unwrap_or(0.0);
                if entry_price_f64 > 0.0 {
                    position.unrealized_pnl =
                        (current_price - entry_price_f64) / entry_price_f64 * 100.0;
                }
            }
        }

        // トレンドフォロー戦略を実行
        if !market_data.is_empty() {
            if let Ok(execution_report) = execute_trend_following_strategy(
                config.target_tokens.clone(),
                current_positions.clone(),
                available_capital,
                &market_data,
                TrendFollowingParams {
                    rsi_overbought: config.trend_rsi_overbought,
                    rsi_oversold: config.trend_rsi_oversold,
                    adx_strong_threshold: config.trend_adx_strong_threshold,
                    r_squared_threshold: config.trend_r_squared_threshold,
                },
            )
            .await
            {
                // トレンドフォローのアクションを実行
                for action in execution_report.actions {
                    match action {
                        TradingAction::AddPosition { token, weight } => {
                            if let Some(&current_price_yocto) = current_prices.get(&token) {
                                let current_price_near =
                                    common::units::Units::yocto_f64_to_near_f64(
                                        current_price_yocto,
                                    );
                                let position_value = available_capital * weight;
                                let position_size = position_value / current_price_near;

                                if position_value
                                    > config
                                        .min_trade_amount
                                        .to_string()
                                        .parse::<f64>()
                                        .unwrap_or(1.0)
                                {
                                    // ホールディングを更新
                                    let current_amount =
                                        current_holdings.get(&token).copied().unwrap_or(0.0);
                                    current_holdings
                                        .insert(token.clone(), current_amount + position_size);

                                    // 取引コストを計算
                                    let trade_cost = position_value * 0.003; // 0.3%手数料
                                    total_costs += trade_cost;
                                    available_capital -= position_value + trade_cost;

                                    // TradeExecutionを記録
                                    trades.push(TradeExecution {
                                        timestamp: current_time,
                                        from_token: config.quote_token.clone(),
                                        to_token: token.clone(),
                                        amount: position_size,
                                        executed_price: current_price_near,
                                        cost: TradingCost {
                                            protocol_fee: BigDecimal::from_f64(trade_cost * 0.7)
                                                .unwrap_or_default(),
                                            slippage: BigDecimal::from_f64(trade_cost * 0.2)
                                                .unwrap_or_default(),
                                            gas_fee: config.gas_cost.clone(),
                                            total: BigDecimal::from_f64(trade_cost)
                                                .unwrap_or_default(),
                                        },
                                        portfolio_value_before: available_capital
                                            + position_value
                                            + trade_cost,
                                        portfolio_value_after: available_capital,
                                        success: true,
                                        reason: format!(
                                            "Trend following: Add position {} ({:.1}%)",
                                            token,
                                            weight * 100.0
                                        ),
                                    });
                                }
                            }
                        }
                        TradingAction::ReducePosition { token, weight } => {
                            if let (Some(&current_amount), Some(&current_price_yocto)) =
                                (current_holdings.get(&token), current_prices.get(&token))
                            {
                                let current_price_near =
                                    common::units::Units::yocto_f64_to_near_f64(
                                        current_price_yocto,
                                    );
                                let reduction_size = current_amount * weight;
                                let reduction_value = reduction_size * current_price_near;

                                if reduction_value > 1.0 {
                                    // 最小取引サイズ
                                    // ホールディングを更新
                                    current_holdings
                                        .insert(token.clone(), current_amount - reduction_size);

                                    // 取引コストを計算
                                    let trade_cost = reduction_value * 0.003; // 0.3%手数料
                                    total_costs += trade_cost;
                                    available_capital += reduction_value - trade_cost;

                                    // TradeExecutionを記録
                                    trades.push(TradeExecution {
                                        timestamp: current_time,
                                        from_token: token.clone(),
                                        to_token: config.quote_token.clone(),
                                        amount: reduction_size,
                                        executed_price: current_price_near,
                                        cost: TradingCost {
                                            protocol_fee: BigDecimal::from_f64(trade_cost * 0.7)
                                                .unwrap_or_default(),
                                            slippage: BigDecimal::from_f64(trade_cost * 0.2)
                                                .unwrap_or_default(),
                                            gas_fee: config.gas_cost.clone(),
                                            total: BigDecimal::from_f64(trade_cost)
                                                .unwrap_or_default(),
                                        },
                                        portfolio_value_before: available_capital - reduction_value
                                            + trade_cost,
                                        portfolio_value_after: available_capital,
                                        success: true,
                                        reason: format!(
                                            "Trend following: Reduce position {} ({:.1}%)",
                                            token,
                                            weight * 100.0
                                        ),
                                    });
                                }
                            }
                        }
                        _ => {} // 他のアクションは無視
                    }
                }
            }
        }

        // ポートフォリオ価値を計算
        let mut total_value = available_capital;
        let mut holdings_value = HashMap::new();

        for (token, amount) in &current_holdings {
            if let Some(&price_yocto) = current_prices.get(token) {
                // priceはyoctoNEAR単位、amountはトークン数量
                // 計算結果をNEAR単位に変換
                let price_near = common::units::Units::yocto_f64_to_near_f64(price_yocto);
                let value = amount * price_near;
                holdings_value.insert(token.clone(), value);
                total_value += value;
            }
        }

        // ポートフォリオ記録
        portfolio_values.push(PortfolioValue {
            timestamp: current_time,
            total_value,
            holdings: holdings_value.into_iter().collect(),
            cash_balance: available_capital,
            unrealized_pnl: total_value - initial_value,
        });

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
        algorithm: AlgorithmType::TrendFollowing,
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
            0.0
        },
    };

    Ok(SimulationResult {
        config: config_summary,
        performance,
        trades,
        portfolio_values,
        execution_summary,
    })
}
