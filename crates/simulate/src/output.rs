use crate::cli::Cli;
use crate::portfolio_state::PortfolioState;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct SimulationResult {
    pub config: SimulationConfig,
    pub performance: PerformanceMetrics,
    pub trades: Vec<TradeEntry>,
    pub portfolio_values: Vec<PortfolioValueEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SimulationConfig {
    pub start_date: String,
    pub end_date: String,
    pub initial_capital: f64,
    pub parameters: SimulationParameters,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SimulationParameters {
    pub top_tokens: usize,
    pub volatility_days: i64,
    pub price_history_days: i64,
    pub rebalance_threshold: f64,
    pub rebalance_interval_days: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub total_return: f64,
    pub annualized_return: f64,
    pub sharpe_ratio: f64,
    pub sortino_ratio: f64,
    pub max_drawdown: f64,
    pub win_rate: f64,
    pub final_balance_near: f64,
    pub total_realized_pnl_near: f64,
    pub trade_count: usize,
    pub liquidation_count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TradeEntry {
    pub timestamp: String,
    pub action: String,
    pub token: String,
    pub amount: u128,
    pub price: f64,
    pub realized_pnl: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PortfolioValueEntry {
    pub timestamp: String,
    pub total_value: f64,
    pub holdings: BTreeMap<String, u128>,
    pub cash_balance: f64,
    pub daily_pnl_near: f64,
    pub daily_pnl_pct: f64,
    pub cumulative_realized_pnl_near: f64,
}

impl SimulationResult {
    pub fn from_state(cli: &Cli, state: &PortfolioState) -> Result<Self> {
        let config = SimulationConfig {
            start_date: cli.start_date.clone(),
            end_date: cli.end_date.clone(),
            initial_capital: cli.initial_capital,
            parameters: SimulationParameters {
                top_tokens: cli.top_tokens,
                volatility_days: cli.volatility_days,
                price_history_days: cli.price_history_days,
                rebalance_threshold: cli.rebalance_threshold,
                rebalance_interval_days: cli.rebalance_interval_days,
            },
        };

        let trades: Vec<TradeEntry> = state
            .trades
            .iter()
            .map(|t| TradeEntry {
                timestamp: t.timestamp.to_rfc3339(),
                action: t.action.clone(),
                token: t.token.clone(),
                amount: t.amount,
                price: t.price_near,
                realized_pnl: t.realized_pnl_near,
            })
            .collect();

        let portfolio_values: Vec<PortfolioValueEntry> = {
            let mut values = Vec::with_capacity(state.snapshots.len());
            let mut prev_value = cli.initial_capital;
            for s in &state.snapshots {
                let daily_pnl_near = s.total_value_near - prev_value;
                let daily_pnl_pct = if prev_value > 0.0 {
                    daily_pnl_near / prev_value
                } else {
                    0.0
                };
                values.push(PortfolioValueEntry {
                    timestamp: s.timestamp.to_rfc3339(),
                    total_value: s.total_value_near,
                    holdings: s.holdings.clone(),
                    cash_balance: s.cash_balance as f64 / 1e24,
                    daily_pnl_near,
                    daily_pnl_pct,
                    cumulative_realized_pnl_near: s.realized_pnl_near,
                });
                prev_value = s.total_value_near;
            }
            values
        };

        let trade_count = state
            .trades
            .iter()
            .filter(|t| t.action != "liquidation")
            .count();
        let liquidation_count = state
            .trades
            .iter()
            .filter(|t| t.action == "liquidation")
            .count();

        let performance = calculate_performance(
            cli.initial_capital,
            &state.snapshots,
            state.realized_pnl,
            trade_count,
            liquidation_count,
        );

        Ok(Self {
            config,
            performance,
            trades,
            portfolio_values,
        })
    }

    pub fn write_to_file(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

fn calculate_performance(
    initial_capital: f64,
    snapshots: &[crate::portfolio_state::PortfolioSnapshot],
    realized_pnl: i128,
    trade_count: usize,
    liquidation_count: usize,
) -> PerformanceMetrics {
    if snapshots.is_empty() {
        return PerformanceMetrics {
            total_return: 0.0,
            annualized_return: 0.0,
            sharpe_ratio: 0.0,
            sortino_ratio: 0.0,
            max_drawdown: 0.0,
            win_rate: 0.0,
            final_balance_near: initial_capital,
            total_realized_pnl_near: realized_pnl as f64 / 1e24,
            trade_count,
            liquidation_count,
        };
    }

    let final_value = snapshots.last().unwrap().total_value_near;
    let total_return = if initial_capital > 0.0 {
        (final_value - initial_capital) / initial_capital
    } else {
        0.0
    };

    // Calculate daily returns
    let mut daily_returns: Vec<f64> = Vec::new();
    let mut prev_value = initial_capital;
    for snapshot in snapshots {
        if prev_value > 0.0 {
            let daily_ret = (snapshot.total_value_near - prev_value) / prev_value;
            daily_returns.push(daily_ret);
        }
        prev_value = snapshot.total_value_near;
    }

    // Annualized return
    let num_days = snapshots.len() as f64;
    let annualized_return = if num_days > 0.0 && final_value > 0.0 && initial_capital > 0.0 {
        (final_value / initial_capital).powf(365.0 / num_days) - 1.0
    } else {
        0.0
    };

    // Sharpe ratio (assuming risk-free rate = 0)
    let sharpe_ratio = calculate_sharpe_ratio(&daily_returns);

    // Sortino ratio
    let sortino_ratio = calculate_sortino_ratio(&daily_returns);

    // Max drawdown
    let max_drawdown = calculate_max_drawdown(snapshots);

    // Win rate
    let winning_days = daily_returns.iter().filter(|&&r| r > 0.0).count();
    let win_rate = if daily_returns.is_empty() {
        0.0
    } else {
        winning_days as f64 / daily_returns.len() as f64
    };

    PerformanceMetrics {
        total_return,
        annualized_return,
        sharpe_ratio,
        sortino_ratio,
        max_drawdown,
        win_rate,
        final_balance_near: final_value,
        total_realized_pnl_near: realized_pnl as f64 / 1e24,
        trade_count,
        liquidation_count,
    }
}

fn calculate_sharpe_ratio(returns: &[f64]) -> f64 {
    if returns.len() < 2 {
        return 0.0;
    }

    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance: f64 =
        returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (returns.len() - 1) as f64;
    let std_dev = variance.sqrt();

    if std_dev == 0.0 {
        0.0
    } else {
        // Annualize: multiply by sqrt(365)
        (mean / std_dev) * 365.0_f64.sqrt()
    }
}

fn calculate_sortino_ratio(returns: &[f64]) -> f64 {
    if returns.len() < 2 {
        return 0.0;
    }

    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let downside_variance: f64 = returns
        .iter()
        .filter(|&&r| r < 0.0)
        .map(|r| r.powi(2))
        .sum::<f64>()
        / returns.len() as f64;
    let downside_dev = downside_variance.sqrt();

    if downside_dev == 0.0 {
        0.0
    } else {
        (mean / downside_dev) * 365.0_f64.sqrt()
    }
}

fn calculate_max_drawdown(snapshots: &[crate::portfolio_state::PortfolioSnapshot]) -> f64 {
    if snapshots.is_empty() {
        return 0.0;
    }

    let mut peak = snapshots[0].total_value_near;
    let mut max_dd = 0.0;

    for snapshot in snapshots {
        if snapshot.total_value_near > peak {
            peak = snapshot.total_value_near;
        }
        if peak > 0.0 {
            let drawdown = (peak - snapshot.total_value_near) / peak;
            if drawdown > max_dd {
                max_dd = drawdown;
            }
        }
    }

    max_dd
}

#[cfg(test)]
mod tests;
