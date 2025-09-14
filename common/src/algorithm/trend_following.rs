use crate::Result;
use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::types::*;

// ==================== トレンドフォロー固有の型定義 ====================

/// 実行レポート
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendExecutionReport {
    pub actions: Vec<TradingAction>,
    pub trend_analysis: Vec<TrendAnalysis>,
    pub timestamp: DateTime<Utc>,
    pub total_signals: usize,
    pub strong_trends: usize,
    pub breakout_signals: usize,
}

/// ポジション情報
#[derive(Debug, Clone)]
pub struct TrendPosition {
    pub token: String,
    pub size: f64,
    pub entry_price: BigDecimal,
    pub entry_time: DateTime<Utc>,
    pub current_price: BigDecimal,
    pub unrealized_pnl: f64,
}

// ==================== 定数 ====================

/// ブレイクアウトのボリューム倍率
const VOLUME_BREAKOUT_MULTIPLIER: f64 = 1.5;

/// 最小トレンド期間（時間）
const MIN_TREND_HOURS: i64 = 6;

/// ポジションサイズの最大値（資金の30%）
const MAX_POSITION_SIZE: f64 = 0.3;

/// Kelly Criterionのリスク調整係数
const KELLY_RISK_FACTOR: f64 = 0.25;

// ==================== コアアルゴリズム ====================

/// 線形回帰によるトレンド分析
pub fn calculate_trend_strength(
    prices: &[f64],
    timestamps: &[DateTime<Utc>],
    r_squared_threshold: f64,
) -> (f64, f64, TrendDirection, TrendStrength) {
    if prices.len() < 3 || timestamps.len() != prices.len() {
        return (0.0, 0.0, TrendDirection::Sideways, TrendStrength::NoTrend);
    }

    // トレンド期間の確認
    let duration = *timestamps.last().unwrap() - timestamps[0];
    if duration.num_hours() < MIN_TREND_HOURS {
        // トレンドが最小期間を満たしていない場合は弱いトレンドとする
        return (0.0, 0.0, TrendDirection::Sideways, TrendStrength::Weak);
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

    // トレンド強度の判定（パラメータ化）
    let strength = if r_squared > r_squared_threshold && slope.abs() > 0.001 {
        TrendStrength::Strong
    } else if r_squared > (r_squared_threshold * 0.6) && slope.abs() > 0.0005 {
        TrendStrength::Moderate
    } else if r_squared > (r_squared_threshold * 0.3) {
        TrendStrength::Weak
    } else {
        TrendStrength::NoTrend
    };

    (slope, r_squared, direction, strength)
}

/// RSI計算（相対力指数） - 最新の値のみを返す
pub fn calculate_rsi(prices: &[f64], period: usize) -> Option<f64> {
    let rsi_values = crate::algorithm::calculate_rsi(prices, period);
    rsi_values.last().copied()
}

/// MACD計算（移動平均収束拡散法）
pub fn calculate_macd(
    prices: &[f64],
    fast_period: usize,
    slow_period: usize,
    signal_period: usize,
) -> (Option<f64>, Option<f64>) {
    let macd_data =
        crate::algorithm::calculate_macd(prices, fast_period, slow_period, signal_period);

    if let Some((macd, signal, _histogram)) = macd_data.last() {
        (Some(*macd), Some(*signal))
    } else {
        (None, None)
    }
}

/// ADX計算（平均方向性指数）
pub fn calculate_adx(highs: &[f64], lows: &[f64], closes: &[f64], period: usize) -> Option<f64> {
    let adx_values = crate::algorithm::calculate_adx(highs, lows, closes, period);
    adx_values.last().copied()
}

/// ボリュームトレンド分析
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
pub fn analyze_trend(
    token: &str,
    prices: &[f64],
    timestamps: &[DateTime<Utc>],
    volumes: &[f64],
    highs: &[f64],
    lows: &[f64],
    r_squared_threshold: f64,
) -> TrendAnalysis {
    // 基本トレンド分析
    let (slope, r_squared, direction, strength) =
        calculate_trend_strength(prices, timestamps, r_squared_threshold);

    // テクニカル指標
    let rsi = calculate_rsi(prices, 14);
    let (_macd, _macd_signal) = calculate_macd(prices, 12, 26, 9);
    let adx = calculate_adx(highs, lows, prices, 14);

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
        rsi,
        adx,
        timestamp: Utc::now(),
    }
}

/// トレンドフォロー取引判断
pub fn make_trend_trading_decision(
    trend_analysis: &TrendAnalysis,
    current_positions: &[TrendPosition],
    available_capital: f64,
    rsi_overbought: f64,
    rsi_oversold: f64,
    adx_strong_threshold: f64,
) -> TradingAction {
    let current_position = current_positions
        .iter()
        .find(|p| p.token == trend_analysis.token);

    // RSIとADXのチェック（パラメータ化）
    let rsi_overbought_flag = trend_analysis.rsi.is_some_and(|rsi| rsi > rsi_overbought);
    let rsi_oversold_flag = trend_analysis.rsi.is_some_and(|rsi| rsi < rsi_oversold);
    let strong_trend = trend_analysis
        .adx
        .is_some_and(|adx| adx > adx_strong_threshold);

    match (&trend_analysis.strength, &trend_analysis.direction) {
        // 強いトレンドでブレイクアウトがある場合
        (TrendStrength::Strong, TrendDirection::Upward)
            if trend_analysis.breakout_signal && !rsi_overbought_flag && strong_trend =>
        {
            if current_position.is_none() && available_capital > 0.0 {
                let _position_size =
                    calculate_kelly_position_size(0.6, 0.15, 0.08, KELLY_RISK_FACTOR);
                TradingAction::Switch {
                    from: "cash".to_string(),
                    to: trend_analysis.token.clone(),
                }
            } else if let Some(_pos) = current_position {
                // 既存ポジションのサイズ調整（RSIが買われすぎでない場合のみ）
                // Position adjustment logic (simplified to Hold for now)
                TradingAction::Hold
            } else {
                TradingAction::Hold
            }
        }

        // 下降トレンドでRSIが売られすぎの場合は逆張りのチャンス
        (_, TrendDirection::Downward) if rsi_oversold_flag && strong_trend => {
            if current_position.is_none() && available_capital > 0.0 {
                let _position_size =
                    calculate_kelly_position_size(0.5, 0.12, 0.10, KELLY_RISK_FACTOR * 0.8);
                TradingAction::Switch {
                    from: "cash".to_string(),
                    to: trend_analysis.token.clone(),
                }
            } else {
                TradingAction::Hold
            }
        }

        // RSIが買われすぎの場合は退出シグナル
        _ if rsi_overbought_flag && current_position.is_some() => TradingAction::Sell {
            token: trend_analysis.token.clone(),
            target: "cash".to_string(),
        },

        // 弱いトレンドまたはサイドウェイの場合は退出
        (TrendStrength::Weak, _) | (TrendStrength::NoTrend, _) | (_, TrendDirection::Sideways)
            if !strong_trend =>
        {
            if current_position.is_some() {
                TradingAction::Sell {
                    token: trend_analysis.token.clone(),
                    target: "cash".to_string(),
                }
            } else {
                TradingAction::Hold
            }
        }

        // その他の場合は様子見
        _ => TradingAction::Hold,
    }
}

/// マーケットデータの型エイリアス
pub type MarketDataTuple = (Vec<f64>, Vec<DateTime<Utc>>, Vec<f64>, Vec<f64>, Vec<f64>);

/// TrendFollowing実行のパラメータ
#[derive(Debug, Clone)]
pub struct TrendFollowingParams {
    pub rsi_overbought: f64,
    pub rsi_oversold: f64,
    pub adx_strong_threshold: f64,
    pub r_squared_threshold: f64,
}

impl Default for TrendFollowingParams {
    fn default() -> Self {
        Self {
            rsi_overbought: 80.0,
            rsi_oversold: 20.0,
            adx_strong_threshold: 20.0,
            r_squared_threshold: 0.5,
        }
    }
}

/// トレンドフォロー戦略の実行
pub async fn execute_trend_following_strategy(
    tokens: Vec<String>,
    current_positions: Vec<TrendPosition>,
    available_capital: f64,
    market_data: &HashMap<String, MarketDataTuple>,
    params: TrendFollowingParams,
) -> Result<TrendExecutionReport> {
    let mut trend_analyses = Vec::new();
    let mut actions = Vec::new();

    for token in tokens {
        if let Some((prices, timestamps, volumes, highs, lows)) = market_data.get(&token) {
            let analysis = analyze_trend(
                &token,
                prices,
                timestamps,
                volumes,
                highs,
                lows,
                params.r_squared_threshold,
            );

            let action = make_trend_trading_decision(
                &analysis,
                &current_positions,
                available_capital,
                params.rsi_overbought,
                params.rsi_oversold,
                params.adx_strong_threshold,
            );

            trend_analyses.push(analysis);

            if action != TradingAction::Hold {
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
