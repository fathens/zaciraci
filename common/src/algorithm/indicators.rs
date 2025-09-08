use bigdecimal::BigDecimal;
use num_traits::Zero;
use std::str::FromStr;

use crate::stats::ValueAtTime;

// ==================== テクニカル指標計算 ====================

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

/// RSI（相対力指数）を計算
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

// ==================== パフォーマンス指標 ====================

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

/// ソルティノレシオを計算
pub fn calculate_sortino_ratio(returns: &[f64], risk_free_rate: f64) -> f64 {
    if returns.is_empty() {
        return 0.0;
    }

    let mean_return: f64 = returns.iter().sum::<f64>() / returns.len() as f64;
    let excess_return = mean_return - risk_free_rate;

    // 下方偏差を計算
    let downside_returns: Vec<f64> = returns
        .iter()
        .map(|&r| (r - risk_free_rate).min(0.0))
        .collect();

    let downside_deviation = if downside_returns.is_empty() {
        0.0
    } else {
        let variance: f64 =
            downside_returns.iter().map(|r| r.powi(2)).sum::<f64>() / downside_returns.len() as f64;
        variance.sqrt()
    };

    if downside_deviation == 0.0 {
        0.0
    } else {
        excess_return / downside_deviation
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
pub fn calculate_volatility_from_value_at_time(values: &[ValueAtTime]) -> f64 {
    let prices: Vec<f64> = values.iter().map(|v| v.value).collect();
    calculate_volatility_from_prices(&prices)
}

/// ボラティリティスコア（0-1の範囲、年率化）を計算
pub fn calculate_volatility_score(values: &[ValueAtTime], annualize: bool) -> f64 {
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

// ==================== 新しい共通指標関数 ====================

/// モメンタムスコアを計算（価格変化の勢い）
pub fn calculate_momentum_score(prices: &[f64], period: usize) -> f64 {
    if prices.len() < period + 1 {
        return 0.0;
    }

    let current_price = prices[prices.len() - 1];
    let past_price = prices[prices.len() - 1 - period];

    if past_price == 0.0 {
        return 0.0;
    }

    (current_price - past_price) / past_price
}

/// MACD（移動平均収束拡散）を計算
pub fn calculate_macd(
    prices: &[f64],
    fast_period: usize,
    slow_period: usize,
    signal_period: usize,
) -> Vec<(f64, f64, f64)> {
    if prices.len() < slow_period {
        return vec![];
    }

    let fast_ema = calculate_exponential_moving_average(prices, fast_period);
    let slow_ema = calculate_exponential_moving_average(prices, slow_period);

    if fast_ema.len() != slow_ema.len() {
        return vec![];
    }

    // MACD線を計算
    let macd_line: Vec<f64> = fast_ema
        .iter()
        .zip(slow_ema.iter())
        .map(|(fast, slow)| fast - slow)
        .collect();

    // シグナル線を計算
    let signal_line = calculate_exponential_moving_average(&macd_line, signal_period);

    // ヒストグラムを計算
    let histogram: Vec<f64> = if macd_line.len() >= signal_line.len() {
        macd_line
            .iter()
            .skip(macd_line.len() - signal_line.len())
            .zip(signal_line.iter())
            .map(|(macd, signal)| macd - signal)
            .collect()
    } else {
        vec![]
    };

    // 結果をタプルで返す (MACD, Signal, Histogram)
    macd_line
        .iter()
        .skip(macd_line.len().saturating_sub(histogram.len()))
        .zip(signal_line.iter())
        .zip(histogram.iter())
        .map(|((macd, signal), hist)| (*macd, *signal, *hist))
        .collect()
}

/// 指数移動平均（EMA）を計算
pub fn calculate_exponential_moving_average(prices: &[f64], period: usize) -> Vec<f64> {
    if prices.is_empty() || period == 0 {
        return vec![];
    }

    let mut ema_values = Vec::new();
    let multiplier = 2.0 / (period as f64 + 1.0);

    // 最初の値は単純移動平均から始める
    if prices.len() >= period {
        let sma: f64 = prices.iter().take(period).sum::<f64>() / period as f64;
        ema_values.push(sma);

        // 残りの値はEMA計算
        for &price in prices.iter().skip(period) {
            let previous_ema = *ema_values.last().unwrap();
            let ema = (price - previous_ema) * multiplier + previous_ema;
            ema_values.push(ema);
        }
    }

    ema_values
}

/// ボリンジャーバンドを計算
pub fn calculate_bollinger_bands(
    prices: &[f64],
    period: usize,
    std_dev_multiplier: f64,
) -> Vec<(f64, f64, f64)> {
    let moving_averages = calculate_moving_average(prices, period);

    if moving_averages.is_empty() {
        return vec![];
    }

    let mut bands = Vec::new();

    for (i, &ma) in moving_averages.iter().enumerate() {
        let start_idx = i;
        let end_idx = i + period;

        if end_idx <= prices.len() {
            let window = &prices[start_idx..end_idx];
            let variance = window
                .iter()
                .map(|&price| (price - ma).powi(2))
                .sum::<f64>()
                / period as f64;
            let std_dev = variance.sqrt();

            let upper_band = ma + (std_dev_multiplier * std_dev);
            let lower_band = ma - (std_dev_multiplier * std_dev);

            bands.push((upper_band, ma, lower_band));
        }
    }

    bands
}

/// ADX（平均方向性指数）を計算
pub fn calculate_adx(high: &[f64], low: &[f64], close: &[f64], period: usize) -> Vec<f64> {
    if high.len() != low.len() || low.len() != close.len() || close.len() < period + 1 {
        return vec![];
    }

    let mut tr_values = Vec::new();
    let mut plus_dm = Vec::new();
    let mut minus_dm = Vec::new();

    // TR (True Range) と DM (Directional Movement) を計算
    for i in 1..close.len() {
        let tr = (high[i] - low[i])
            .max((high[i] - close[i - 1]).abs())
            .max((low[i] - close[i - 1]).abs());
        tr_values.push(tr);

        let up_move = high[i] - high[i - 1];
        let down_move = low[i - 1] - low[i];

        let plus_dm_val = if up_move > down_move && up_move > 0.0 {
            up_move
        } else {
            0.0
        };
        let minus_dm_val = if down_move > up_move && down_move > 0.0 {
            down_move
        } else {
            0.0
        };

        plus_dm.push(plus_dm_val);
        minus_dm.push(minus_dm_val);
    }

    // ATR (Average True Range) を計算
    let atr = calculate_moving_average(&tr_values, period);
    let plus_di_raw = calculate_moving_average(&plus_dm, period);
    let minus_di_raw = calculate_moving_average(&minus_dm, period);

    if atr.is_empty() || plus_di_raw.is_empty() || minus_di_raw.is_empty() {
        return vec![];
    }

    // DI+ と DI- を計算
    let mut dx_values = Vec::new();

    for ((plus_avg, minus_avg), atr_val) in
        plus_di_raw.iter().zip(minus_di_raw.iter()).zip(atr.iter())
    {
        if *atr_val != 0.0 {
            let plus_di = (plus_avg / atr_val) * 100.0;
            let minus_di = (minus_avg / atr_val) * 100.0;

            let dx = if plus_di + minus_di != 0.0 {
                ((plus_di - minus_di).abs() / (plus_di + minus_di)) * 100.0
            } else {
                0.0
            };
            dx_values.push(dx);
        }
    }

    // ADX を計算（DX の移動平均）
    calculate_moving_average(&dx_values, period)
}

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
    }

    #[test]
    fn test_calculate_momentum_score() {
        let prices = vec![100.0, 105.0, 110.0, 115.0, 120.0];
        let score = calculate_momentum_score(&prices, 2);

        // (120 - 110) / 110 = 0.0909...
        assert!((score - 0.0909).abs() < 0.001);
    }

    #[test]
    fn test_calculate_volatility_from_prices() {
        let prices = vec![100.0, 105.0, 103.0, 108.0, 106.0];
        let volatility = calculate_volatility_from_prices(&prices);
        assert!(volatility > 0.0 && volatility < 0.05);
    }
}
