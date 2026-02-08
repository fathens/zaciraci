use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

use crate::types::{
    ExchangeRate, NearValue, TokenAmount, TokenInAccount, TokenOutAccount, TokenPrice,
};

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
    pub price: TokenPrice,
    pub volume: Option<BigDecimal>,
}

/// 価格履歴データ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceHistory {
    pub token: TokenOutAccount,
    pub quote_token: TokenInAccount,
    pub prices: Vec<PricePoint>,
}

// ==================== トークン情報 ====================

/// 統合トークン情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenData {
    /// トークン名
    pub symbol: TokenOutAccount,
    /// 現在の交換レート（tokens_smallest / NEAR + decimals）
    pub current_rate: ExchangeRate,
    pub historical_volatility: f64,
    pub liquidity_score: Option<f64>,
    /// 時価総額（NearValue: NEAR 単位）
    pub market_cap: Option<NearValue>,
}

/// トークン保有情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenHolding {
    /// トークン名
    pub token: TokenOutAccount,
    /// 保有量（TokenAmount: smallest_units + decimals）
    pub amount: TokenAmount,
    /// 現在の交換レート
    pub current_rate: ExchangeRate,
}

// ==================== 予測データ ====================

/// NEAR トークンの標準 decimals（wNEAR など）
pub const DEFAULT_DECIMALS: u8 = 24;

/// 予測価格
///
/// Chronos API から返される予測値は price 形式（NEAR/token）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictedPrice {
    pub timestamp: DateTime<Utc>,
    /// 予測価格（NEAR/token）
    pub price: TokenPrice,
    pub confidence: Option<BigDecimal>,
}

/// 予測データを格納する構造体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionData {
    pub token: TokenOutAccount,
    /// 現在の価格（NEAR/token）
    pub current_price: TokenPrice,
    /// 24時間後の予測価格（NEAR/token）
    pub predicted_price_24h: TokenPrice,
    pub timestamp: DateTime<Utc>,
    pub confidence: Option<BigDecimal>,
}

/// トークン予測結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPredictionResult {
    pub token: TokenOutAccount,
    pub quote_token: TokenInAccount,
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

// ==================== 取引判断パラメータ ====================

/// 取引判断のためのパラメータ
///
/// `make_trading_decision` に渡す設定パラメータをまとめた構造体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingDecisionParams {
    /// 最小利益閾値（無次元、例: 0.05 = 5%）
    pub min_profit_threshold: f64,
    /// スイッチ判定の乗数（無次元）
    pub switch_multiplier: f64,
    /// 最小取引価値（NearValue: NEAR 単位）
    pub min_trade_value: NearValue,
}

impl Default for TradingDecisionParams {
    fn default() -> Self {
        Self {
            min_profit_threshold: 0.05,
            switch_multiplier: 1.5,
            min_trade_value: NearValue::from_near(BigDecimal::from(1)),
        }
    }
}

// ==================== 取引アクション ====================

/// 統合取引アクション
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TradingAction {
    /// トークンを保持
    Hold,
    /// トークンを売却して別のトークンに切り替え
    Sell {
        token: TokenOutAccount,
        target: TokenOutAccount,
    },
    /// あるトークンから別のトークンへ切り替え
    Switch {
        from: TokenOutAccount,
        to: TokenOutAccount,
    },
    /// ポートフォリオリバランス
    Rebalance {
        target_weights: BTreeMap<TokenOutAccount, f64>,
    },
    /// ポジション追加
    AddPosition { token: TokenOutAccount, weight: f64 },
    /// ポジション削減
    ReducePosition { token: TokenOutAccount, weight: f64 },
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
    pub weights: BTreeMap<TokenOutAccount, f64>,
    pub timestamp: DateTime<Utc>,
    pub expected_return: f64,
    pub expected_volatility: f64,
    pub sharpe_ratio: f64,
}

/// ポートフォリオメトリクス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioMetrics {
    pub daily_return: f64,
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
    /// トークン保有量（TokenAmount: smallest_units + decimals）
    pub holdings: BTreeMap<TokenOutAccount, TokenAmount>,
    /// 総価値（NEAR単位、BigDecimal精度）
    pub total_value: NearValue,
    /// 現金残高（NEAR単位、BigDecimal精度）
    pub cash_balance: NearValue,
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
    pub token: TokenOutAccount,
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
    pub tokens: HashMap<TokenOutAccount, TokenData>,
    pub predictions: HashMap<TokenOutAccount, PredictionData>,
    pub price_histories: HashMap<TokenOutAccount, PriceHistory>,
    pub timestamp: DateTime<Utc>,
}

// ==================== トップトークン情報 ====================

/// トップトークン情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopTokenInfo {
    pub token: TokenOutAccount,
    pub volatility: BigDecimal,
}

#[cfg(test)]
mod tests;
