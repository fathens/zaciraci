use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

use crate::types::{Price, PriceF64, YoctoAmount};

// ==================== 取引関連型 ====================

/// 取引の種類
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TradeType {
    Buy,
    Sell,
    Swap,
}

// ==================== 共通価格データ ====================

/// 価格ポイント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricePoint {
    pub timestamp: DateTime<Utc>,
    pub price: Price,
    pub volume: Option<BigDecimal>,
}

/// 価格履歴データ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceHistory {
    pub token: String,
    pub quote_token: String,
    pub prices: Vec<PricePoint>,
}

// ==================== トークン情報 ====================

/// 統合トークン情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenData {
    pub symbol: String,
    pub current_price: Price,
    pub historical_volatility: f64,
    pub liquidity_score: Option<f64>,
    pub market_cap: Option<f64>,
    pub decimals: Option<u8>,
}

/// トークン保有情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenHolding {
    pub token: String,
    /// 保有量（トークンの最小単位）
    pub amount: YoctoAmount,
    pub current_price: Price,
}

// ==================== 予測データ ====================

/// 予測価格
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictedPrice {
    pub timestamp: DateTime<Utc>,
    pub price: Price,
    pub confidence: Option<BigDecimal>,
}

/// 予測データを格納する構造体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionData {
    pub token: String,
    pub current_price: Price,
    pub predicted_price_24h: Price,
    pub timestamp: DateTime<Utc>,
    pub confidence: Option<BigDecimal>,
}

/// トークン予測結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPredictionResult {
    pub token: String,
    pub quote_token: String,
    pub prediction_time: DateTime<Utc>,
    pub predictions: Vec<PredictedPrice>,
}

// ==================== 後方互換性のための型エイリアス ====================

/// PortfolioAction を TradingAction で統一
pub type PortfolioAction = TradingAction;

/// TrendTradingAction を TradingAction で統一  
pub type TrendTradingAction = TradingAction;

/// TokenInfo を TokenData で統一
pub type TokenInfo = TokenData;

// ==================== 取引アクション ====================

/// 統合取引アクション
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TradingAction {
    /// トークンを保持
    Hold,
    /// トークンを売却して別のトークンに切り替え
    Sell { token: String, target: String },
    /// あるトークンから別のトークンへ切り替え
    Switch { from: String, to: String },
    /// ポートフォリオリバランス
    Rebalance {
        target_weights: BTreeMap<String, f64>,
    },
    /// ポジション追加
    AddPosition { token: String, weight: f64 },
    /// ポジション削減
    ReducePosition { token: String, weight: f64 },
}

// ==================== アルゴリズム実行結果 ====================

/// アルゴリズムタイプ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlgorithmType {
    Momentum,
    Portfolio,
}

/// パフォーマンス指標
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub total_return: f64,
    pub sharpe_ratio: f64,
    pub max_drawdown: f64,
    pub win_rate: f64,
    pub total_trades: usize,
    pub annualized_return: Option<f64>,
    pub volatility: Option<f64>,
    pub sortino_ratio: Option<f64>,
    pub calmar_ratio: Option<f64>,
}

/// 統合実行レポート
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReport {
    pub actions: Vec<TradingAction>,
    pub timestamp: DateTime<Utc>,
    pub expected_return: Option<f64>,
    pub algorithm_type: AlgorithmType,
    pub performance_metrics: Option<PerformanceMetrics>,
    pub total_trades: usize,
    pub success_count: usize,
    pub failed_count: usize,
    pub skipped_count: usize,
}

impl ExecutionReport {
    pub fn new(actions: Vec<TradingAction>, algorithm_type: AlgorithmType) -> Self {
        let total_trades = actions.len();
        Self {
            actions,
            timestamp: Utc::now(),
            expected_return: None,
            algorithm_type,
            performance_metrics: None,
            total_trades,
            success_count: 0,
            failed_count: 0,
            skipped_count: 0,
        }
    }

    pub fn mark_success(&mut self) {
        self.success_count += 1;
    }

    pub fn mark_failed(&mut self) {
        self.failed_count += 1;
    }

    pub fn mark_skipped(&mut self) {
        self.skipped_count += 1;
    }
}

// ==================== ポートフォリオ関連 ====================

/// ポートフォリオの重み
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioWeights {
    pub weights: BTreeMap<String, f64>,
    pub timestamp: DateTime<Utc>,
    pub expected_return: f64,
    pub expected_volatility: f64,
    pub sharpe_ratio: f64,
}

/// ポートフォリオメトリクス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioMetrics {
    pub cumulative_return: f64,
    pub annualized_return: f64,
    pub volatility: f64,
    pub sharpe_ratio: f64,
    pub sortino_ratio: f64,
    pub max_drawdown: f64,
    pub calmar_ratio: f64,
    pub turnover_rate: f64,
}

/// ウォレット情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletInfo {
    pub holdings: BTreeMap<String, f64>,
    pub total_value: f64,
    pub cash_balance: f64,
}

// ==================== トレンド分析関連 ====================

/// トレンド方向
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TrendDirection {
    Upward,
    Downward,
    Sideways,
}

/// トレンド強度
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TrendStrength {
    Strong,
    Moderate,
    Weak,
    NoTrend,
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
    pub rsi: Option<f64>,
    pub adx: Option<f64>,
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
    pub bollinger_upper: Option<f64>,
    pub bollinger_lower: Option<f64>,
    pub stochastic_k: Option<f64>,
    pub stochastic_d: Option<f64>,
}

// ==================== 市場データ ====================

/// 統合市場データ
#[derive(Debug, Clone)]
pub struct MarketData {
    pub tokens: HashMap<String, TokenData>,
    pub predictions: HashMap<String, PredictionData>,
    pub price_histories: HashMap<String, PriceHistory>,
    pub timestamp: DateTime<Utc>,
}

// ==================== トップトークン情報 ====================

/// トップトークン情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopTokenInfo {
    pub token: String,
    pub volatility: f64,
    pub volume_24h: f64,
    pub current_price: PriceF64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    // ==================== TradingAction のテスト ====================

    #[test]
    fn test_trading_action_hold() {
        let action = TradingAction::Hold;
        assert_eq!(action, TradingAction::Hold);
    }

    #[test]
    fn test_trading_action_sell() {
        let action = TradingAction::Sell {
            token: "token1".to_string(),
            target: "token2".to_string(),
        };
        match action {
            TradingAction::Sell { token, target } => {
                assert_eq!(token, "token1");
                assert_eq!(target, "token2");
            }
            _ => panic!("Expected Sell action"),
        }
    }

    #[test]
    fn test_trading_action_rebalance() {
        let mut weights = BTreeMap::new();
        weights.insert("token1".to_string(), 0.5);
        weights.insert("token2".to_string(), 0.5);

        let action = TradingAction::Rebalance {
            target_weights: weights.clone(),
        };

        match action {
            TradingAction::Rebalance { target_weights } => {
                assert_eq!(target_weights.len(), 2);
                assert_eq!(target_weights.get("token1"), Some(&0.5));
            }
            _ => panic!("Expected Rebalance action"),
        }
    }

    #[test]
    fn test_trading_action_clone() {
        let action1 = TradingAction::Hold;
        let action2 = action1.clone();
        assert_eq!(action1, action2);
    }

    // ==================== ExecutionReport のテスト ====================

    #[test]
    fn test_execution_report_new() {
        let actions = vec![TradingAction::Hold];
        let report = ExecutionReport::new(actions.clone(), AlgorithmType::Momentum);

        assert_eq!(report.actions.len(), 1);
        assert_eq!(report.total_trades, 1);
        assert_eq!(report.success_count, 0);
        assert_eq!(report.failed_count, 0);
        assert_eq!(report.skipped_count, 0);
    }

    #[test]
    fn test_execution_report_mark_success() {
        let actions = vec![TradingAction::Hold];
        let mut report = ExecutionReport::new(actions, AlgorithmType::Portfolio);

        report.mark_success();
        assert_eq!(report.success_count, 1);
    }

    #[test]
    fn test_execution_report_mark_failed() {
        let actions = vec![];
        let mut report = ExecutionReport::new(actions, AlgorithmType::Momentum);

        report.mark_failed();
        assert_eq!(report.failed_count, 1);
    }

    #[test]
    fn test_execution_report_mark_skipped() {
        let actions = vec![];
        let mut report = ExecutionReport::new(actions, AlgorithmType::Portfolio);

        report.mark_skipped();
        assert_eq!(report.skipped_count, 1);
    }

    #[test]
    fn test_execution_report_multiple_marks() {
        let actions = vec![TradingAction::Hold, TradingAction::Hold];
        let mut report = ExecutionReport::new(actions, AlgorithmType::Momentum);

        report.mark_success();
        report.mark_success();
        report.mark_failed();
        report.mark_skipped();

        assert_eq!(report.success_count, 2);
        assert_eq!(report.failed_count, 1);
        assert_eq!(report.skipped_count, 1);
    }

    // ==================== PredictionData のテスト ====================

    #[test]
    fn test_prediction_data_creation() {
        let prediction = PredictionData {
            token: "test.tkn.near".to_string(),
            current_price: Price::new(BigDecimal::from_str("1.0").unwrap()),
            predicted_price_24h: Price::new(BigDecimal::from_str("1.2").unwrap()),
            timestamp: Utc::now(),
            confidence: Some(BigDecimal::from_str("0.85").unwrap()),
        };

        assert_eq!(prediction.token, "test.tkn.near");
        assert!(prediction.confidence.is_some());
    }

    #[test]
    fn test_prediction_data_without_confidence() {
        let prediction = PredictionData {
            token: "test.tkn.near".to_string(),
            current_price: Price::new(BigDecimal::from_str("1.0").unwrap()),
            predicted_price_24h: Price::new(BigDecimal::from_str("1.2").unwrap()),
            timestamp: Utc::now(),
            confidence: None,
        };

        assert!(prediction.confidence.is_none());
    }

    // ==================== TradeType のテスト ====================

    #[test]
    fn test_trade_type_equality() {
        assert_eq!(TradeType::Buy, TradeType::Buy);
        assert_ne!(TradeType::Buy, TradeType::Sell);
    }

    #[test]
    fn test_trade_type_clone() {
        let trade_type = TradeType::Swap;
        let cloned = trade_type.clone();
        assert_eq!(trade_type, cloned);
    }

    // ==================== TrendDirection のテスト ====================

    #[test]
    fn test_trend_direction_equality() {
        assert_eq!(TrendDirection::Upward, TrendDirection::Upward);
        assert_ne!(TrendDirection::Upward, TrendDirection::Downward);
    }

    #[test]
    fn test_trend_direction_clone() {
        let direction = TrendDirection::Sideways;
        let cloned = direction.clone();
        assert_eq!(direction, cloned);
    }

    // ==================== TrendStrength のテスト ====================

    #[test]
    fn test_trend_strength_values() {
        let strengths = [
            TrendStrength::Strong,
            TrendStrength::Moderate,
            TrendStrength::Weak,
            TrendStrength::NoTrend,
        ];

        assert_eq!(strengths.len(), 4);
    }

    // ==================== PerformanceMetrics のテスト ====================

    #[test]
    fn test_performance_metrics_creation() {
        let metrics = PerformanceMetrics {
            total_return: 0.15,
            sharpe_ratio: 1.5,
            max_drawdown: 0.1,
            win_rate: 0.6,
            total_trades: 100,
            annualized_return: Some(0.25),
            volatility: Some(0.2),
            sortino_ratio: Some(2.0),
            calmar_ratio: Some(1.5),
        };

        assert_eq!(metrics.total_return, 0.15);
        assert_eq!(metrics.total_trades, 100);
        assert!(metrics.annualized_return.is_some());
    }

    // ==================== TokenData のテスト ====================

    #[test]
    fn test_token_data_creation() {
        let token = TokenData {
            symbol: "NEAR".to_string(),
            current_price: Price::new(BigDecimal::from_str("5.0").unwrap()),
            historical_volatility: 0.3,
            liquidity_score: Some(0.8),
            market_cap: Some(1000000.0),
            decimals: Some(24),
        };

        assert_eq!(token.symbol, "NEAR");
        assert_eq!(token.decimals, Some(24));
    }

    // ==================== シリアライゼーションのテスト ====================

    #[test]
    fn test_trading_action_serialization() {
        let action = TradingAction::Hold;
        let serialized = serde_json::to_string(&action).unwrap();
        let deserialized: TradingAction = serde_json::from_str(&serialized).unwrap();
        assert_eq!(action, deserialized);
    }

    #[test]
    fn test_prediction_data_serialization() {
        let prediction = PredictionData {
            token: "test".to_string(),
            current_price: Price::new(BigDecimal::from_str("1.0").unwrap()),
            predicted_price_24h: Price::new(BigDecimal::from_str("1.2").unwrap()),
            timestamp: Utc::now(),
            confidence: Some(BigDecimal::from_str("0.9").unwrap()),
        };

        let serialized = serde_json::to_string(&prediction).unwrap();
        let deserialized: PredictionData = serde_json::from_str(&serialized).unwrap();
        assert_eq!(prediction.token, deserialized.token);
    }

    // ==================== PricePoint のテスト ====================

    #[test]
    fn test_price_point_creation() {
        let price_point = PricePoint {
            timestamp: Utc::now(),
            price: Price::new(BigDecimal::from_str("123.456").unwrap()),
            volume: Some(BigDecimal::from(1000)),
        };

        assert_eq!(
            price_point.price,
            Price::new(BigDecimal::from_str("123.456").unwrap())
        );
        assert!(price_point.volume.is_some());
    }

    #[test]
    fn test_price_point_serialization() {
        let price_point = PricePoint {
            timestamp: Utc::now(),
            price: Price::new(BigDecimal::from_str("999.123456789").unwrap()),
            volume: Some(BigDecimal::from(5000)),
        };

        let serialized = serde_json::to_string(&price_point).unwrap();
        let deserialized: PricePoint = serde_json::from_str(&serialized).unwrap();

        assert_eq!(price_point.price, deserialized.price);
        assert_eq!(price_point.volume, deserialized.volume);
    }

    #[test]
    fn test_price_point_serialization_without_volume() {
        let price_point = PricePoint {
            timestamp: Utc::now(),
            price: Price::new(BigDecimal::from(100)),
            volume: None,
        };

        let serialized = serde_json::to_string(&price_point).unwrap();
        let deserialized: PricePoint = serde_json::from_str(&serialized).unwrap();

        assert_eq!(price_point.price, deserialized.price);
        assert!(deserialized.volume.is_none());
    }

    // ==================== TokenData のシリアライゼーションテスト ====================

    #[test]
    fn test_token_data_serialization() {
        let token = TokenData {
            symbol: "NEAR".to_string(),
            current_price: Price::new(BigDecimal::from_str("5.123").unwrap()),
            historical_volatility: 0.3,
            liquidity_score: Some(0.8),
            market_cap: Some(1000000.0),
            decimals: Some(24),
        };

        let serialized = serde_json::to_string(&token).unwrap();
        let deserialized: TokenData = serde_json::from_str(&serialized).unwrap();

        assert_eq!(token.symbol, deserialized.symbol);
        assert_eq!(token.current_price, deserialized.current_price);
        assert_eq!(
            token.historical_volatility,
            deserialized.historical_volatility
        );
    }

    // ==================== PriceHistory のテスト ====================

    #[test]
    fn test_price_history_serialization() {
        let history = PriceHistory {
            token: "test.token".to_string(),
            quote_token: "wrap.near".to_string(),
            prices: vec![
                PricePoint {
                    timestamp: Utc::now(),
                    price: Price::new(BigDecimal::from(100)),
                    volume: Some(BigDecimal::from(1000)),
                },
                PricePoint {
                    timestamp: Utc::now(),
                    price: Price::new(BigDecimal::from(110)),
                    volume: Some(BigDecimal::from(2000)),
                },
            ],
        };

        let serialized = serde_json::to_string(&history).unwrap();
        let deserialized: PriceHistory = serde_json::from_str(&serialized).unwrap();

        assert_eq!(history.token, deserialized.token);
        assert_eq!(history.prices.len(), deserialized.prices.len());
        assert_eq!(history.prices[0].price, deserialized.prices[0].price);
    }

    // ==================== Price 型のJSON形式テスト ====================

    #[test]
    fn test_price_json_format() {
        // Price 型が正しくJSONにシリアライズされることを確認
        let price = Price::new(BigDecimal::from_str("123.456789").unwrap());
        let json = serde_json::to_string(&price).unwrap();

        // BigDecimal単体のシリアライズ形式と比較
        let bd = BigDecimal::from_str("123.456789").unwrap();
        let bd_json = serde_json::to_string(&bd).unwrap();

        // Price と BigDecimal は同じJSON形式でシリアライズされることを確認
        assert_eq!(json, bd_json);

        // デシリアライズの往復確認
        let deserialized: Price = serde_json::from_str(&json).unwrap();
        assert_eq!(price, deserialized);
    }

    #[test]
    fn test_price_comparison_in_structures() {
        // Price 型を含む構造体の比較が正しく動作することを確認
        let token1 = TokenData {
            symbol: "TEST".to_string(),
            current_price: Price::new(BigDecimal::from(100)),
            historical_volatility: 0.2,
            liquidity_score: None,
            market_cap: None,
            decimals: None,
        };

        let token2 = TokenData {
            symbol: "TEST".to_string(),
            current_price: Price::new(BigDecimal::from(100)),
            historical_volatility: 0.2,
            liquidity_score: None,
            market_cap: None,
            decimals: None,
        };

        let token3 = TokenData {
            symbol: "TEST".to_string(),
            current_price: Price::new(BigDecimal::from(200)), // 異なる価格
            historical_volatility: 0.2,
            liquidity_score: None,
            market_cap: None,
            decimals: None,
        };

        assert_eq!(token1.current_price, token2.current_price);
        assert_ne!(token1.current_price, token3.current_price);
    }
}
