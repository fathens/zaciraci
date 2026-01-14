use anyhow::Result;
use bigdecimal::BigDecimal;
use chrono::{DateTime, Duration, Utc};
use clap::Args;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

// =============================================================================
// 型安全な単位型（common::types からの re-export）
// =============================================================================
//
// シミュレーションコードでは以下の型を使用する：
// - TokenPriceF64: 価格（NEAR/token）
// - TokenAmountF64: トークン数量
// - YoctoValueF64: 金額（yoctoNEAR）- 内部計算用
// - NearValueF64: 金額（NEAR）- 表示・保存用
//
// 演算:
// - TokenAmountF64 × TokenPriceF64 = YoctoValueF64
// - YoctoValueF64.to_near() → NearValueF64

pub use common::types::{
    NearValue, NearValueF64, TokenAmountF64, TokenPriceF64, YoctoAmount, YoctoValueF64,
};

// Trading related structures
#[derive(Debug, Clone, PartialEq)]
pub enum TradingDecision {
    Hold,
    Sell { target_token: String },
    Switch { from: String, to: String },
}

#[derive(Debug, Clone)]
pub struct TradingConfig {
    pub min_profit_threshold: f64,
    pub switch_multiplier: f64,
    pub min_trade_amount: f64,
}

#[derive(Debug, Clone)]
pub struct TokenOpportunity {
    pub token: String,
    pub expected_return: f64,
    pub confidence: Option<f64>,
}

// Immutable data structures for better functional programming
#[derive(Debug, Clone, PartialEq)]
pub struct ImmutablePortfolio {
    /// トークン別保有量（smallest_unit）
    pub holdings: HashMap<String, TokenAmountF64>,
    /// 現金残高（NEAR）
    pub cash_balance: NearValueF64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct MarketSnapshot {
    /// 価格マップ（yoctoNEAR/smallest_unit = NEAR/token）
    pub prices: HashMap<String, TokenPriceF64>,
    pub timestamp: DateTime<Utc>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DataQuality {
    High,   // All tokens have fresh data (< 1 hour)
    Medium, // Most tokens have acceptable data
    Low,    // Some tokens have stale data
    Poor,   // Significant data quality issues
}

#[derive(Debug, Clone)]
pub struct PortfolioTransition {
    pub from: ImmutablePortfolio,
    pub to: ImmutablePortfolio,
    pub action: TradingDecision,
    /// 取引コスト（yoctoNEAR）
    pub cost: YoctoValueF64,
    pub reason: String,
}

// Strategy Pattern for different trading algorithms
pub trait TradingStrategy {
    fn name(&self) -> &'static str;
    fn make_decision(
        &self,
        portfolio: &ImmutablePortfolio,
        market: &MarketSnapshot,
        opportunities: &[TokenOpportunity],
        config: &TradingConfig,
    ) -> Result<TradingDecision>;
    fn should_rebalance(&self, portfolio: &ImmutablePortfolio, market: &MarketSnapshot) -> bool;
}

#[derive(Debug, Clone)]
pub struct MomentumStrategy {
    pub min_confidence: f64,
    pub lookback_periods: usize,
}

#[derive(Debug, Clone)]
pub struct PortfolioStrategy {
    pub max_positions: usize,
    pub rebalance_threshold: f64,
}

#[derive(Debug, Clone)]
pub struct TrendFollowingStrategy {
    pub trend_window: usize,
    pub volatility_threshold: f64,
}

pub struct StrategyContext {
    pub strategy: Box<dyn TradingStrategy>,
}

impl fmt::Debug for StrategyContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StrategyContext")
            .field("strategy", &self.strategy.name())
            .finish()
    }
}

// Constants
pub const MIN_PROFIT_THRESHOLD: f64 = 0.05;
pub const SWITCH_MULTIPLIER: f64 = 1.5;

// CLI Arguments
#[derive(Args)]
pub struct SimulateArgs {
    /// シミュレーション開始日 (YYYY-MM-DD)
    /// (注意: topコマンドで生成したトークンファイルが必要です)
    #[clap(short, long)]
    pub start: Option<String>,

    /// シミュレーション終了日 (YYYY-MM-DD)
    #[clap(short, long)]
    pub end: Option<String>,

    /// 初期資金 (NEAR) [デフォルト: 1000.0]
    #[clap(short, long, default_value = "1000.0")]
    pub capital: f64,

    /// ベース通貨 [デフォルト: wrap.near]
    /// (注意: topコマンドで作成されたtokens/ディレクトリからトークンを読み取ります)
    #[clap(short, long, default_value = "wrap.near")]
    pub quote_token: String,

    /// 出力ディレクトリ [デフォルト: simulation_results/]
    #[clap(short, long, default_value = "simulation_results")]
    pub output: String,

    /// リバランス間隔 (例: 2h, 90m, 1h30m, 4h) [デフォルト: 1d]
    #[clap(long, default_value = "1d")]
    pub rebalance_interval: String,

    /// 手数料モデル [デフォルト: realistic]
    #[clap(long, default_value = "realistic")]
    pub fee_model: String,

    /// カスタム手数料率 (0.0-1.0)
    #[clap(long)]
    pub custom_fee: Option<f64>,

    /// スリッページ率 (0.0-1.0) [デフォルト: 0.01]
    #[clap(long, default_value = "0.01")]
    pub slippage: f64,

    /// ガス料金 (NEAR) [デフォルト: 0.01]
    #[clap(long, default_value = "0.01")]
    pub gas_cost: f64,

    /// 最小取引額 (NEAR) [デフォルト: 1.0]
    #[clap(long, default_value = "1.0")]
    pub min_trade: f64,

    /// 予測期間 (時間) [デフォルト: 24]
    #[clap(long, default_value = "24")]
    pub prediction_horizon: u64,

    /// 予測に使用する過去データ期間 (日数) [デフォルト: 30]
    #[clap(long, default_value = "30")]
    pub historical_days: u64,

    /// チャートを生成
    #[clap(long)]
    pub chart: bool,

    /// 詳細ログ
    #[clap(short, long)]
    pub verbose: bool,

    /// 予測モデル (未指定の場合はサーバーのデフォルトモデルを使用)
    #[clap(long)]
    pub model: Option<String>,

    /// Portfolioアルゴリズムのリバランス閾値 (0.0-1.0) [デフォルト: 0.05]
    #[clap(long, default_value = "0.05")]
    pub portfolio_rebalance_threshold: f64,

    /// Portfolioアルゴリズムのリバランス間隔 (例: 2h, 90m, 1d) [デフォルト: 1d]
    #[clap(long, default_value = "1d")]
    pub portfolio_rebalance_interval: String,

    /// Momentumアルゴリズムの最低利益率閾値 (0.0-1.0) [デフォルト: 0.01]
    #[clap(long, default_value = "0.01")]
    pub momentum_min_profit_threshold: f64,

    /// Momentumアルゴリズムの切り替え倍率 [デフォルト: 1.2]
    #[clap(long, default_value = "1.2")]
    pub momentum_switch_multiplier: f64,

    /// Momentumアルゴリズムの最小取引額 (NEAR) [デフォルト: 0.1]
    #[clap(long, default_value = "0.1")]
    pub momentum_min_trade_amount: f64,

    /// TrendFollowingアルゴリズムのRSI買われすぎ閾値 [デフォルト: 80.0]
    #[clap(long, default_value = "80.0")]
    pub trend_rsi_overbought: f64,

    /// TrendFollowingアルゴリズムのRSI売られすぎ閾値 [デフォルト: 20.0]
    #[clap(long, default_value = "20.0")]
    pub trend_rsi_oversold: f64,

    /// TrendFollowingアルゴリズムのADX強トレンド閾値 [デフォルト: 20.0]
    #[clap(long, default_value = "20.0")]
    pub trend_adx_strong_threshold: f64,

    /// TrendFollowingアルゴリズムのR²閾値 [デフォルト: 0.5]
    #[clap(long, default_value = "0.5")]
    pub trend_r_squared_threshold: f64,
}

// Configuration structures
#[derive(Debug, Clone)]
pub struct SimulationConfig {
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
    pub algorithm: AlgorithmType,
    pub initial_capital: BigDecimal,
    pub quote_token: String,
    pub target_tokens: Vec<String>,
    pub rebalance_interval: RebalanceInterval,
    pub fee_model: FeeModel,
    pub slippage_rate: f64,
    pub gas_cost: BigDecimal,
    pub min_trade_amount: BigDecimal,
    pub prediction_horizon: chrono::Duration,
    pub historical_days: i64,               // 予測に使用する過去データ期間
    pub model: Option<String>,              // 予測モデル
    pub verbose: bool,                      // 詳細出力フラグ
    pub portfolio_rebalance_threshold: f64, // Portfolioアルゴリズムのリバランス閾値
    pub portfolio_rebalance_interval: RebalanceInterval, // Portfolioアルゴリズムのリバランス間隔
    pub momentum_min_profit_threshold: f64, // Momentumアルゴリズムの最低利益率閾値
    pub momentum_switch_multiplier: f64,    // Momentumアルゴリズムの切り替え倍率
    pub momentum_min_trade_amount: f64,     // Momentumアルゴリズムの最小取引額
    pub trend_rsi_overbought: f64,          // TrendFollowingアルゴリズムのRSI買われすぎ閾値
    pub trend_rsi_oversold: f64,            // TrendFollowingアルゴリズムのRSI売られすぎ閾値
    pub trend_adx_strong_threshold: f64,    // TrendFollowingアルゴリズムのADX強トレンド閾値
    pub trend_r_squared_threshold: f64,     // TrendFollowingアルゴリズムのR²閾値
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AlgorithmType {
    Momentum,
    Portfolio,
}

#[derive(Debug, Clone)]
pub struct RebalanceInterval {
    duration: Duration,
}

impl RebalanceInterval {
    pub fn parse(s: &str) -> Result<Self> {
        let input = s.trim();
        let mut total_seconds = 0i64;
        let mut current_number = String::new();
        let mut i = 0;
        let chars: Vec<char> = input.chars().collect();

        while i < chars.len() {
            let ch = chars[i];
            if ch.is_ascii_digit() {
                current_number.push(ch);
            } else if ch.is_ascii_alphabetic() {
                if current_number.is_empty() {
                    return Err(anyhow::anyhow!("Invalid interval format: {}", s));
                }

                let value: i64 = current_number.parse().map_err(|_| {
                    anyhow::anyhow!("Invalid number in interval: {}", current_number)
                })?;
                current_number.clear();

                // Read the unit (could be multiple chars like 'min')
                let mut unit = String::new();
                while i < chars.len() && chars[i].is_ascii_alphabetic() {
                    unit.push(chars[i]);
                    i += 1;
                }
                i -= 1; // Adjust because the loop will increment again

                let multiplier = match unit.as_str() {
                    "s" | "sec" | "second" | "seconds" => 1,
                    "m" | "min" | "minute" | "minutes" => 60,
                    "h" | "hr" | "hour" | "hours" => 3600,
                    "d" | "day" | "days" => 86400,
                    "w" | "week" | "weeks" => 604800,
                    _ => return Err(anyhow::anyhow!("Unknown time unit: {}", unit)),
                };

                total_seconds += value * multiplier;
            } else if !ch.is_whitespace() {
                return Err(anyhow::anyhow!("Invalid character in interval: {}", ch));
            }
            i += 1;
        }

        // Handle case where input ends with a number (invalid)
        if !current_number.is_empty() {
            return Err(anyhow::anyhow!("Interval must specify a unit: {}", s));
        }

        if total_seconds <= 0 {
            return Err(anyhow::anyhow!("Interval must be positive: {}", s));
        }

        Ok(RebalanceInterval {
            duration: Duration::seconds(total_seconds),
        })
    }

    pub fn as_duration(&self) -> Duration {
        self.duration
    }
}

impl fmt::Display for RebalanceInterval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut seconds = self.duration.num_seconds();
        let mut parts = Vec::new();

        // Weeks
        if seconds >= 604800 {
            parts.push(format!("{}w", seconds / 604800));
            seconds %= 604800;
        }

        // Days
        if seconds >= 86400 {
            parts.push(format!("{}d", seconds / 86400));
            seconds %= 86400;
        }

        // Hours
        if seconds >= 3600 {
            parts.push(format!("{}h", seconds / 3600));
            seconds %= 3600;
        }

        // Minutes
        if seconds >= 60 {
            parts.push(format!("{}m", seconds / 60));
            seconds %= 60;
        }

        // Seconds
        if seconds > 0 {
            parts.push(format!("{}s", seconds));
        }

        write!(f, "{}", parts.join(""))
    }
}

impl std::str::FromStr for RebalanceInterval {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

impl MarketSnapshot {
    /// Create a new MarketSnapshot from price data
    ///
    /// # Arguments
    /// * `prices` - 価格マップ（無次元比率）
    pub fn new(prices: HashMap<String, TokenPriceF64>) -> Self {
        let data_quality = if prices.len() >= 2 {
            DataQuality::High
        } else if prices.len() == 1 {
            DataQuality::Low
        } else {
            DataQuality::Poor
        };

        Self {
            prices,
            timestamp: Utc::now(),
            data_quality,
        }
    }

    /// Check if the market snapshot has reliable data
    pub fn is_reliable(&self) -> bool {
        matches!(self.data_quality, DataQuality::High | DataQuality::Medium)
    }

    /// Get price for a specific token（無次元比率）
    pub fn get_price(&self, token: &str) -> Option<TokenPriceF64> {
        self.prices.get(token).copied()
    }

    /// Create MarketSnapshot from price data at a specific time
    ///
    /// 返される価格は無次元比率（yoctoNEAR/smallest_unit = NEAR/token）
    pub fn from_price_data(
        price_data: &HashMap<String, Vec<common::stats::ValueAtTime>>,
        timestamp: DateTime<Utc>,
    ) -> Result<Self> {
        let mut prices: HashMap<String, TokenPriceF64> = HashMap::new();

        for (token, data_points) in price_data {
            // Find the closest price point to the target timestamp
            if let Some(closest_point) = data_points.iter().min_by_key(|point| {
                (DateTime::<Utc>::from_naive_utc_and_offset(point.time, Utc) - timestamp).abs()
            }) {
                // 価格は無次元比率として取得
                let price_value = closest_point
                    .value
                    .to_string()
                    .parse::<f64>()
                    .unwrap_or(0.0);
                prices.insert(
                    token.clone(),
                    TokenPriceF64::from_near_per_token(price_value),
                );
            }
        }

        let data_quality = if prices.len() >= 2 {
            DataQuality::High
        } else if prices.len() == 1 {
            DataQuality::Low
        } else {
            DataQuality::Poor
        };

        Ok(Self {
            prices,
            timestamp,
            data_quality,
        })
    }
}

#[derive(Debug, Clone)]
pub enum FeeModel {
    Realistic,
    Zero,
    Custom(f64),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingCost {
    pub protocol_fee: BigDecimal,
    pub slippage: BigDecimal,
    pub gas_fee: BigDecimal,
    pub total: BigDecimal,
}

// Trading context struct to reduce function arguments
#[derive(Debug)]
pub struct TradingContext<'a> {
    pub price_data: &'a HashMap<String, Vec<common::stats::ValueAtTime>>,
    pub current_date: DateTime<Utc>,
    pub fee_model: &'a FeeModel,
    pub slippage_rate: f64,
    pub gas_cost: &'a BigDecimal,
}

// Portfolio state struct to manage mutable state
#[derive(Debug)]
pub struct PortfolioState<'a> {
    pub holdings: &'a mut HashMap<String, BigDecimal>,
    pub cash_balance: &'a mut BigDecimal,
    pub total_cost: &'a mut BigDecimal,
    pub trades: &'a mut Vec<TradeExecution>,
}

/// データ欠損によるスキップイベントの記録
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataGapEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: DataGapEventType,
    pub affected_tokens: Vec<String>,
    pub reason: String,
    pub impact: DataGapImpact,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DataGapEventType {
    TradingSkipped,   // 取引をスキップ
    RebalanceSkipped, // リバランスをスキップ
    PriceDataMissing, // 価格データが見つからない
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataGapImpact {
    pub duration_hours: i64,                         // データ欠損期間（時間）
    pub last_known_timestamp: DateTime<Utc>,         // 最後に取得できた時刻
    pub next_known_timestamp: Option<DateTime<Utc>>, // 次に取得できた時刻
}

/// データ品質に関する統計情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataQualityStats {
    pub total_timesteps: usize,        // 総タイムステップ数
    pub skipped_timesteps: usize,      // スキップしたタイムステップ数
    pub data_coverage_percentage: f64, // データカバレッジ率
    pub longest_gap_hours: i64,        // 最長データギャップ（時間）
    pub gap_events: Vec<DataGapEvent>, // データギャップイベント一覧
}

// Result structures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub config: SimulationSummary,
    pub performance: PerformanceMetrics,
    pub trades: Vec<TradeExecution>,
    pub portfolio_values: Vec<PortfolioValue>,
    pub execution_summary: ExecutionSummary,
    pub data_quality: DataQualityStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiAlgorithmSimulationResult {
    pub results: Vec<SimulationResult>,
    pub comparison: AlgorithmComparison,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlgorithmComparison {
    pub best_return: (AlgorithmType, f64),
    pub best_sharpe: (AlgorithmType, f64),
    pub lowest_drawdown: (AlgorithmType, f64),
    pub summary_table: Vec<AlgorithmSummaryRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlgorithmSummaryRow {
    pub algorithm: AlgorithmType,
    pub total_return_pct: f64,
    pub annualized_return: f64,
    pub sharpe_ratio: f64,
    pub max_drawdown_pct: f64,
    pub total_trades: usize,
    pub win_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationSummary {
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
    pub algorithm: AlgorithmType,
    /// 初期資金（NEAR）
    pub initial_capital: NearValueF64,
    /// 最終価値（NEAR）
    pub final_value: NearValueF64,
    /// 総リターン（NEAR）
    pub total_return: NearValueF64,
    pub duration_days: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// 総リターン（NEAR）
    pub total_return: NearValueF64,
    /// 年率リターン（比率）
    pub annualized_return: f64,
    /// 総リターン率（%）
    pub total_return_pct: f64,
    /// ボラティリティ（比率）
    pub volatility: f64,
    /// 最大ドローダウン（NEAR）
    pub max_drawdown: NearValueF64,
    /// 最大ドローダウン率（%）
    pub max_drawdown_pct: f64,
    /// シャープレシオ
    pub sharpe_ratio: f64,
    /// ソルティノレシオ
    pub sortino_ratio: f64,
    pub total_trades: usize,
    pub winning_trades: usize,
    pub losing_trades: usize,
    /// 勝率（比率）
    pub win_rate: f64,
    /// プロフィットファクター（比率）
    pub profit_factor: f64,
    /// 総コスト（NEAR）
    pub total_costs: NearValueF64,
    /// コスト比率（%）
    pub cost_ratio: f64,
    pub simulation_days: i64,
    pub active_trading_days: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeExecution {
    pub timestamp: DateTime<Utc>,
    pub from_token: String,
    pub to_token: String,
    /// 取引数量（smallest_unit）
    pub amount: TokenAmountF64,
    /// 約定価格（無次元比率）
    pub executed_price: TokenPriceF64,
    pub cost: TradingCost,
    /// 取引前のポートフォリオ価値（NEAR）
    pub portfolio_value_before: NearValueF64,
    /// 取引後のポートフォリオ価値（NEAR）
    pub portfolio_value_after: NearValueF64,
    pub success: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioValue {
    pub timestamp: DateTime<Utc>,
    /// 総ポートフォリオ価値（NEAR）
    pub total_value: NearValueF64,
    /// トークン別保有価値（NEAR）
    pub holdings: HashMap<String, NearValueF64>,
    /// 現金残高（NEAR）
    pub cash_balance: NearValueF64,
    /// 未実現損益（NEAR）
    pub unrealized_pnl: NearValueF64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSummary {
    pub total_trades: usize,
    pub successful_trades: usize,
    pub failed_trades: usize,
    /// 成功率（比率）
    pub success_rate: f64,
    /// 総コスト（NEAR）
    pub total_cost: NearValueF64,
    /// 取引あたりの平均コスト（NEAR）
    pub avg_cost_per_trade: NearValueF64,
}

// Trait implementations
impl From<&str> for AlgorithmType {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "momentum" => AlgorithmType::Momentum,
            "portfolio" => AlgorithmType::Portfolio,
            _ => AlgorithmType::Portfolio, // デフォルト
        }
    }
}

impl From<(&str, Option<f64>)> for FeeModel {
    fn from((model, custom_rate): (&str, Option<f64>)) -> Self {
        match model.to_lowercase().as_str() {
            "zero" => FeeModel::Zero,
            "custom" => FeeModel::Custom(custom_rate.unwrap_or(0.003)),
            _ => FeeModel::Realistic, // デフォルト
        }
    }
}
