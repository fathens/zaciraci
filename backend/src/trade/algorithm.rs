pub mod momentum;
pub mod portfolio;
pub mod trend_following;

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

// 共通ユーティリティ関数はcommonクレートを使用
// Use common crate functions for price calculations, moving averages, RSI, etc.
// Re-export commonly used functions from common crate for convenience

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

    // Tests for deleted functions moved to common crate tests
    // Use zaciraci-common for price calculations, moving averages, RSI, etc.

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
