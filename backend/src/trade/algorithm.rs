pub mod momentum;

use crate::Result;
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

/// 取引の実行結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeResult {
    pub trade_type: TradeType,
    pub from_token: String,
    pub to_token: String,
    pub amount: BigDecimal,
    pub executed_price: BigDecimal,
    pub timestamp: DateTime<Utc>,
    pub transaction_hash: Option<String>,
    pub gas_used: Option<u64>,
    pub success: bool,
    pub error_message: Option<String>,
}

/// アルゴリズムの設定パラメータ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlgorithmConfig {
    pub name: String,
    pub parameters: std::collections::HashMap<String, serde_json::Value>,
    pub enabled: bool,
    pub max_trade_amount: BigDecimal,
    pub min_trade_amount: BigDecimal,
    pub max_slippage: f64,
}

/// 価格データポイント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceDataPoint {
    pub timestamp: DateTime<Utc>,
    pub price: BigDecimal,
    pub volume: Option<BigDecimal>,
}

/// トークンの市場データ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketData {
    pub token: String,
    pub quote_token: String,
    pub current_price: BigDecimal,
    pub price_history: Vec<PriceDataPoint>,
    pub volume_24h: Option<BigDecimal>,
    pub market_cap: Option<BigDecimal>,
    pub last_updated: DateTime<Utc>,
}

// ==================== 共通ユーティリティ関数 ====================

/// 価格変化率を計算
#[allow(dead_code)]
pub fn calculate_price_change_percentage(old_price: &BigDecimal, new_price: &BigDecimal) -> f64 {
    if old_price.is_zero() {
        return 0.0;
    }

    let old = old_price.to_string().parse::<f64>().unwrap_or(0.0);
    let new = new_price.to_string().parse::<f64>().unwrap_or(0.0);

    ((new - old) / old) * 100.0
}

/// 移動平均を計算
#[allow(dead_code)]
pub fn calculate_moving_average(prices: &[f64], window: usize) -> Vec<f64> {
    if prices.len() < window {
        return Vec::new();
    }

    let mut averages = Vec::new();
    for i in window..=prices.len() {
        let sum: f64 = prices[i - window..i].iter().sum();
        averages.push(sum / window as f64);
    }
    averages
}

/// RSI（相対力指数）を計算
#[allow(dead_code)]
pub fn calculate_rsi(prices: &[f64], period: usize) -> Vec<f64> {
    if prices.len() < period + 1 {
        return Vec::new();
    }

    let mut rsi_values = Vec::new();

    // 価格変化を計算
    let mut gains = Vec::new();
    let mut losses = Vec::new();

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

    // RSI計算
    for i in period..gains.len() {
        let avg_gain: f64 = gains[i - period + 1..=i].iter().sum::<f64>() / period as f64;
        let avg_loss: f64 = losses[i - period + 1..=i].iter().sum::<f64>() / period as f64;

        if avg_loss == 0.0 {
            rsi_values.push(100.0);
        } else {
            let rs = avg_gain / avg_loss;
            let rsi = 100.0 - (100.0 / (1.0 + rs));
            rsi_values.push(rsi);
        }
    }

    rsi_values
}

/// 取引コストを計算
#[allow(dead_code)]
pub fn calculate_trading_cost(
    amount: &BigDecimal,
    fee_rate: f64,
    gas_cost: Option<BigDecimal>,
) -> BigDecimal {
    let amount_f64 = amount.to_string().parse::<f64>().unwrap_or(0.0);
    let fee = BigDecimal::from_str(&(amount_f64 * fee_rate).to_string())
        .unwrap_or_else(|_| BigDecimal::zero());

    match gas_cost {
        Some(gas) => fee + gas,
        None => fee,
    }
}

/// リスク調整リターンを計算（シャープレシオ）
#[allow(dead_code)]
pub fn calculate_sharpe_ratio(returns: &[f64], risk_free_rate: f64) -> f64 {
    if returns.is_empty() {
        return 0.0;
    }

    let avg_return: f64 = returns.iter().sum::<f64>() / returns.len() as f64;
    let excess_return = avg_return - risk_free_rate;

    // 標準偏差を計算
    let variance: f64 = returns
        .iter()
        .map(|r| (r - avg_return).powi(2))
        .sum::<f64>()
        / returns.len() as f64;
    let std_dev = variance.sqrt();

    if std_dev == 0.0 {
        return 0.0;
    }

    excess_return / std_dev
}

/// 最大ドローダウンを計算
#[allow(dead_code)]
pub fn calculate_max_drawdown(portfolio_values: &[f64]) -> f64 {
    if portfolio_values.len() < 2 {
        return 0.0;
    }

    let mut max_drawdown = 0.0;
    let mut peak = portfolio_values[0];

    for &value in portfolio_values.iter().skip(1) {
        if value > peak {
            peak = value;
        }

        let drawdown = (peak - value) / peak;
        if drawdown > max_drawdown {
            max_drawdown = drawdown;
        }
    }

    max_drawdown * 100.0 // パーセンテージで返す
}

// ==================== トレイト定義 ====================

/// 取引アルゴリズムの共通インターフェース
#[allow(dead_code)]
pub trait TradingAlgorithm {
    type Config;
    type Signal;

    /// アルゴリズムの初期化
    fn new(config: Self::Config) -> Self;

    /// 市場データから取引シグナルを生成
    fn generate_signal(&self, market_data: &MarketData) -> Result<Option<Self::Signal>>;

    /// アルゴリズムの名前を取得
    fn name(&self) -> &str;

    /// パフォーマンス指標を計算
    fn calculate_performance(&self, trades: &[TradeResult]) -> Result<PerformanceMetrics>;
}

/// パフォーマンス指標
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub total_return: f64,
    pub annualized_return: f64,
    pub max_drawdown: f64,
    pub sharpe_ratio: f64,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub total_trades: usize,
    pub winning_trades: usize,
    pub losing_trades: usize,
}

// ==================== テスト ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_price_change_percentage() {
        let old_price = BigDecimal::from(100);
        let new_price = BigDecimal::from(110);

        let change = calculate_price_change_percentage(&old_price, &new_price);
        assert!((change - 10.0).abs() < 0.001);

        // 下落のテスト
        let new_price = BigDecimal::from(90);
        let change = calculate_price_change_percentage(&old_price, &new_price);
        assert!((change - (-10.0)).abs() < 0.001);

        // ゼロ価格のテスト
        let zero_price = BigDecimal::from(0);
        let change = calculate_price_change_percentage(&zero_price, &new_price);
        assert_eq!(change, 0.0);
    }

    #[test]
    fn test_calculate_moving_average() {
        let prices = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let ma = calculate_moving_average(&prices, 3);

        assert_eq!(ma.len(), 4);
        assert_eq!(ma[0], 2.0); // (1+2+3)/3
        assert_eq!(ma[1], 3.0); // (2+3+4)/3
        assert_eq!(ma[2], 4.0); // (3+4+5)/3
        assert_eq!(ma[3], 5.0); // (4+5+6)/3

        // ウィンドウサイズが配列長より大きい場合
        let ma = calculate_moving_average(&prices, 10);
        assert!(ma.is_empty());
    }

    #[test]
    fn test_calculate_trading_cost() {
        let amount = BigDecimal::from(1000);
        let fee_rate = 0.003; // 0.3%

        let cost = calculate_trading_cost(&amount, fee_rate, None);
        assert_eq!(cost, BigDecimal::from(3)); // 1000 * 0.003

        // ガス料金込み
        let gas_cost = BigDecimal::from(1);
        let cost = calculate_trading_cost(&amount, fee_rate, Some(gas_cost));
        assert_eq!(cost, BigDecimal::from(4)); // 3 + 1
    }

    #[test]
    fn test_calculate_max_drawdown() {
        let values = vec![100.0, 110.0, 90.0, 120.0, 80.0, 150.0];
        let max_dd = calculate_max_drawdown(&values);

        // 120から80への下落が最大: (120-80)/120 = 33.33%
        assert!((max_dd - 33.333333333333336).abs() < 0.001);

        // 単調増加の場合
        let values = vec![100.0, 110.0, 120.0, 130.0];
        let max_dd = calculate_max_drawdown(&values);
        assert_eq!(max_dd, 0.0);

        // 空配列の場合
        let values = vec![];
        let max_dd = calculate_max_drawdown(&values);
        assert_eq!(max_dd, 0.0);
    }
}
