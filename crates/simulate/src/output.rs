use crate::cli::RunArgs;
use crate::portfolio_state::{
    PortfolioState, SwapEvent, SwapMethod, TradeAction, pnl_to_near, to_f64_or_warn,
    to_u128_or_warn,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct SimulationResult {
    pub config: SimulationConfig,
    pub performance: PerformanceMetrics,
    pub trades: Vec<TradeEntry>,
    pub swap_events: Vec<SwapEventEntry>,
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
    pub price_history_days: i64,
    pub rebalance_threshold: f64,
    pub rebalance_interval_days: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub total_return: f64,
    pub sharpe_ratio: f64,
    pub sortino_ratio: f64,
    pub max_drawdown: f64,
    pub win_rate: f64,
    pub final_balance_near: f64,
    pub total_realized_pnl_near: f64,
    pub trade_count: usize,
    pub liquidation_count: usize,
    #[serde(flatten)]
    pub swap_stats: SwapStats,
}

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SwapStats {
    pub total_swaps: usize,
    pub pool_based_swaps: usize,
    pub fallback_swaps: usize,
    pub fallback_rate: f64,
}

impl SwapStats {
    pub fn from_events(events: &[SwapEvent]) -> Self {
        let total_swaps = events.len();
        let pool_based_swaps = events
            .iter()
            .filter(|e| e.swap_method == SwapMethod::PoolBased)
            .count();
        let fallback_swaps = events
            .iter()
            .filter(|e| e.swap_method == SwapMethod::DbRate)
            .count();
        let fallback_rate = if total_swaps > 0 {
            fallback_swaps as f64 / total_swaps as f64
        } else {
            0.0
        };
        Self {
            total_swaps,
            pool_based_swaps,
            fallback_swaps,
            fallback_rate,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TradeEntry {
    pub timestamp: String,
    pub action: TradeAction,
    pub token: String,
    pub amount: u128,
    pub price: f64,
    pub realized_pnl: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SwapEventEntry {
    pub timestamp: String,
    pub token_in: String,
    pub amount_in: String,
    pub amount_in_raw: u128,
    pub token_out: String,
    pub amount_out: String,
    pub amount_out_raw: u128,
    pub swap_method: SwapMethod,
    pub pool_ids: Vec<u32>,
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
    pub fn from_state(cli: &RunArgs, state: &PortfolioState) -> Result<Self> {
        let config = SimulationConfig {
            start_date: cli.start_date.clone(),
            end_date: cli.end_date.clone(),
            initial_capital: cli.initial_capital,
            parameters: SimulationParameters {
                top_tokens: cli.top_tokens,
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
                token: t.token.to_string(),
                amount: to_u128_or_warn(t.amount.smallest_units(), "trade_amount"),
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
                // Convert holdings to String -> u128 for JSON output
                let holdings_map: BTreeMap<String, u128> = s
                    .holdings
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.to_string(),
                            to_u128_or_warn(v.smallest_units(), "snapshot_holdings"),
                        )
                    })
                    .collect();
                values.push(PortfolioValueEntry {
                    timestamp: s.timestamp.to_rfc3339(),
                    total_value: s.total_value_near,
                    holdings: holdings_map,
                    cash_balance: to_f64_or_warn(
                        s.cash_balance.as_bigdecimal(),
                        "snapshot_cash_balance",
                    ) / 1e24,
                    daily_pnl_near,
                    daily_pnl_pct,
                    cumulative_realized_pnl_near: s.realized_pnl_near,
                });
                prev_value = s.total_value_near;
            }
            values
        };

        let swap_events: Vec<SwapEventEntry> = state
            .swap_events
            .iter()
            .map(|e| SwapEventEntry {
                timestamp: e.timestamp.to_rfc3339(),
                token_in: e.token_in.to_string(),
                amount_in: e.amount_in.to_string(),
                amount_in_raw: to_u128_or_warn(
                    e.amount_in.smallest_units(),
                    "swap_event_amount_in",
                ),
                token_out: e.token_out.to_string(),
                amount_out: e.amount_out.to_string(),
                amount_out_raw: to_u128_or_warn(
                    e.amount_out.smallest_units(),
                    "swap_event_amount_out",
                ),
                swap_method: e.swap_method,
                pool_ids: e.pool_ids.clone(),
            })
            .collect();

        let swap_stats = SwapStats::from_events(&state.swap_events);

        let trade_count = state
            .trades
            .iter()
            .filter(|t| t.action != TradeAction::Liquidation)
            .count();
        let liquidation_count = state
            .trades
            .iter()
            .filter(|t| t.action == TradeAction::Liquidation)
            .count();

        let performance = calculate_performance(PerformanceInput {
            initial_capital: cli.initial_capital,
            snapshots: &state.snapshots,
            realized_pnl: state.realized_pnl,
            trade_count,
            liquidation_count,
            rebalance_interval_days: cli.rebalance_interval_days,
            swap_stats,
        });

        Ok(Self {
            config,
            performance,
            trades,
            swap_events,
            portfolio_values,
        })
    }

    pub fn write_to_file(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

struct PerformanceInput<'a> {
    initial_capital: f64,
    snapshots: &'a [crate::portfolio_state::PortfolioSnapshot],
    realized_pnl: i128,
    trade_count: usize,
    liquidation_count: usize,
    rebalance_interval_days: i64,
    swap_stats: SwapStats,
}

fn calculate_performance(input: PerformanceInput<'_>) -> PerformanceMetrics {
    let PerformanceInput {
        initial_capital,
        snapshots,
        realized_pnl,
        trade_count,
        liquidation_count,
        rebalance_interval_days,
        swap_stats,
    } = input;

    let Some(last) = snapshots.last() else {
        return PerformanceMetrics {
            total_return: 0.0,
            sharpe_ratio: 0.0,
            sortino_ratio: 0.0,
            max_drawdown: 0.0,
            win_rate: 0.0,
            final_balance_near: initial_capital,
            total_realized_pnl_near: pnl_to_near(realized_pnl),
            trade_count,
            liquidation_count,
            swap_stats,
        };
    };
    let final_value = last.total_value_near;
    let total_return = if initial_capital > 0.0 {
        (final_value - initial_capital) / initial_capital
    } else {
        0.0
    };

    // Calculate per-period returns
    let mut period_returns: Vec<f64> = Vec::new();
    let mut prev_value = initial_capital;
    for snapshot in snapshots {
        if prev_value > 0.0 {
            let ret = (snapshot.total_value_near - prev_value) / prev_value;
            period_returns.push(ret);
        }
        prev_value = snapshot.total_value_near;
    }

    // Sharpe ratio (assuming risk-free rate = 0)
    let sharpe_ratio = calculate_sharpe_ratio(&period_returns, rebalance_interval_days);

    // Sortino ratio
    let sortino_ratio = calculate_sortino_ratio(&period_returns, rebalance_interval_days);

    // Max drawdown
    let max_drawdown = calculate_max_drawdown(snapshots);

    // Win rate
    let winning_periods = period_returns.iter().filter(|&&r| r > 0.0).count();
    let win_rate = if period_returns.is_empty() {
        0.0
    } else {
        winning_periods as f64 / period_returns.len() as f64
    };

    PerformanceMetrics {
        total_return,
        sharpe_ratio,
        sortino_ratio,
        max_drawdown,
        win_rate,
        final_balance_near: final_value,
        total_realized_pnl_near: pnl_to_near(realized_pnl),
        trade_count,
        liquidation_count,
        swap_stats,
    }
}

fn calculate_sharpe_ratio(returns: &[f64], interval_days: i64) -> f64 {
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
        // Annualize: multiply by sqrt(periods_per_year)
        let periods_per_year = 365.0 / interval_days as f64;
        (mean / std_dev) * periods_per_year.sqrt()
    }
}

fn calculate_sortino_ratio(returns: &[f64], interval_days: i64) -> f64 {
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
        let periods_per_year = 365.0 / interval_days as f64;
        (mean / downside_dev) * periods_per_year.sqrt()
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
