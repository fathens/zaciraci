use super::data::{fetch_price_data, get_prices_at_time};
use super::types::*;
use crate::api::backend::BackendClient;
use anyhow::Result;
use std::collections::HashMap;

/// Run momentum simulation
pub async fn run_momentum_simulation(config: &SimulationConfig) -> Result<SimulationResult> {
    println!(
        "📈 Running momentum simulation for tokens: {:?}",
        config.target_tokens
    );

    let backend_client = BackendClient::new();

    // 1. 価格データを取得
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
    println!("📊 Running portfolio optimization simulation");
    println!(
        "🔧 Optimizing portfolio for tokens: {:?}",
        config.target_tokens
    );

    let backend_client = BackendClient::new();

    // 1. 価格データを取得
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
    println!("📉 Running trend following simulation");
    println!("📊 Following trends for tokens: {:?}", config.target_tokens);

    let backend_client = BackendClient::new();

    // 1. 価格データを取得
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
    let trades = Vec::new();
    let mut current_holdings = HashMap::new();

    // 初期ポートフォリオ設定（均等分散）
    let tokens_count = config.target_tokens.len() as f64;
    let initial_per_token = initial_value / tokens_count;

    // 初期価格データを取得
    let initial_prices = get_prices_at_time(price_data, config.start_date)?;

    for token in &config.target_tokens {
        if let Some(&initial_price) = initial_prices.get(token) {
            let token_amount = initial_per_token / initial_price;
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

        // ポートフォリオ価値を計算
        let mut total_value = 0.0;
        let mut holdings_value = HashMap::new();

        for (token, amount) in &current_holdings {
            if let Some(&price) = current_prices.get(token) {
                let value = amount * price;
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
    let total_costs = 0.0; // 現在は取引がないため
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
        total_cost: trades
            .iter()
            .map(|t| t.cost.total.to_string().parse::<f64>().unwrap_or(0.0))
            .sum(),
        avg_cost_per_trade: if !trades.is_empty() {
            trades
                .iter()
                .map(|t| t.cost.total.to_string().parse::<f64>().unwrap_or(0.0))
                .sum::<f64>()
                / trades.len() as f64
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
    let trades = Vec::new();
    let mut current_holdings = HashMap::new();

    // 初期ポートフォリオ設定（均等分散）
    let tokens_count = config.target_tokens.len() as f64;
    let initial_per_token = initial_value / tokens_count;

    // 初期価格データを取得
    let initial_prices = get_prices_at_time(price_data, config.start_date)?;

    for token in &config.target_tokens {
        if let Some(&initial_price) = initial_prices.get(token) {
            let token_amount = initial_per_token / initial_price;
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

        // ポートフォリオ価値を計算
        let mut total_value = 0.0;
        let mut holdings_value = HashMap::new();

        for (token, amount) in &current_holdings {
            if let Some(&price) = current_prices.get(token) {
                let value = amount * price;
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
    let total_costs = 0.0; // 現在は取引がないため
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
        total_cost: trades
            .iter()
            .map(|t| t.cost.total.to_string().parse::<f64>().unwrap_or(0.0))
            .sum(),
        avg_cost_per_trade: if !trades.is_empty() {
            trades
                .iter()
                .map(|t| t.cost.total.to_string().parse::<f64>().unwrap_or(0.0))
                .sum::<f64>()
                / trades.len() as f64
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
    let trades = Vec::new();
    let mut current_holdings = HashMap::new();

    // 初期ポートフォリオ設定（均等分散）
    let tokens_count = config.target_tokens.len() as f64;
    let initial_per_token = initial_value / tokens_count;

    // 初期価格データを取得
    let initial_prices = get_prices_at_time(price_data, config.start_date)?;

    for token in &config.target_tokens {
        if let Some(&initial_price) = initial_prices.get(token) {
            let token_amount = initial_per_token / initial_price;
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

        // ポートフォリオ価値を計算
        let mut total_value = 0.0;
        let mut holdings_value = HashMap::new();

        for (token, amount) in &current_holdings {
            if let Some(&price) = current_prices.get(token) {
                let value = amount * price;
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
    let total_costs = 0.0; // 現在は取引がないため
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
        total_cost: trades
            .iter()
            .map(|t| t.cost.total.to_string().parse::<f64>().unwrap_or(0.0))
            .sum(),
        avg_cost_per_trade: if !trades.is_empty() {
            trades
                .iter()
                .map(|t| t.cost.total.to_string().parse::<f64>().unwrap_or(0.0))
                .sum::<f64>()
                / trades.len() as f64
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
