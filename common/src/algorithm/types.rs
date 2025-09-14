use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

// ==================== 取引関連型 ====================

/// 取引の種類
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TradeType {
    Buy,
    Sell,
    Swap,
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

// ==================== 共通価格データ ====================

/// 価格ポイント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricePoint {
    pub timestamp: DateTime<Utc>,
    pub price: BigDecimal,
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
    pub current_price: BigDecimal,
    pub historical_volatility: f64,
    pub liquidity_score: Option<f64>,
    pub market_cap: Option<f64>,
    pub decimals: Option<u8>,
}

/// トークン保有情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenHolding {
    pub token: String,
    pub amount: BigDecimal,
    pub current_price: BigDecimal,
}

// ==================== 予測データ ====================

/// 予測価格
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictedPrice {
    pub timestamp: DateTime<Utc>,
    pub price: BigDecimal,
    pub confidence: Option<f64>,
}

/// 予測データを格納する構造体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionData {
    pub token: String,
    pub current_price: BigDecimal,
    pub predicted_price_24h: BigDecimal,
    pub timestamp: DateTime<Utc>,
    pub confidence: Option<f64>,
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
    TrendFollowing,
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
    pub current_price: f64,
}
