use super::types::*;
use anyhow::Result;
use chrono::{DateTime, Utc};
use common::algorithm::indicators::calculate_max_drawdown;

/// Calculate comprehensive performance metrics
pub fn calculate_performance_metrics(
    initial_value: f64,
    final_value: f64,
    portfolio_values: &[PortfolioValue],
    trades: &[TradeExecution],
    total_costs: f64,
    start_date: DateTime<Utc>,
    end_date: DateTime<Utc>,
) -> Result<PerformanceMetrics> {
    let duration = end_date - start_date;
    let duration_days = duration.num_days();

    let total_return = final_value - initial_value;
    let total_return_pct = if initial_value > 0.0 {
        (total_return / initial_value) * 100.0
    } else {
        0.0
    };

    // Calculate annualized return
    let years = duration_days as f64 / 365.25;
    let annualized_return = if years > 0.0 && initial_value > 0.0 {
        ((final_value / initial_value).powf(1.0 / years) - 1.0) * 100.0
    } else {
        0.0
    };

    // Calculate volatility from portfolio values
    let returns: Vec<f64> = portfolio_values
        .windows(2)
        .map(|window| {
            let prev = window[0].total_value.as_f64();
            let curr = window[1].total_value.as_f64();
            if prev > 0.0 {
                (curr - prev) / prev
            } else {
                0.0
            }
        })
        .collect();

    let volatility = calculate_volatility(&returns) * 100.0; // Convert to percentage

    // Calculate maximum drawdown
    let mut peak = initial_value;
    let mut max_drawdown = 0.0;
    let mut max_drawdown_value = 0.0;

    for portfolio_value in portfolio_values {
        let value = portfolio_value.total_value.as_f64();
        if value > peak {
            peak = value;
        }
        let drawdown = peak - value;
        let drawdown_pct = if peak > 0.0 { drawdown / peak } else { 0.0 };

        if drawdown > max_drawdown_value {
            max_drawdown_value = drawdown;
            max_drawdown = drawdown_pct;
        }
    }

    let max_drawdown_pct = max_drawdown * 100.0;

    // Risk-adjusted returns
    let risk_free_rate = 0.0; // Assuming 0% risk-free rate
    let excess_return = annualized_return - risk_free_rate;

    // Calculate Sharpe ratio with proper handling of edge cases
    // When volatility is zero or near-zero, the Sharpe ratio becomes undefined or infinite
    // We cap it to a large but reasonable value to maintain mathematical integrity
    const MAX_SHARPE_RATIO: f64 = 999.99; // Display cap for extreme values

    let sharpe_ratio = if volatility == 0.0 {
        // Perfect consistency (no volatility)
        // Sharpe ratio is mathematically undefined, but we need a practical representation
        if excess_return > 0.0 {
            MAX_SHARPE_RATIO // Positive return with no risk
        } else if excess_return < 0.0 {
            -MAX_SHARPE_RATIO // Negative return with no risk
        } else {
            0.0 // No return and no risk
        }
    } else {
        // Normal calculation with cap for display purposes
        let calculated_sharpe = excess_return / volatility;
        if calculated_sharpe.is_finite() {
            // Cap only for display/practical purposes, not for mathematical invalidity
            calculated_sharpe.clamp(-MAX_SHARPE_RATIO, MAX_SHARPE_RATIO)
        } else {
            0.0 // Handle NaN or infinity cases
        }
    };

    // Sortino ratio calculation (downside deviation)
    let negative_returns: Vec<f64> = returns.iter().filter(|&&r| r < 0.0).cloned().collect();
    let downside_deviation = if !negative_returns.is_empty() {
        let mean_negative = negative_returns.iter().sum::<f64>() / negative_returns.len() as f64;
        (negative_returns
            .iter()
            .map(|r| (r - mean_negative).powi(2))
            .sum::<f64>()
            / negative_returns.len() as f64)
            .sqrt()
            * 100.0
    } else {
        0.0
    };

    // Sortino ratio with same mathematical handling as Sharpe ratio
    let sortino_ratio = if downside_deviation == 0.0 {
        // No downside volatility - either no negative returns or perfect consistency
        // Use Sharpe ratio as a reasonable proxy
        sharpe_ratio
    } else {
        let calculated_sortino = excess_return / downside_deviation;
        if calculated_sortino.is_finite() {
            // Apply same display cap as Sharpe ratio
            calculated_sortino.clamp(-MAX_SHARPE_RATIO, MAX_SHARPE_RATIO)
        } else {
            sharpe_ratio // Fallback to Sharpe ratio for undefined cases
        }
    };

    // Trade analysis
    let trade_metrics = analyze_trades(trades);

    let cost_ratio = if initial_value > 0.0 {
        (total_costs / initial_value) * 100.0
    } else {
        0.0
    };

    // Calculate active trading days (days with trades)
    let active_trading_days = count_active_trading_days(trades);

    Ok(PerformanceMetrics {
        total_return,
        annualized_return,
        total_return_pct,
        volatility,
        max_drawdown: max_drawdown_value,
        max_drawdown_pct,
        sharpe_ratio,
        sortino_ratio,
        total_trades: trades.len(),
        winning_trades: trade_metrics.winning_trades,
        losing_trades: trade_metrics.losing_trades,
        win_rate: trade_metrics.win_rate,
        profit_factor: trade_metrics.profit_factor,
        total_costs,
        cost_ratio,
        simulation_days: duration_days,
        active_trading_days,
    })
}

/// Legacy performance metrics calculation (for backward compatibility)
pub fn calculate_performance_metrics_legacy(
    initial_value: f64,
    final_value: f64,
    portfolio_values: &[PortfolioValue],
    trades: &[TradeExecution],
    duration_days: i64,
) -> PerformanceMetrics {
    let total_return = (final_value - initial_value) / initial_value;
    let annualized_return = if duration_days > 0 {
        total_return * 365.0 / duration_days as f64
    } else {
        0.0
    };

    // ボラティリティ計算
    let returns: Vec<f64> = portfolio_values
        .windows(2)
        .map(|w| {
            (w[1].total_value.as_f64() - w[0].total_value.as_f64()) / w[0].total_value.as_f64()
        })
        .collect();

    let volatility = if returns.len() > 1 {
        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance =
            returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
        variance.sqrt() * (252.0_f64).sqrt() // 年率換算
    } else {
        0.0
    };

    // ドローダウン計算
    let portfolio_values_f64: Vec<f64> = portfolio_values
        .iter()
        .map(|pv| pv.total_value.as_f64())
        .collect();
    let max_drawdown = calculate_max_drawdown(&portfolio_values_f64);

    // 取引分析
    let mut total_profit = 0.0;
    let mut total_loss = 0.0;
    let mut winning_trades_count = 0;

    for trade in trades {
        let profit_loss =
            trade.portfolio_value_after.as_f64() - trade.portfolio_value_before.as_f64();
        if profit_loss > 0.0 {
            total_profit += profit_loss;
            winning_trades_count += 1;
        } else if profit_loss < 0.0 {
            total_loss += -profit_loss; // 損失は正の値として計算
        }
    }

    let losing_trades = trades.len() - winning_trades_count;
    let win_rate = if trades.is_empty() {
        0.0
    } else {
        winning_trades_count as f64 / trades.len() as f64
    };

    // プロフィットファクター = 総利益 / 総損失
    let profit_factor = if total_loss > 0.0 {
        total_profit / total_loss
    } else if total_profit > 0.0 {
        // 損失がない場合は無限大を表す大きな値
        f64::MAX
    } else {
        // 利益も損失もない場合
        0.0
    };

    let total_costs = trades
        .iter()
        .map(|t| t.cost.total.to_string().parse::<f64>().unwrap_or(0.0))
        .sum::<f64>();

    let cost_ratio = if final_value > 0.0 {
        total_costs / final_value * 100.0
    } else {
        0.0
    };

    // シャープレシオ（表示用上限付き）
    const MAX_SHARPE_RATIO: f64 = 999.99; // 極端な値の表示用上限

    let sharpe_ratio = if volatility == 0.0 {
        // ボラティリティゼロ（完全に一定）の場合
        // 数学的には未定義だが、実用的な表現が必要
        if annualized_return > 0.0 {
            MAX_SHARPE_RATIO // リスクなしでプラスリターン
        } else if annualized_return < 0.0 {
            -MAX_SHARPE_RATIO // リスクなしでマイナスリターン
        } else {
            0.0 // リターンもリスクもなし
        }
    } else {
        let calculated_sharpe = annualized_return / volatility;
        if calculated_sharpe.is_finite() {
            // 表示用の上限を適用（数学的無効性ではなく実用上の理由）
            calculated_sharpe.clamp(-MAX_SHARPE_RATIO, MAX_SHARPE_RATIO)
        } else {
            0.0 // NaNや無限大の処理
        }
    };

    PerformanceMetrics {
        total_return,
        annualized_return,
        total_return_pct: total_return * 100.0,
        volatility,
        max_drawdown,
        max_drawdown_pct: max_drawdown * 100.0,
        sharpe_ratio,
        sortino_ratio: sharpe_ratio, // 暫定的にシャープレシオと同じ
        total_trades: trades.len(),
        winning_trades: winning_trades_count,
        losing_trades,
        win_rate,
        profit_factor,
        total_costs,
        cost_ratio,
        simulation_days: duration_days,
        active_trading_days: if trades.is_empty() { 0 } else { duration_days },
    }
}

/// Analyze individual trades
#[derive(Debug)]
pub struct TradeAnalysis {
    pub winning_trades: usize,
    pub losing_trades: usize,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub avg_win: f64,
    pub avg_loss: f64,
    pub largest_win: f64,
    pub largest_loss: f64,
}

pub fn analyze_trades(trades: &[TradeExecution]) -> TradeAnalysis {
    if trades.is_empty() {
        return TradeAnalysis {
            winning_trades: 0,
            losing_trades: 0,
            win_rate: 0.0,
            profit_factor: 0.0,
            avg_win: 0.0,
            avg_loss: 0.0,
            largest_win: 0.0,
            largest_loss: 0.0,
        };
    }

    let mut total_profit = 0.0;
    let mut total_loss = 0.0;
    let mut winning_trades = 0;
    let mut losing_trades = 0;
    let mut wins = Vec::new();
    let mut losses = Vec::new();

    for trade in trades {
        let pnl = trade.portfolio_value_after.as_f64() - trade.portfolio_value_before.as_f64();

        if pnl > 0.0 {
            total_profit += pnl;
            winning_trades += 1;
            wins.push(pnl);
        } else if pnl < 0.0 {
            total_loss += -pnl; // Store as positive value
            losing_trades += 1;
            losses.push(-pnl);
        }
    }

    let win_rate = (winning_trades as f64 / trades.len() as f64) * 100.0;

    let profit_factor = if total_loss > 0.0 {
        total_profit / total_loss
    } else if total_profit > 0.0 {
        f64::INFINITY
    } else {
        0.0
    };

    let avg_win = if !wins.is_empty() {
        wins.iter().sum::<f64>() / wins.len() as f64
    } else {
        0.0
    };

    let avg_loss = if !losses.is_empty() {
        losses.iter().sum::<f64>() / losses.len() as f64
    } else {
        0.0
    };

    let largest_win = wins.iter().fold(0.0f64, |a, &b| a.max(b));
    let largest_loss = losses.iter().fold(0.0f64, |a, &b| a.max(b));

    TradeAnalysis {
        winning_trades,
        losing_trades,
        win_rate,
        profit_factor,
        avg_win,
        avg_loss,
        largest_win,
        largest_loss,
    }
}

/// Calculate volatility from returns
pub fn calculate_volatility(returns: &[f64]) -> f64 {
    if returns.len() <= 1 {
        return 0.0;
    }

    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance =
        returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (returns.len() - 1) as f64;

    variance.sqrt()
}

/// Calculate rolling statistics for portfolio values
pub fn calculate_rolling_statistics(
    portfolio_values: &[PortfolioValue],
    window_size: usize,
) -> Vec<RollingStats> {
    let mut rolling_stats = Vec::new();

    if portfolio_values.len() < window_size {
        return rolling_stats;
    }

    for window in portfolio_values.windows(window_size) {
        let values: Vec<f64> = window.iter().map(|pv| pv.total_value.as_f64()).collect();
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let volatility = calculate_volatility(&values);

        let returns: Vec<f64> = values.windows(2).map(|w| (w[1] - w[0]) / w[0]).collect();

        let return_volatility = calculate_volatility(&returns);

        rolling_stats.push(RollingStats {
            timestamp: window.last().unwrap().timestamp,
            mean_value: mean,
            volatility,
            return_volatility,
            min_value: values.iter().fold(f64::INFINITY, |a, &b| a.min(b)),
            max_value: values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b)),
        });
    }

    rolling_stats
}

#[derive(Debug, Clone)]
pub struct RollingStats {
    pub timestamp: DateTime<Utc>,
    pub mean_value: f64,
    pub volatility: f64,
    pub return_volatility: f64,
    pub min_value: f64,
    pub max_value: f64,
}

/// Calculate portfolio correlation matrix
pub fn calculate_correlation_matrix(
    portfolio_values: &[PortfolioValue],
) -> HashMap<String, HashMap<String, f64>> {
    let mut correlations = HashMap::new();

    // Extract time series for each token
    let mut token_series: HashMap<String, Vec<f64>> = HashMap::new();

    for pv in portfolio_values {
        for (token, value) in &pv.holdings {
            token_series
                .entry(token.clone())
                .or_default()
                .push(value.as_f64());
        }
    }

    // Calculate correlation between each pair of tokens
    let tokens: Vec<String> = token_series.keys().cloned().collect();

    for token1 in &tokens {
        let mut token1_correlations = HashMap::new();

        for token2 in &tokens {
            let correlation = if token1 == token2 {
                1.0
            } else {
                calculate_correlation(
                    token_series.get(token1).unwrap(),
                    token_series.get(token2).unwrap(),
                )
            };

            token1_correlations.insert(token2.clone(), correlation);
        }

        correlations.insert(token1.clone(), token1_correlations);
    }

    correlations
}

/// Calculate Pearson correlation coefficient between two series
fn calculate_correlation(series1: &[f64], series2: &[f64]) -> f64 {
    if series1.len() != series2.len() || series1.len() < 2 {
        return 0.0;
    }

    let n = series1.len() as f64;
    let mean1 = series1.iter().sum::<f64>() / n;
    let mean2 = series2.iter().sum::<f64>() / n;

    let mut numerator = 0.0;
    let mut sum_sq1 = 0.0;
    let mut sum_sq2 = 0.0;

    for (i, &v1) in series1.iter().enumerate() {
        let v2 = series2[i];
        let diff1 = v1 - mean1;
        let diff2 = v2 - mean2;

        numerator += diff1 * diff2;
        sum_sq1 += diff1 * diff1;
        sum_sq2 += diff2 * diff2;
    }

    let denominator = (sum_sq1 * sum_sq2).sqrt();

    if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

/// Count active trading days
fn count_active_trading_days(trades: &[TradeExecution]) -> i64 {
    if trades.is_empty() {
        return 0;
    }

    let mut trading_days = std::collections::HashSet::new();

    for trade in trades {
        let date = trade.timestamp.date_naive();
        trading_days.insert(date);
    }

    trading_days.len() as i64
}

/// Calculate Information Ratio
pub fn calculate_information_ratio(portfolio_returns: &[f64], benchmark_returns: &[f64]) -> f64 {
    if portfolio_returns.len() != benchmark_returns.len() || portfolio_returns.is_empty() {
        return 0.0;
    }

    let excess_returns: Vec<f64> = portfolio_returns
        .iter()
        .zip(benchmark_returns.iter())
        .map(|(p, b)| p - b)
        .collect();

    let mean_excess_return = excess_returns.iter().sum::<f64>() / excess_returns.len() as f64;
    let tracking_error = calculate_volatility(&excess_returns);

    if tracking_error > 0.0 {
        mean_excess_return / tracking_error
    } else {
        0.0
    }
}

/// Calculate Maximum Drawdown Duration
pub fn calculate_max_drawdown_duration(portfolio_values: &[PortfolioValue]) -> i64 {
    if portfolio_values.len() < 2 {
        return 0;
    }

    let mut peak_value = portfolio_values[0].total_value.as_f64();
    let mut peak_time = portfolio_values[0].timestamp;
    let mut max_duration = 0i64;
    let mut current_drawdown_start: Option<DateTime<Utc>> = None;

    for pv in portfolio_values.iter().skip(1) {
        let current_value = pv.total_value.as_f64();
        if current_value > peak_value {
            // New peak reached
            peak_value = current_value;
            peak_time = pv.timestamp;

            // End of drawdown period
            if let Some(start) = current_drawdown_start {
                let duration = (pv.timestamp - start).num_days();
                max_duration = max_duration.max(duration);
                current_drawdown_start = None;
            }
        } else if current_value < peak_value && current_drawdown_start.is_none() {
            // Start of drawdown period
            current_drawdown_start = Some(peak_time);
        }
    }

    // Handle case where drawdown extends to the end
    if let Some(start) = current_drawdown_start {
        let duration = (portfolio_values.last().unwrap().timestamp - start).num_days();
        max_duration = max_duration.max(duration);
    }

    max_duration
}

use std::collections::HashMap;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const MAX_SHARPE_RATIO: f64 = 999.99;

    /// Helper function to create a PortfolioValue for testing
    fn pv(timestamp: DateTime<Utc>, total_value: f64) -> PortfolioValue {
        PortfolioValue {
            timestamp,
            total_value: NearValueF64::from_near(total_value),
            holdings: HashMap::new(),
            cash_balance: NearValueF64::from_near(total_value),
            unrealized_pnl: NearValueF64::zero(),
        }
    }

    #[test]
    fn test_sharpe_ratio_normal_case() {
        // Normal volatility case
        let portfolio_values = vec![
            pv(Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(), 1000.0),
            pv(Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap(), 1010.0),
            pv(Utc.with_ymd_and_hms(2024, 1, 3, 0, 0, 0).unwrap(), 1005.0),
            pv(Utc.with_ymd_and_hms(2024, 1, 4, 0, 0, 0).unwrap(), 1020.0),
        ];

        let result = calculate_performance_metrics(
            1000.0,
            1020.0,
            &portfolio_values,
            &[],
            0.0,
            Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2024, 1, 4, 0, 0, 0).unwrap(),
        )
        .unwrap();

        // Should have a reasonable Sharpe ratio
        assert!(result.sharpe_ratio > 0.0);
        assert!(result.sharpe_ratio <= MAX_SHARPE_RATIO); // Should be within bounds

        // For normal volatility, should not hit the cap
        println!("Normal case Sharpe ratio: {}", result.sharpe_ratio);
    }

    #[test]
    fn test_sharpe_ratio_zero_volatility() {
        // Perfect consistency - no volatility
        let portfolio_values = vec![
            pv(Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(), 1000.0),
            pv(Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap(), 1000.0),
            pv(Utc.with_ymd_and_hms(2024, 1, 3, 0, 0, 0).unwrap(), 1000.0),
        ];

        let result = calculate_performance_metrics(
            1000.0,
            1000.0,
            &portfolio_values,
            &[],
            0.0,
            Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2024, 1, 3, 0, 0, 0).unwrap(),
        )
        .unwrap();

        // Zero return and zero volatility should give 0
        assert_eq!(result.sharpe_ratio, 0.0);
    }

    #[test]
    fn test_sharpe_ratio_extremely_low_volatility() {
        // Nearly constant values - extremely low volatility
        let portfolio_values = vec![
            pv(Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(), 1000.0),
            pv(
                Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap(),
                1000.00001,
            ),
            pv(
                Utc.with_ymd_and_hms(2024, 1, 3, 0, 0, 0).unwrap(),
                1000.00002,
            ),
            pv(
                Utc.with_ymd_and_hms(2024, 1, 4, 0, 0, 0).unwrap(),
                1000.00003,
            ),
        ];

        let result = calculate_performance_metrics(
            1000.0,
            1000.00003,
            &portfolio_values,
            &[],
            0.0,
            Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2024, 1, 4, 0, 0, 0).unwrap(),
        )
        .unwrap();

        // Should be capped at MAX_SHARPE_RATIO
        assert_eq!(result.sharpe_ratio, MAX_SHARPE_RATIO);
    }

    #[test]
    fn test_sharpe_ratio_positive_return_zero_volatility() {
        // Constant positive growth with perfect consistency
        let portfolio_values = vec![
            pv(Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(), 1000.0),
            pv(Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap(), 1100.0),
        ];

        // Create constant returns by adding the same value
        let mut values = portfolio_values.clone();
        for i in 2..10 {
            values.push(pv(
                Utc.with_ymd_and_hms(2024, 1, 1 + i, 0, 0, 0).unwrap(),
                1100.0, // Keep constant after initial jump
            ));
        }

        let result = calculate_performance_metrics(
            1000.0,
            1100.0,
            &values,
            &[],
            0.0,
            Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2024, 1, 10, 0, 0, 0).unwrap(),
        )
        .unwrap();

        // Positive return with very low volatility should be capped
        assert!(result.sharpe_ratio > 0.0);
        assert!(result.sharpe_ratio <= MAX_SHARPE_RATIO);
    }

    #[test]
    fn test_sharpe_ratio_negative_return_zero_volatility() {
        // Constant negative return with zero volatility
        let portfolio_values = vec![
            pv(Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(), 1000.0),
            pv(Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap(), 900.0),
        ];

        // Keep constant after initial drop
        let mut values = portfolio_values.clone();
        for i in 2..10 {
            values.push(pv(
                Utc.with_ymd_and_hms(2024, 1, 1 + i, 0, 0, 0).unwrap(),
                900.0,
            ));
        }

        let result = calculate_performance_metrics(
            1000.0,
            900.0,
            &values,
            &[],
            0.0,
            Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2024, 1, 10, 0, 0, 0).unwrap(),
        )
        .unwrap();

        // Negative return with very low volatility should be capped at negative max
        assert!(result.sharpe_ratio < 0.0);
        assert!(result.sharpe_ratio >= -MAX_SHARPE_RATIO);
    }

    #[test]
    fn test_sharpe_ratio_handles_extreme_values() {
        // Test that extreme calculated values are properly capped
        let portfolio_values = vec![
            pv(Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(), 1000.0),
            pv(Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap(), 2000.0), // 100% gain in one day
            pv(
                Utc.with_ymd_and_hms(2024, 1, 3, 0, 0, 0).unwrap(),
                2000.0000001, // Then nearly constant
            ),
        ];

        let result = calculate_performance_metrics(
            1000.0,
            2000.0000001,
            &portfolio_values,
            &[],
            0.0,
            Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2024, 1, 3, 0, 0, 0).unwrap(),
        )
        .unwrap();

        // Should be capped regardless of the actual calculated value
        assert!(result.sharpe_ratio <= MAX_SHARPE_RATIO);
        assert!(result.sharpe_ratio >= -MAX_SHARPE_RATIO);
    }

    #[test]
    fn test_legacy_sharpe_ratio_calculation() {
        // Test legacy function also handles edge cases properly
        let portfolio_values = vec![
            pv(Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(), 1000.0),
            pv(Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap(), 1000.0),
        ];

        let result =
            calculate_performance_metrics_legacy(1000.0, 1000.0, &portfolio_values, &[], 1);

        // Legacy function should also handle zero volatility case
        assert_eq!(result.sharpe_ratio, 0.0); // No return, no volatility
    }

    #[test]
    fn test_volatility_calculation() {
        // Test the volatility calculation function directly
        let returns = vec![0.01, -0.005, 0.008, -0.002, 0.015];
        let volatility = calculate_volatility(&returns);

        assert!(volatility > 0.0);
        assert!(volatility.is_finite());

        // Test with constant returns (zero volatility)
        let constant_returns = vec![0.0, 0.0, 0.0, 0.0];
        let zero_volatility = calculate_volatility(&constant_returns);
        assert_eq!(zero_volatility, 0.0);

        // Test with single return
        let single_return = vec![0.05];
        let single_volatility = calculate_volatility(&single_return);
        assert_eq!(single_volatility, 0.0);
    }
}
