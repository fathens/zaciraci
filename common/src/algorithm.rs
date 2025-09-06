pub mod momentum;
pub mod portfolio;
pub mod prediction;
pub mod trend_following;

use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use num_traits::Zero;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

// ==================== 共通型定義 ====================

/// 取引の種類
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TradeType {
    Buy,
    Sell,
    Swap,
}

/// 価格履歴データ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceHistory {
    pub timestamp: DateTime<Utc>,
    pub price: BigDecimal,
    pub volume: Option<BigDecimal>,
}

/// トークン情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub symbol: String,
    pub address: String,
    pub decimals: u8,
    pub current_price: BigDecimal,
}

/// 取引実行結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeExecution {
    pub trade_type: TradeType,
    pub token_in: String,
    pub token_out: String,
    pub amount_in: BigDecimal,
    pub amount_out: BigDecimal,
    pub timestamp: DateTime<Utc>,
    pub cost: BigDecimal,
    pub success: bool,
}

// ==================== 共通関数 ====================

/// 価格変化率を計算
pub fn calculate_price_change_percentage(old_price: &BigDecimal, new_price: &BigDecimal) -> f64 {
    if old_price.is_zero() {
        return 0.0;
    }

    let old = old_price.to_string().parse::<f64>().unwrap_or(0.0);
    let new = new_price.to_string().parse::<f64>().unwrap_or(0.0);

    ((new - old) / old) * 100.0
}

/// 移動平均を計算
pub fn calculate_moving_average(prices: &[f64], period: usize) -> Vec<f64> {
    if prices.len() < period || period == 0 {
        return vec![];
    }

    let mut averages = Vec::new();
    for i in (period - 1)..prices.len() {
        let sum: f64 = prices[(i + 1).saturating_sub(period)..=i].iter().sum();
        averages.push(sum / period as f64);
    }

    averages
}

/// RSIを計算
pub fn calculate_rsi(prices: &[f64], period: usize) -> Vec<f64> {
    if prices.len() <= period {
        return vec![];
    }

    let mut gains = Vec::new();
    let mut losses = Vec::new();

    // 価格変化を計算
    for i in 1..prices.len() {
        let change = prices[i] - prices[i - 1];
        if change > 0.0 {
            gains.push(change);
            losses.push(0.0);
        } else {
            gains.push(0.0);
            losses.push(-change);
        }
    }

    let mut rsi_values = Vec::new();

    // 最初の平均を計算
    let mut avg_gain: f64 = gains.iter().take(period).sum::<f64>() / period as f64;
    let mut avg_loss: f64 = losses.iter().take(period).sum::<f64>() / period as f64;

    // 最初のRSI
    let rs = if avg_loss == 0.0 {
        100.0
    } else {
        avg_gain / avg_loss
    };
    rsi_values.push(100.0 - (100.0 / (1.0 + rs)));

    // 残りのRSIを計算
    for i in period..gains.len() {
        avg_gain = (avg_gain * (period as f64 - 1.0) + gains[i]) / period as f64;
        avg_loss = (avg_loss * (period as f64 - 1.0) + losses[i]) / period as f64;

        let rs = if avg_loss == 0.0 {
            100.0
        } else {
            avg_gain / avg_loss
        };
        rsi_values.push(100.0 - (100.0 / (1.0 + rs)));
    }

    rsi_values
}

/// 取引コストを計算
pub fn calculate_trading_cost(amount: &BigDecimal, fee_rate: f64, slippage: f64) -> BigDecimal {
    let amount_f64 = amount.to_string().parse::<f64>().unwrap_or(0.0);
    let total_cost = amount_f64 * (fee_rate + slippage);
    BigDecimal::from_str(&total_cost.to_string()).unwrap_or_else(|_| BigDecimal::zero())
}

/// シャープレシオを計算
pub fn calculate_sharpe_ratio(returns: &[f64], risk_free_rate: f64) -> f64 {
    if returns.is_empty() {
        return 0.0;
    }

    let mean_return = returns.iter().sum::<f64>() / returns.len() as f64;
    let excess_return = mean_return - risk_free_rate;

    if excess_return == 0.0 {
        return 0.0;
    }

    let variance = returns
        .iter()
        .map(|r| (r - mean_return).powi(2))
        .sum::<f64>()
        / returns.len() as f64;

    let std_dev = variance.sqrt();

    if std_dev == 0.0 {
        0.0
    } else {
        excess_return / std_dev
    }
}

/// 最大ドローダウンを計算
pub fn calculate_max_drawdown(cumulative_returns: &[f64]) -> f64 {
    if cumulative_returns.is_empty() {
        return 0.0;
    }

    let mut peak = cumulative_returns[0];
    let mut max_drawdown = 0.0;

    for &value in cumulative_returns.iter().skip(1) {
        if value > peak {
            peak = value;
        }

        let drawdown = (peak - value) / peak;
        if drawdown > max_drawdown {
            max_drawdown = drawdown;
        }
    }

    max_drawdown
}

// ==================== ボラティリティ計算 ====================

/// 価格リストからボラティリティ（標準偏差）を計算
pub fn calculate_volatility_from_prices(prices: &[f64]) -> f64 {
    if prices.len() < 2 {
        return 0.0;
    }

    // リターンを計算
    let mut returns = Vec::new();
    for i in 1..prices.len() {
        if prices[i - 1] != 0.0 {
            let r = (prices[i] - prices[i - 1]) / prices[i - 1];
            returns.push(r);
        }
    }

    if returns.is_empty() {
        return 0.0;
    }

    // 平均リターン
    let mean: f64 = returns.iter().sum::<f64>() / returns.len() as f64;

    // 標準偏差
    let variance: f64 =
        returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;

    variance.sqrt()
}

/// ValueAtTimeからボラティリティを計算
pub fn calculate_volatility_from_value_at_time(values: &[crate::stats::ValueAtTime]) -> f64 {
    let prices: Vec<f64> = values.iter().map(|v| v.value).collect();
    calculate_volatility_from_prices(&prices)
}

/// ボラティリティスコア（0-1の範囲、年率化）を計算
pub fn calculate_volatility_score(values: &[crate::stats::ValueAtTime], annualize: bool) -> f64 {
    let volatility = calculate_volatility_from_value_at_time(values);

    if annualize && volatility > 0.0 {
        // 年率化（1日24時間、365日として）
        let annualized_volatility = volatility * (365.0_f64 * 24.0_f64).sqrt();
        // 0-1の範囲にクランプ
        annualized_volatility.clamp(0.0, 1.0)
    } else {
        volatility
    }
}

// ==================== パフォーマンス指標 ====================

/// パフォーマンス指標
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub total_return: f64,
    pub sharpe_ratio: f64,
    pub max_drawdown: f64,
    pub win_rate: f64,
    pub total_trades: usize,
}

// ==================== テスト ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_moving_average() {
        let prices = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let ma = calculate_moving_average(&prices, 3);

        assert_eq!(ma.len(), 4);
        assert_eq!(ma[0], 2.0); // (1+2+3)/3
        assert_eq!(ma[1], 3.0); // (2+3+4)/3
        assert_eq!(ma[2], 4.0); // (3+4+5)/3
        assert_eq!(ma[3], 5.0); // (4+5+6)/3

        // period が配列長より大きい場合
        let ma = calculate_moving_average(&prices, 10);
        assert!(ma.is_empty());

        // period が 0 の場合
        let ma = calculate_moving_average(&prices, 0);
        assert!(ma.is_empty());

        // 空配列の場合
        let ma = calculate_moving_average(&[], 3);
        assert!(ma.is_empty());
    }

    #[test]
    fn test_calculate_volatility_functions() {
        let prices = vec![100.0, 105.0, 103.0, 108.0, 106.0];
        let volatility = calculate_volatility_from_prices(&prices);
        assert!(volatility > 0.0 && volatility < 0.05);

        // ValueAtTime を使用
        let values: Vec<crate::stats::ValueAtTime> = prices
            .iter()
            .enumerate()
            .map(|(i, &price)| crate::stats::ValueAtTime {
                time: chrono::Utc::now().naive_utc() + chrono::Duration::hours(i as i64),
                value: price,
            })
            .collect();

        let volatility2 = calculate_volatility_from_value_at_time(&values);
        assert!((volatility - volatility2).abs() < 0.0001);

        // ボラティリティスコア（年率化）
        let score = calculate_volatility_score(&values, true);
        assert!((0.0..=1.0).contains(&score));
    }

    #[test]
    fn test_calculate_rsi() {
        let prices = vec![
            44.0, 44.25, 44.5, 43.75, 44.5, 44.75, 44.5, 44.25, 44.0, 44.25, 44.75, 45.0, 45.25,
            45.5, 45.25,
        ];
        let rsi = calculate_rsi(&prices, 14);

        // RSI should return some values
        assert!(!rsi.is_empty());

        // RSI values should be between 0 and 100
        for value in rsi {
            assert!((0.0..=100.0).contains(&value));
        }
    }
}
