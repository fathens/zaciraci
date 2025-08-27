use crate::Result;
use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ==================== 型定義 ====================

/// トレンド強度の種類
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TrendStrength {
    Strong,
    Moderate,
    Weak,
    NoTrend,
}

/// トレンド方向
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TrendDirection {
    Upward,
    Downward,
    Sideways,
}

/// トレンド分析結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendAnalysis {
    pub token: String,
    pub direction: TrendDirection,
    pub strength: TrendStrength,
    pub slope: f64,
    pub r_squared: f64,
    pub volume_trend: f64,
    pub breakout_signal: bool,
    pub timestamp: DateTime<Utc>,
}

/// テクニカル指標データ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechnicalIndicators {
    pub rsi: Option<f64>,
    pub macd: Option<f64>,
    pub macd_signal: Option<f64>,
    pub adx: Option<f64>,
    pub volume_ma: Option<f64>,
    pub price_ma_short: Option<f64>,
    pub price_ma_long: Option<f64>,
}

/// トレンドフォロー取引アクション
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TrendTradingAction {
    /// 強いトレンドに乗る
    EnterTrend { token: String, position_size: f64 },
    /// トレンドから退出
    ExitTrend { token: String },
    /// ポジションサイズを調整
    AdjustPosition { token: String, new_size: f64 },
    /// 待機
    Wait,
}

/// 実行レポート
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendExecutionReport {
    pub actions: Vec<TrendTradingAction>,
    pub trend_analysis: Vec<TrendAnalysis>,
    pub timestamp: DateTime<Utc>,
    pub total_signals: usize,
    pub strong_trends: usize,
    pub breakout_signals: usize,
}

/// ポジション情報
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TrendPosition {
    pub token: String,
    pub size: f64,
    pub entry_price: BigDecimal,
    pub entry_time: DateTime<Utc>,
    pub current_price: BigDecimal,
    pub unrealized_pnl: f64,
}

// ==================== 定数 ====================

/// RSI上限閾値（買われすぎ）
#[allow(dead_code)]
const RSI_OVERBOUGHT: f64 = 70.0;

/// RSI下限閾値（売られすぎ）
#[allow(dead_code)]
const RSI_OVERSOLD: f64 = 30.0;

/// ADX強トレンド閾値
#[allow(dead_code)]
const ADX_STRONG_TREND: f64 = 25.0;

/// 線形回帰のR²閾値（トレンド強度判定）
const R_SQUARED_THRESHOLD: f64 = 0.7;

/// ブレイクアウトのボリューム倍率
const VOLUME_BREAKOUT_MULTIPLIER: f64 = 1.5;

/// 最小トレンド期間（時間）
#[allow(dead_code)]
const MIN_TREND_HOURS: i64 = 6;

/// ポジションサイズの最大値（資金の30%）
const MAX_POSITION_SIZE: f64 = 0.3;

/// Kelly Criterionのリスク調整係数
const KELLY_RISK_FACTOR: f64 = 0.25;

// ==================== コアアルゴリズム ====================

/// 線形回帰によるトレンド分析
#[allow(dead_code)]
pub fn calculate_trend_strength(
    prices: &[f64],
    timestamps: &[DateTime<Utc>],
) -> (f64, f64, TrendDirection, TrendStrength) {
    if prices.len() < 3 || timestamps.len() != prices.len() {
        return (0.0, 0.0, TrendDirection::Sideways, TrendStrength::NoTrend);
    }

    let n = prices.len() as f64;

    // 時間を数値に変換（開始時間からの経過時間）
    let start_time = timestamps[0];
    let x_values: Vec<f64> = timestamps
        .iter()
        .map(|t| (*t - start_time).num_seconds() as f64)
        .collect();

    // 線形回帰計算
    let sum_x: f64 = x_values.iter().sum();
    let sum_y: f64 = prices.iter().sum();
    let sum_xy: f64 = x_values.iter().zip(prices.iter()).map(|(x, y)| x * y).sum();
    let sum_x_squared: f64 = x_values.iter().map(|x| x * x).sum();
    let _sum_y_squared: f64 = prices.iter().map(|y| y * y).sum();

    // 傾き（slope）の計算
    let slope = (n * sum_xy - sum_x * sum_y) / (n * sum_x_squared - sum_x * sum_x);

    // 決定係数（R²）の計算
    let y_mean = sum_y / n;
    let ss_tot: f64 = prices.iter().map(|y| (y - y_mean).powi(2)).sum();
    let ss_res: f64 = x_values
        .iter()
        .zip(prices.iter())
        .map(|(x, y)| {
            let y_pred = y_mean + slope * (x - sum_x / n);
            (y - y_pred).powi(2)
        })
        .sum();

    let r_squared = if ss_tot != 0.0 {
        1.0 - (ss_res / ss_tot)
    } else {
        0.0
    };

    // トレンド方向の判定
    let direction = if slope.abs() < 0.0001 {
        TrendDirection::Sideways
    } else if slope > 0.0 {
        TrendDirection::Upward
    } else {
        TrendDirection::Downward
    };

    // トレンド強度の判定
    let strength = if r_squared > R_SQUARED_THRESHOLD && slope.abs() > 0.001 {
        TrendStrength::Strong
    } else if r_squared > 0.4 && slope.abs() > 0.0005 {
        TrendStrength::Moderate
    } else if r_squared > 0.2 {
        TrendStrength::Weak
    } else {
        TrendStrength::NoTrend
    };

    (slope, r_squared, direction, strength)
}

/// RSI計算（相対力指数）
#[allow(dead_code)]
pub fn calculate_rsi(prices: &[f64], period: usize) -> Option<f64> {
    if prices.len() < period + 1 {
        return None;
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

    if gains.len() < period {
        return None;
    }

    // 直近period分の平均を計算
    let recent_gains: f64 = gains[gains.len() - period..].iter().sum();
    let recent_losses: f64 = losses[losses.len() - period..].iter().sum();

    let avg_gain = recent_gains / period as f64;
    let avg_loss = recent_losses / period as f64;

    if avg_loss == 0.0 {
        return Some(100.0);
    }

    let rs = avg_gain / avg_loss;
    Some(100.0 - (100.0 / (1.0 + rs)))
}

/// MACD計算（移動平均収束拡散法）
#[allow(dead_code)]
pub fn calculate_macd(
    prices: &[f64],
    fast_period: usize,
    slow_period: usize,
    signal_period: usize,
) -> (Option<f64>, Option<f64>) {
    if prices.len() < slow_period || prices.len() < signal_period {
        return (None, None);
    }

    // EMA計算のヘルパー関数
    let calculate_ema = |data: &[f64], period: usize| -> Vec<f64> {
        let alpha = 2.0 / (period as f64 + 1.0);
        let mut ema = Vec::new();
        ema.push(data[0]);

        for i in 1..data.len() {
            let new_ema = alpha * data[i] + (1.0 - alpha) * ema[i - 1];
            ema.push(new_ema);
        }
        ema
    };

    let fast_ema = calculate_ema(prices, fast_period);
    let slow_ema = calculate_ema(prices, slow_period);

    if fast_ema.len() != slow_ema.len() {
        return (None, None);
    }

    // MACD線を計算
    let macd_line: Vec<f64> = fast_ema
        .iter()
        .zip(slow_ema.iter())
        .map(|(fast, slow)| fast - slow)
        .collect();

    // シグナル線を計算
    let signal_line = calculate_ema(&macd_line, signal_period);

    let current_macd = macd_line.last().copied();
    let current_signal = signal_line.last().copied();

    (current_macd, current_signal)
}

/// ADX計算（平均方向性指数）
#[allow(dead_code)]
pub fn calculate_adx(highs: &[f64], lows: &[f64], closes: &[f64], period: usize) -> Option<f64> {
    if highs.len() < period + 1 || highs.len() != lows.len() || highs.len() != closes.len() {
        return None;
    }

    let mut true_ranges = Vec::new();
    let mut plus_dms = Vec::new();
    let mut minus_dms = Vec::new();

    // True Range, +DM, -DMの計算
    for i in 1..highs.len() {
        let high_low = highs[i] - lows[i];
        let high_close_prev = (highs[i] - closes[i - 1]).abs();
        let low_close_prev = (lows[i] - closes[i - 1]).abs();
        let true_range = high_low.max(high_close_prev).max(low_close_prev);
        true_ranges.push(true_range);

        let plus_dm =
            if highs[i] - highs[i - 1] > lows[i - 1] - lows[i] && highs[i] - highs[i - 1] > 0.0 {
                highs[i] - highs[i - 1]
            } else {
                0.0
            };
        plus_dms.push(plus_dm);

        let minus_dm =
            if lows[i - 1] - lows[i] > highs[i] - highs[i - 1] && lows[i - 1] - lows[i] > 0.0 {
                lows[i - 1] - lows[i]
            } else {
                0.0
            };
        minus_dms.push(minus_dm);
    }

    if true_ranges.len() < period {
        return None;
    }

    // 移動平均を計算
    let atr = true_ranges[true_ranges.len() - period..]
        .iter()
        .sum::<f64>()
        / period as f64;
    let plus_di =
        (plus_dms[plus_dms.len() - period..].iter().sum::<f64>() / period as f64) / atr * 100.0;
    let minus_di =
        (minus_dms[minus_dms.len() - period..].iter().sum::<f64>() / period as f64) / atr * 100.0;

    // ADXの計算
    let dx = ((plus_di - minus_di).abs() / (plus_di + minus_di)) * 100.0;
    Some(dx)
}

/// ボリュームトレンド分析
#[allow(dead_code)]
pub fn analyze_volume_trend(volumes: &[f64], prices: &[f64]) -> f64 {
    if volumes.len() != prices.len() || volumes.len() < 2 {
        return 0.0;
    }

    let mut volume_price_correlation = 0.0;
    let mut valid_pairs = 0;

    for i in 1..volumes.len() {
        let price_change = prices[i] - prices[i - 1];
        let volume_change = volumes[i] - volumes[i - 1];

        if price_change != 0.0 && volume_change != 0.0 {
            // 価格上昇時にボリューム増加、価格下降時にボリューム減少なら正の相関
            let correlation = if (price_change > 0.0 && volume_change > 0.0)
                || (price_change < 0.0 && volume_change < 0.0)
            {
                1.0
            } else {
                -1.0
            };
            volume_price_correlation += correlation;
            valid_pairs += 1;
        }
    }

    if valid_pairs == 0 {
        0.0
    } else {
        volume_price_correlation / valid_pairs as f64
    }
}

/// ブレイクアウト検出
#[allow(dead_code)]
pub fn detect_breakout(
    current_price: f64,
    resistance: f64,
    support: f64,
    current_volume: f64,
    avg_volume: f64,
) -> bool {
    let volume_confirmed = current_volume > avg_volume * VOLUME_BREAKOUT_MULTIPLIER;
    let price_breakout = current_price > resistance || current_price < support;

    volume_confirmed && price_breakout
}

/// Kelly Criterionによるポジションサイズ計算
#[allow(dead_code)]
pub fn calculate_kelly_position_size(
    win_rate: f64,
    avg_win: f64,
    avg_loss: f64,
    risk_factor: f64,
) -> f64 {
    if avg_loss == 0.0 || win_rate >= 1.0 || win_rate <= 0.0 {
        return 0.0;
    }

    let lose_rate = 1.0 - win_rate;
    let win_loss_ratio = avg_win / avg_loss;

    let kelly_percentage = (win_rate * win_loss_ratio - lose_rate) / win_loss_ratio;
    let adjusted_kelly = kelly_percentage * risk_factor;

    adjusted_kelly.clamp(0.0, MAX_POSITION_SIZE)
}

/// 総合トレンド分析
#[allow(dead_code)]
pub fn analyze_trend(
    token: &str,
    prices: &[f64],
    timestamps: &[DateTime<Utc>],
    volumes: &[f64],
    highs: &[f64],
    lows: &[f64],
) -> TrendAnalysis {
    // 基本トレンド分析
    let (slope, r_squared, direction, strength) = calculate_trend_strength(prices, timestamps);

    // テクニカル指標
    let _rsi = calculate_rsi(prices, 14);
    let (_macd, _macd_signal) = calculate_macd(prices, 12, 26, 9);
    let _adx = calculate_adx(highs, lows, prices, 14);

    // ボリューム分析
    let volume_trend = analyze_volume_trend(volumes, prices);

    // ブレイクアウト検出
    let avg_volume = if volumes.len() > 20 {
        volumes[volumes.len() - 20..].iter().sum::<f64>() / 20.0
    } else {
        volumes.iter().sum::<f64>() / volumes.len() as f64
    };

    let current_price = prices.last().copied().unwrap_or(0.0);
    let current_volume = volumes.last().copied().unwrap_or(0.0);

    // サポート・レジスタンス（簡易計算）
    let max_price = prices.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min_price = prices.iter().cloned().fold(f64::INFINITY, f64::min);

    let breakout_signal = detect_breakout(
        current_price,
        max_price,
        min_price,
        current_volume,
        avg_volume,
    );

    TrendAnalysis {
        token: token.to_string(),
        direction,
        strength,
        slope,
        r_squared,
        volume_trend,
        breakout_signal,
        timestamp: Utc::now(),
    }
}

/// トレンドフォロー取引判断
#[allow(dead_code)]
pub fn make_trend_trading_decision(
    trend_analysis: &TrendAnalysis,
    current_positions: &[TrendPosition],
    available_capital: f64,
) -> TrendTradingAction {
    let current_position = current_positions
        .iter()
        .find(|p| p.token == trend_analysis.token);

    match (&trend_analysis.strength, &trend_analysis.direction) {
        // 強いトレンドでブレイクアウトがある場合
        (TrendStrength::Strong, TrendDirection::Upward) if trend_analysis.breakout_signal => {
            if current_position.is_none() && available_capital > 0.0 {
                let position_size =
                    calculate_kelly_position_size(0.6, 0.15, 0.08, KELLY_RISK_FACTOR);
                TrendTradingAction::EnterTrend {
                    token: trend_analysis.token.clone(),
                    position_size: position_size.min(available_capital),
                }
            } else if let Some(pos) = current_position {
                // 既存ポジションのサイズ調整
                let new_size = (pos.size * 1.2).min(MAX_POSITION_SIZE);
                if new_size > pos.size {
                    TrendTradingAction::AdjustPosition {
                        token: trend_analysis.token.clone(),
                        new_size,
                    }
                } else {
                    TrendTradingAction::Wait
                }
            } else {
                TrendTradingAction::Wait
            }
        }

        // 弱いトレンドまたはサイドウェイの場合は退出
        (TrendStrength::Weak, _) | (TrendStrength::NoTrend, _) | (_, TrendDirection::Sideways) => {
            if current_position.is_some() {
                TrendTradingAction::ExitTrend {
                    token: trend_analysis.token.clone(),
                }
            } else {
                TrendTradingAction::Wait
            }
        }

        // その他の場合は様子見
        _ => TrendTradingAction::Wait,
    }
}

/// マーケットデータの型エイリアス
pub type MarketDataTuple = (Vec<f64>, Vec<DateTime<Utc>>, Vec<f64>, Vec<f64>, Vec<f64>);

/// トレンドフォロー戦略の実行
#[allow(dead_code)]
pub async fn execute_trend_following_strategy(
    tokens: Vec<String>,
    current_positions: Vec<TrendPosition>,
    available_capital: f64,
    market_data: &HashMap<String, MarketDataTuple>,
) -> Result<TrendExecutionReport> {
    let mut trend_analyses = Vec::new();
    let mut actions = Vec::new();

    for token in tokens {
        if let Some((prices, timestamps, volumes, highs, lows)) = market_data.get(&token) {
            let analysis = analyze_trend(&token, prices, timestamps, volumes, highs, lows);

            let action =
                make_trend_trading_decision(&analysis, &current_positions, available_capital);

            trend_analyses.push(analysis);

            if action != TrendTradingAction::Wait {
                actions.push(action);
            }
        }
    }

    let strong_trends = trend_analyses
        .iter()
        .filter(|a| a.strength == TrendStrength::Strong)
        .count();

    let breakout_signals = trend_analyses.iter().filter(|a| a.breakout_signal).count();

    Ok(TrendExecutionReport {
        actions,
        trend_analysis: trend_analyses.clone(),
        timestamp: Utc::now(),
        total_signals: trend_analyses.len(),
        strong_trends,
        breakout_signals,
    })
}

#[cfg(test)]
mod tests;
