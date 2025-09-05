use crate::api::backend::BackendClient;
use anyhow::Result;
use bigdecimal::BigDecimal;
use bigdecimal::FromPrimitive;
use chrono::{DateTime, Datelike, Duration, Utc};
use clap::Args;
use common::algorithm::momentum::{
    calculate_confidence_adjusted_return, rank_tokens_by_momentum, PredictionData, TradingAction,
};
// Portfolio and trend_following algorithms are available but not yet used in cli_tokens
// use common::algorithm::portfolio;
// use common::algorithm::trend_following;
use common::api::chronos::ChronosApiClient;
use common::api::traits::PredictionClient;
use common::prediction::ZeroShotPredictionRequest;
use common::stats::ValueAtTime;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

// Momentum アルゴリズム関連の構造体と定数
// PredictionData and TradingAction are now imported from common::algorithm::momentum

// New refactored data structures for better testability
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

// Phase 2: Immutable data structures for better functional programming
#[derive(Debug, Clone, PartialEq)]
pub struct ImmutablePortfolio {
    pub holdings: HashMap<String, f64>,
    pub cash_balance: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct MarketSnapshot {
    pub prices: HashMap<String, f64>,
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
    pub cost: f64,
    pub reason: String,
}

// Phase 3: Strategy Pattern for different trading algorithms
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
    strategy: Box<dyn TradingStrategy>,
}

impl fmt::Debug for StrategyContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StrategyContext")
            .field("strategy", &self.strategy.name())
            .finish()
    }
}

const MIN_PROFIT_THRESHOLD: f64 = 0.05;
const SWITCH_MULTIPLIER: f64 = 1.5;

#[derive(Args)]
pub struct SimulateArgs {
    /// シミュレーション開始日 (YYYY-MM-DD)
    #[clap(short, long)]
    pub start: Option<String>,

    /// シミュレーション終了日 (YYYY-MM-DD)
    #[clap(short, long)]
    pub end: Option<String>,

    /// 使用するアルゴリズム (未指定の場合は全アルゴリズムを実行)
    #[clap(short, long)]
    pub algorithm: Option<String>,

    /// 初期資金 (NEAR) [デフォルト: 1000.0]
    #[clap(short, long, default_value = "1000.0")]
    pub capital: f64,

    /// ベース通貨 [デフォルト: wrap.near]
    #[clap(short, long, default_value = "wrap.near")]
    pub quote_token: String,

    /// 対象トークンリスト (カンマ区切り)
    #[clap(short, long)]
    pub tokens: Option<String>,

    /// 自動取得する際のトークン数 [デフォルト: 10]
    #[clap(short, long, default_value = "10")]
    pub num_tokens: usize,

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
}

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
    pub historical_days: i64, // 予測に使用する過去データ期間
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AlgorithmType {
    Momentum,
    Portfolio,
    TrendFollowing,
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
    pub price_data: &'a HashMap<String, Vec<ValueAtTime>>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub config: SimulationSummary,
    pub performance: PerformanceMetrics,
    pub trades: Vec<TradeExecution>,
    pub portfolio_values: Vec<PortfolioValue>,
    pub execution_summary: ExecutionSummary,
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
    pub initial_capital: f64,
    pub final_value: f64,
    pub total_return: f64,
    pub duration_days: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub total_return: f64,
    pub annualized_return: f64,
    pub total_return_pct: f64,
    pub volatility: f64,
    pub max_drawdown: f64,
    pub max_drawdown_pct: f64,
    pub sharpe_ratio: f64,
    pub sortino_ratio: f64,
    pub total_trades: usize,
    pub winning_trades: usize,
    pub losing_trades: usize,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub total_costs: f64,
    pub cost_ratio: f64,
    pub simulation_days: i64,
    pub active_trading_days: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeExecution {
    pub timestamp: DateTime<Utc>,
    pub from_token: String,
    pub to_token: String,
    pub amount: f64,
    pub executed_price: f64,
    pub cost: TradingCost,
    pub portfolio_value_before: f64,
    pub portfolio_value_after: f64,
    pub success: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioValue {
    pub timestamp: DateTime<Utc>,
    pub total_value: f64,
    pub holdings: HashMap<String, f64>,
    pub cash_balance: f64,
    pub unrealized_pnl: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSummary {
    pub total_trades: usize,
    pub successful_trades: usize,
    pub failed_trades: usize,
    pub success_rate: f64,
    pub total_cost: f64,
    pub avg_cost_per_trade: f64,
}

impl From<&str> for AlgorithmType {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "momentum" => AlgorithmType::Momentum,
            "portfolio" => AlgorithmType::Portfolio,
            "trend_following" | "trend-following" => AlgorithmType::TrendFollowing,
            _ => AlgorithmType::Momentum, // デフォルト
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

pub async fn run(args: SimulateArgs) -> Result<()> {
    println!("🚀 Starting trading simulation...");

    let run_all_algorithms = args.algorithm.is_none();

    if args.verbose {
        println!("📋 Configuration:");
        if run_all_algorithms {
            println!("  Algorithm: All algorithms (Momentum, Portfolio, TrendFollowing)");
        } else {
            println!("  Algorithm: {:?}", args.algorithm);
        }
        println!("  Capital: {} {}", args.capital, args.quote_token);
        println!("  Fee Model: {}", args.fee_model);
        println!("  Output: {}", args.output);
    }

    // outputを先に保存
    let output_dir = args.output.clone();

    // 1. 設定の検証と変換
    let config = validate_and_convert_args(args).await?;

    if config.target_tokens.is_empty() {
        return Err(anyhow::anyhow!("No target tokens specified"));
    }

    println!(
        "📊 Simulation period: {} to {}",
        config.start_date.format("%Y-%m-%d"),
        config.end_date.format("%Y-%m-%d")
    );
    println!("🎯 Target tokens: {:?}", config.target_tokens);

    if run_all_algorithms {
        // 全アルゴリズムを実行
        println!("🔄 Running all algorithms...");
        let mut results = Vec::new();

        let algorithms = vec![
            AlgorithmType::Momentum,
            AlgorithmType::Portfolio,
            AlgorithmType::TrendFollowing,
        ];

        for algorithm in algorithms {
            println!("📈 Running {:?} algorithm...", algorithm);
            let mut algo_config = config.clone();
            algo_config.algorithm = algorithm.clone();

            let result = match algorithm {
                AlgorithmType::Momentum => run_momentum_simulation(&algo_config).await?,
                AlgorithmType::Portfolio => run_portfolio_simulation(&algo_config).await?,
                AlgorithmType::TrendFollowing => {
                    run_trend_following_simulation(&algo_config).await?
                }
            };

            results.push(result);
        }

        // 比較データを作成
        let comparison = create_algorithm_comparison(&results);
        let multi_result = MultiAlgorithmSimulationResult {
            results,
            comparison,
        };

        // 複数アルゴリズムの結果を保存
        save_multi_algorithm_result(&multi_result, &output_dir)?;
    } else {
        // 単一アルゴリズムを実行
        let result = match config.algorithm {
            AlgorithmType::Momentum => run_momentum_simulation(&config).await?,
            AlgorithmType::Portfolio => run_portfolio_simulation(&config).await?,
            AlgorithmType::TrendFollowing => run_trend_following_simulation(&config).await?,
        };

        // 単一アルゴリズムの結果を保存
        save_simulation_result(&result, &output_dir)?;
    }

    println!("✅ Simulation completed!");

    if !run_all_algorithms {
        // 単一アルゴリズムの場合のみサマリーを表示
        println!(
            "📊 For detailed results, check the output directory: {}",
            output_dir
        );
    }

    Ok(())
}

async fn validate_and_convert_args(args: SimulateArgs) -> Result<SimulationConfig> {
    use chrono::NaiveDate;
    use std::str::FromStr;

    // 日付の解析
    let start_date = if let Some(start_str) = args.start {
        let naive_date = NaiveDate::parse_from_str(&start_str, "%Y-%m-%d")?;
        naive_date.and_hms_opt(0, 0, 0).unwrap().and_utc()
    } else {
        // デフォルト: 30日前
        Utc::now() - chrono::Duration::days(30)
    };

    let end_date = if let Some(end_str) = args.end {
        let naive_date = NaiveDate::parse_from_str(&end_str, "%Y-%m-%d")?;
        naive_date.and_hms_opt(23, 59, 59).unwrap().and_utc()
    } else {
        // デフォルト: 現在
        Utc::now()
    };

    if start_date >= end_date {
        return Err(anyhow::anyhow!("Start date must be before end date"));
    }

    // トークンリストの解析
    let target_tokens = if let Some(tokens_str) = args.tokens {
        tokens_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect()
    } else {
        // 自動でtop volatility tokensを取得
        if args.verbose {
            println!("🔍 Fetching top {} volatility tokens...", args.num_tokens);
        }

        use crate::utils::config::Config;
        let config = Config::from_env();
        let backend_client = BackendClient::new_with_url(config.backend_url);
        let volatility_tokens = backend_client
            .get_volatility_tokens(
                start_date,
                end_date,
                args.num_tokens as u32,
                None, // quote_token なしで試してみる
                None, // min_depth はデフォルト値を使用
            )
            .await?;

        if volatility_tokens.is_empty() {
            return Err(anyhow::anyhow!(
                "No volatility tokens found for the specified period. Please specify tokens manually with --tokens option."
            ));
        }

        let tokens: Vec<String> = volatility_tokens
            .into_iter()
            .map(|token_account| token_account.to_string())
            .collect();

        if args.verbose {
            println!("✅ Found {} volatility tokens", tokens.len());
        }

        tokens
    };

    Ok(SimulationConfig {
        start_date,
        end_date,
        algorithm: if let Some(algo) = &args.algorithm {
            AlgorithmType::from(algo.as_str())
        } else {
            AlgorithmType::Momentum // 全アルゴリズム実行時の一時的な値
        },
        initial_capital: BigDecimal::from_str(&args.capital.to_string())?,
        quote_token: args.quote_token,
        target_tokens,
        rebalance_interval: RebalanceInterval::parse(&args.rebalance_interval)?,
        fee_model: FeeModel::from((args.fee_model.as_str(), args.custom_fee)),
        slippage_rate: args.slippage,
        gas_cost: BigDecimal::from_str(&args.gas_cost.to_string())?,
        min_trade_amount: BigDecimal::from_str(&args.min_trade.to_string())?,
        prediction_horizon: chrono::Duration::hours(args.prediction_horizon as i64),
        historical_days: args.historical_days as i64,
    })
}

#[allow(dead_code)]
async fn run_buy_and_hold_simulation(config: &SimulationConfig) -> Result<SimulationResult> {
    println!(
        "💰 Running simulation for algorithm: {:?}",
        config.algorithm
    );

    match config.algorithm {
        AlgorithmType::Momentum => run_momentum_simulation(config).await,
        AlgorithmType::Portfolio => run_portfolio_simulation(config).await,
        AlgorithmType::TrendFollowing => run_trend_following_simulation(config).await,
    }
}

async fn run_momentum_simulation(config: &SimulationConfig) -> Result<SimulationResult> {
    println!(
        "📈 Running momentum simulation for tokens: {:?}",
        config.target_tokens
    );

    let backend_client = BackendClient::new();

    // 1. 価格データを取得
    let price_data = fetch_price_data(&backend_client, config).await?;

    if price_data.is_empty() {
        return Err(anyhow::anyhow!(
            "No price data available for simulation period. Please check your backend connection and ensure price data exists for the specified tokens and time period."
        ));
    }

    // 2. Momentumアルゴリズムでシミュレーション実行
    let simulation_result = run_momentum_timestep_simulation(config, &price_data).await?;

    Ok(simulation_result)
}

async fn run_portfolio_simulation(config: &SimulationConfig) -> Result<SimulationResult> {
    println!("📊 Running portfolio optimization simulation");
    println!(
        "🔧 Optimizing portfolio for tokens: {:?}",
        config.target_tokens
    );

    let backend_client = BackendClient::new();

    // 1. 価格データを取得
    let price_data = fetch_price_data(&backend_client, config).await?;

    if price_data.is_empty() {
        return Err(anyhow::anyhow!(
            "No price data available for simulation period. Please check your backend connection and ensure price data exists for the specified tokens and time period."
        ));
    }

    // 2. Portfolio最適化アルゴリズムでシミュレーション実行
    let simulation_result = run_portfolio_timestep_simulation(config, &price_data).await?;

    Ok(simulation_result)
}

async fn run_trend_following_simulation(config: &SimulationConfig) -> Result<SimulationResult> {
    println!("📉 Running trend following simulation");
    println!("📊 Following trends for tokens: {:?}", config.target_tokens);

    let backend_client = BackendClient::new();

    // 1. 価格データを取得
    let price_data = fetch_price_data(&backend_client, config).await?;

    if price_data.is_empty() {
        return Err(anyhow::anyhow!(
            "No price data available for simulation period. Please check your backend connection and ensure price data exists for the specified tokens and time period."
        ));
    }

    // 2. TrendFollowingアルゴリズムでシミュレーション実行
    let simulation_result = run_trend_following_timestep_simulation(config, &price_data).await?;

    Ok(simulation_result)
}

#[allow(dead_code)]
async fn run_simple_simulation(config: &SimulationConfig) -> Result<SimulationResult> {
    let duration = config.end_date - config.start_date;
    let duration_days = duration.num_days();

    // 暫定的なbuy-and-hold実装
    let mock_return = 0.15; // 15%のリターンと仮定
    let initial_value = config
        .initial_capital
        .to_string()
        .parse::<f64>()
        .unwrap_or(1000.0);
    let final_value = initial_value * (1.0 + mock_return);

    // 簡単なパフォーマンス指標
    let performance = PerformanceMetrics {
        total_return: mock_return,
        annualized_return: mock_return * 365.0 / duration_days as f64,
        total_return_pct: mock_return * 100.0,
        volatility: 0.25,
        max_drawdown: -0.1,
        max_drawdown_pct: -10.0,
        sharpe_ratio: 0.8,
        sortino_ratio: 1.2,
        total_trades: 1,
        winning_trades: 1,
        losing_trades: 0,
        win_rate: 1.0,
        profit_factor: 0.0,
        total_costs: 30.0,
        cost_ratio: 30.0 / final_value * 100.0,
        simulation_days: duration_days,
        active_trading_days: 1,
    };

    let config_summary = SimulationSummary {
        start_date: config.start_date,
        end_date: config.end_date,
        algorithm: config.algorithm.clone(),
        initial_capital: initial_value,
        final_value,
        total_return: mock_return * 100.0,
        duration_days,
    };

    let execution_summary = ExecutionSummary {
        total_trades: 1,
        successful_trades: 1,
        failed_trades: 0,
        success_rate: 1.0,
        total_cost: 30.0,
        avg_cost_per_trade: 30.0,
    };

    Ok(SimulationResult {
        config: config_summary,
        performance,
        trades: vec![],
        portfolio_values: vec![],
        execution_summary,
    })
}

fn save_simulation_result(result: &SimulationResult, _output_dir: &str) -> Result<()> {
    use crate::utils::file::ensure_directory_exists;
    use std::path::PathBuf;

    // 出力ディレクトリの作成
    let base_dir = std::env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
    let start_date_str = result.config.start_date.format("%Y-%m-%d");
    let end_date_str = result.config.end_date.format("%Y-%m-%d");
    let algorithm_str = format!("{:?}", result.config.algorithm).to_lowercase();
    let final_output_dir = PathBuf::from(&base_dir)
        .join("simulation_results")
        .join(format!(
            "{}_{}_{}",
            algorithm_str, start_date_str, end_date_str
        ));

    ensure_directory_exists(&final_output_dir)?;

    // JSONファイルに保存
    let result_file = final_output_dir.join("results.json");
    let json_content = serde_json::to_string_pretty(result)?;
    std::fs::write(&result_file, json_content)?;
    println!("💾 Results saved to: {}", result_file.display());

    Ok(())
}

/// 価格データを取得
async fn fetch_price_data(
    backend_client: &BackendClient,
    config: &SimulationConfig,
) -> Result<HashMap<String, Vec<ValueAtTime>>> {
    let mut price_data = HashMap::new();

    // 必要なデータ期間を計算
    let data_start_date = config.start_date - chrono::Duration::days(config.historical_days);
    let data_end_date = config.end_date + config.prediction_horizon;

    println!(
        "📈 Fetching price data from {} to {}",
        data_start_date.format("%Y-%m-%d %H:%M"),
        data_end_date.format("%Y-%m-%d %H:%M")
    );

    for token in &config.target_tokens {
        println!("  Getting data for {}", token);

        let values = backend_client
            .get_price_history(
                &config.quote_token,
                token,
                data_start_date.naive_utc(),
                data_end_date.naive_utc(),
            )
            .await?;

        if values.is_empty() {
            println!("  ⚠️ No price data found for {}", token);
        } else {
            println!("  ✅ Found {} data points for {}", values.len(), token);
            price_data.insert(token.clone(), values);
        }
    }

    Ok(price_data)
}

/// Momentumアルゴリズムでタイムステップシミュレーションを実行
async fn run_momentum_timestep_simulation(
    config: &SimulationConfig,
    price_data: &HashMap<String, Vec<ValueAtTime>>,
) -> Result<SimulationResult> {
    let duration = config.end_date - config.start_date;
    let duration_days = duration.num_days();
    let initial_value = config
        .initial_capital
        .to_string()
        .parse::<f64>()
        .unwrap_or(1000.0);

    // タイムステップ設定
    let time_step = config.rebalance_interval.as_duration();

    let mut current_time = config.start_date;
    let mut portfolio_values = Vec::new();
    let mut trades = Vec::new();
    let mut current_holdings = HashMap::new();

    // 初期ポートフォリオ設定（均等分散）
    let tokens_count = config.target_tokens.len() as f64;
    let initial_per_token = initial_value / tokens_count;

    // 初期価格データを取得
    let initial_prices = get_prices_at_time(price_data, config.start_date)?;

    for token in &config.target_tokens {
        if let Some(&initial_price) = initial_prices.get(token) {
            let token_amount = initial_per_token / initial_price; // 価格で割って数量を計算
            current_holdings.insert(token.clone(), token_amount);
        } else {
            return Err(anyhow::anyhow!(
                "No price data found for token: {} at start date",
                token
            ));
        }
    }

    let mut step_count = 0;
    let max_steps = 1000; // 無限ループ防止

    while current_time <= config.end_date && step_count < max_steps {
        step_count += 1;

        // 現在時点での価格を取得
        let current_prices = get_prices_at_time(price_data, current_time)?;

        // API統合による予測を生成
        let backend_client = BackendClient::new();
        let predictions = generate_api_predictions(
            &backend_client,
            &config.target_tokens,
            &config.quote_token,
            current_time,
            config.historical_days,
            config.prediction_horizon,
        )
        .await?;

        // Momentumアルゴリズムで取引決定
        let ranked_tokens = rank_tokens_by_momentum(predictions.clone());

        // 現在のポートフォリオで取引決定
        for (token, amount) in current_holdings.clone() {
            let current_price = current_prices.get(&token).copied().unwrap_or(0.0);

            // 現在のトークンの期待リターンを計算
            let current_return = predictions
                .iter()
                .find(|p| p.token == token)
                .map(calculate_confidence_adjusted_return)
                .unwrap_or(0.0);

            // 取引決定 - リファクタリングされた関数を使用
            let trading_config = TradingConfig {
                min_profit_threshold: MIN_PROFIT_THRESHOLD,
                switch_multiplier: SWITCH_MULTIPLIER,
                min_trade_amount: 1.0, // config.min_trade_amount would be ideal
            };

            let opportunities = convert_ranked_tokens_to_opportunities(&ranked_tokens);
            let decision = make_trading_decision(
                &token,
                current_return,
                &opportunities,
                amount,
                &trading_config,
            );

            let action = convert_decision_to_action(decision, &token);

            // 取引実行
            let mut trade_ctx = TradeContext {
                current_token: &token,
                current_amount: amount,
                current_price,
                all_prices: &current_prices,
                holdings: &mut current_holdings,
                timestamp: current_time,
                config,
            };

            if let Some(trade) = execute_trading_action(action, &mut trade_ctx)? {
                trades.push(trade);
            }
        }

        // ポートフォリオ価値を計算
        let mut total_value = 0.0;
        let mut holdings_value = HashMap::new();

        for (token, amount) in &current_holdings {
            if let Some(&price) = current_prices.get(token) {
                let value = amount * price;
                holdings_value.insert(token.clone(), value);
                total_value += value;
            }
        }

        // ポートフォリオ記録
        portfolio_values.push(PortfolioValue {
            timestamp: current_time,
            total_value,
            holdings: holdings_value.into_iter().collect(),
            cash_balance: 0.0,
            unrealized_pnl: total_value - initial_value,
        });

        // 次のステップへ
        current_time += time_step;
    }

    let final_value = portfolio_values
        .last()
        .map(|pv| pv.total_value)
        .unwrap_or(initial_value);

    let total_return = (final_value - initial_value) / initial_value;

    // パフォーマンス指標を計算
    let performance = calculate_performance_metrics(
        initial_value,
        final_value,
        &portfolio_values,
        &trades,
        duration_days,
    );

    let config_summary = SimulationSummary {
        start_date: config.start_date,
        end_date: config.end_date,
        algorithm: config.algorithm.clone(),
        initial_capital: initial_value,
        final_value,
        total_return: total_return * 100.0,
        duration_days,
    };

    let execution_summary = ExecutionSummary {
        total_trades: trades.len(),
        successful_trades: trades.len(), // 暫定的に全て成功とする
        failed_trades: 0,
        success_rate: 1.0,
        total_cost: trades
            .iter()
            .map(|t| t.cost.total.to_string().parse::<f64>().unwrap_or(0.0))
            .sum(),
        avg_cost_per_trade: if trades.is_empty() {
            0.0
        } else {
            trades
                .iter()
                .map(|t| t.cost.total.to_string().parse::<f64>().unwrap_or(0.0))
                .sum::<f64>()
                / trades.len() as f64
        },
    };

    Ok(SimulationResult {
        config: config_summary,
        performance,
        trades,
        portfolio_values,
        execution_summary,
    })
}

/// 指定時刻での価格を取得（前後1時間以内のデータが必要）
fn get_prices_at_time(
    price_data: &HashMap<String, Vec<ValueAtTime>>,
    target_time: DateTime<Utc>,
) -> Result<HashMap<String, f64>> {
    let mut prices = HashMap::new();
    let one_hour = chrono::Duration::hours(1);
    let time_window_start = target_time - one_hour;
    let time_window_end = target_time + one_hour;

    for (token, values) in price_data {
        // target_time の前後1時間以内のデータを検索
        let nearby_values: Vec<&ValueAtTime> = values
            .iter()
            .filter(|v| {
                let value_time: DateTime<Utc> = DateTime::from_naive_utc_and_offset(v.time, Utc);
                value_time >= time_window_start && value_time <= time_window_end
            })
            .collect();

        if nearby_values.is_empty() {
            return Err(anyhow::anyhow!(
                "No price data found for token '{}' within 1 hour of target time {}. \
                 This indicates insufficient data quality for reliable simulation. \
                 Please ensure continuous price data is available for the simulation period.",
                token,
                target_time.format("%Y-%m-%d %H:%M:%S UTC")
            ));
        }

        // 最も近い価格データを選択
        let closest_value = nearby_values
            .iter()
            .min_by_key(|v| {
                let value_time: DateTime<Utc> = DateTime::from_naive_utc_and_offset(v.time, Utc);
                (value_time - target_time).num_seconds().abs()
            })
            .unwrap();

        prices.insert(token.clone(), closest_value.value);
    }

    Ok(prices)
}

/// パフォーマンス指標を計算
fn calculate_performance_metrics(
    initial_value: f64,
    final_value: f64,
    portfolio_values: &[PortfolioValue],
    trades: &[TradeExecution],
    duration_days: i64,
) -> PerformanceMetrics {
    let total_return = (final_value - initial_value) / initial_value;
    let annualized_return = if duration_days > 0 {
        total_return * 365.0 / duration_days as f64
    } else {
        0.0
    };

    // ボラティリティ計算
    let returns: Vec<f64> = portfolio_values
        .windows(2)
        .map(|w| (w[1].total_value - w[0].total_value) / w[0].total_value)
        .collect();

    let volatility = if returns.len() > 1 {
        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance =
            returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
        variance.sqrt() * (252.0_f64).sqrt() // 年率換算
    } else {
        0.0
    };

    // ドローダウン計算
    let mut max_value = initial_value;
    let mut max_drawdown = 0.0;

    for pv in portfolio_values {
        if pv.total_value > max_value {
            max_value = pv.total_value;
        }
        let drawdown = (pv.total_value - max_value) / max_value;
        if drawdown < max_drawdown {
            max_drawdown = drawdown;
        }
    }

    // 取引分析
    let mut total_profit = 0.0;
    let mut total_loss = 0.0;
    let mut winning_trades_count = 0;

    for trade in trades {
        let profit_loss = trade.portfolio_value_after - trade.portfolio_value_before;
        if profit_loss > 0.0 {
            total_profit += profit_loss;
            winning_trades_count += 1;
        } else if profit_loss < 0.0 {
            total_loss += -profit_loss; // 損失は正の値として計算
        }
    }

    let losing_trades = trades.len() - winning_trades_count;
    let win_rate = if trades.is_empty() {
        0.0
    } else {
        winning_trades_count as f64 / trades.len() as f64
    };

    // プロフィットファクター = 総利益 / 総損失
    let profit_factor = if total_loss > 0.0 {
        total_profit / total_loss
    } else if total_profit > 0.0 {
        // 損失がない場合は無限大を表す大きな値
        f64::MAX
    } else {
        // 利益も損失もない場合
        0.0
    };

    let total_costs = trades
        .iter()
        .map(|t| t.cost.total.to_string().parse::<f64>().unwrap_or(0.0))
        .sum::<f64>();

    let cost_ratio = if final_value > 0.0 {
        total_costs / final_value * 100.0
    } else {
        0.0
    };

    // シャープレシオ
    let sharpe_ratio = if volatility > 0.0 {
        annualized_return / volatility
    } else {
        0.0
    };

    PerformanceMetrics {
        total_return,
        annualized_return,
        total_return_pct: total_return * 100.0,
        volatility,
        max_drawdown,
        max_drawdown_pct: max_drawdown * 100.0,
        sharpe_ratio,
        sortino_ratio: sharpe_ratio, // 暫定的にシャープレシオと同じ
        total_trades: trades.len(),
        winning_trades: winning_trades_count,
        losing_trades,
        win_rate,
        profit_factor,
        total_costs,
        cost_ratio,
        simulation_days: duration_days,
        active_trading_days: if trades.is_empty() { 0 } else { duration_days },
    }
}

/// API統合による予測生成（Chronos API使用）
async fn generate_api_predictions(
    backend_client: &BackendClient,
    target_tokens: &[String],
    quote_token: &str,
    current_time: DateTime<Utc>,
    historical_days: i64,
    prediction_horizon: Duration,
) -> Result<Vec<PredictionData>> {
    let mut predictions = Vec::new();

    // Chronos APIクライアントを初期化
    let chronos_url = std::env::var("CHRONOS_URL")
        .or_else(|_| std::env::var("BACKEND_URL").map(|url| format!("{}/chronos", url)))
        .unwrap_or_else(|_| "http://localhost:8000".to_string());

    let chronos_client = ChronosApiClient::new(chronos_url);

    let history_start = current_time - Duration::days(historical_days);
    let _prediction_hours = prediction_horizon.num_hours() as usize;

    for token in target_tokens {
        // 価格履歴を取得
        let price_history_result = backend_client
            .get_price_history(
                token,
                quote_token,
                history_start.naive_utc(),
                current_time.naive_utc(),
            )
            .await;

        let price_history = match price_history_result {
            Ok(history) => history,
            Err(e) => {
                eprintln!("Warning: Failed to get price history for {}: {}", token, e);
                continue;
            }
        };

        if price_history.len() < 10 {
            eprintln!(
                "Warning: Insufficient price history for {}: {} points",
                token,
                price_history.len()
            );
            continue;
        }

        // 価格履歴をChronos API用のフォーマットに変換
        let timestamps: Vec<DateTime<Utc>> = price_history
            .iter()
            .map(|p| DateTime::from_naive_utc_and_offset(p.time, Utc))
            .collect();
        let values: Vec<f64> = price_history.iter().map(|p| p.value).collect();

        if values.is_empty() {
            continue;
        }

        let current_price = *values.last().unwrap();
        let forecast_until = current_time + prediction_horizon;

        // Chronos API予測リクエストを作成
        let prediction_request = ZeroShotPredictionRequest {
            timestamp: timestamps,
            values,
            forecast_until,
            model_name: Some("chronos_default".to_string()),
            model_params: None,
        };

        // 予測を実行
        match chronos_client.predict(prediction_request).await {
            Ok(async_response) => {
                // 予測完了まで待機
                match chronos_client
                    .poll_prediction_until_complete(&async_response.task_id)
                    .await
                {
                    Ok(result) => {
                        if let Some(forecast_result) = result.result {
                            // 24時間後の予測価格を取得（最初のデータポイント）
                            let predicted_price_24h = forecast_result
                                .forecast_values
                                .first()
                                .copied()
                                .unwrap_or(current_price);

                            // 信頼度計算（利用可能であればメトリクスから、そうでなければデフォルト）
                            let confidence = forecast_result
                                .metrics
                                .as_ref()
                                .and_then(|m| m.get("confidence"))
                                .copied()
                                .unwrap_or(0.7); // デフォルト信頼度

                            predictions.push(PredictionData {
                                token: token.clone(),
                                current_price: BigDecimal::from_f64(current_price)
                                    .unwrap_or_default(),
                                predicted_price_24h: BigDecimal::from_f64(predicted_price_24h)
                                    .unwrap_or_default(),
                                timestamp: current_time,
                                confidence: Some(confidence),
                            });
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to get prediction result for {}: {}",
                            token, e
                        );
                        // フォールバック：現在価格をそのまま使用
                        predictions.push(PredictionData {
                            token: token.clone(),
                            current_price: BigDecimal::from_f64(current_price).unwrap_or_default(),
                            predicted_price_24h: BigDecimal::from_f64(current_price)
                                .unwrap_or_default(),
                            timestamp: current_time,
                            confidence: Some(0.1), // 低い信頼度
                        });
                    }
                }
            }
            Err(e) => {
                eprintln!("Warning: Failed to start prediction for {}: {}", token, e);
                // フォールバック：現在価格をそのまま使用
                predictions.push(PredictionData {
                    token: token.clone(),
                    current_price: BigDecimal::from_f64(current_price).unwrap_or_default(),
                    predicted_price_24h: BigDecimal::from_f64(current_price).unwrap_or_default(),
                    timestamp: current_time,
                    confidence: Some(0.1), // 低い信頼度
                });
            }
        }
    }

    Ok(predictions)
}

/// シンプルなボラティリティ計算（残しておく必要があるため再実装）
fn calculate_simple_volatility(prices: &[f64]) -> f64 {
    if prices.len() < 2 {
        return 0.0;
    }

    let mut returns = Vec::new();
    for i in 1..prices.len() {
        if prices[i - 1] > 0.0 {
            returns.push((prices[i] - prices[i - 1]) / prices[i - 1]);
        }
    }

    if returns.is_empty() {
        return 0.0;
    }

    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;

    variance.sqrt()
}

struct TradeContext<'a> {
    current_token: &'a str,
    current_amount: f64,
    current_price: f64,
    all_prices: &'a HashMap<String, f64>,
    holdings: &'a mut HashMap<String, f64>,
    timestamp: DateTime<Utc>,
    config: &'a SimulationConfig,
}

/// 取引アクションを実行
fn execute_trading_action(
    action: TradingAction,
    ctx: &mut TradeContext,
) -> Result<Option<TradeExecution>> {
    match action {
        TradingAction::Hold => Ok(None),

        TradingAction::Sell { token: _, target } => {
            let target_price = ctx.all_prices.get(&target).copied().unwrap_or(0.0);
            if target_price <= 0.0 {
                return Ok(None);
            }

            // 取引コストを計算
            let trade_cost = calculate_trading_cost(
                ctx.current_amount,
                &ctx.config.fee_model,
                ctx.config.slippage_rate,
                ctx.config
                    .gas_cost
                    .to_string()
                    .parse::<f64>()
                    .unwrap_or(0.01),
            );

            let net_amount = ctx.current_amount - trade_cost;
            let new_amount = net_amount * ctx.current_price / target_price;

            // ポートフォリオ更新
            ctx.holdings.remove(ctx.current_token);
            ctx.holdings.insert(target.clone(), new_amount);

            let portfolio_before = ctx.current_amount * ctx.current_price;
            let portfolio_after = new_amount * target_price;

            Ok(Some(TradeExecution {
                timestamp: ctx.timestamp,
                from_token: ctx.current_token.to_string(),
                to_token: target,
                amount: ctx.current_amount,
                executed_price: target_price,
                cost: TradingCost {
                    protocol_fee: BigDecimal::from_f64(trade_cost * 0.7).unwrap_or_default(),
                    slippage: BigDecimal::from_f64(trade_cost * 0.2).unwrap_or_default(),
                    gas_fee: ctx.config.gas_cost.clone(),
                    total: BigDecimal::from_f64(trade_cost).unwrap_or_default(),
                },
                portfolio_value_before: portfolio_before,
                portfolio_value_after: portfolio_after,
                success: true,
                reason: "Momentum sell executed".to_string(),
            }))
        }

        TradingAction::Switch { from: _, to } => {
            let target_price = ctx.all_prices.get(&to).copied().unwrap_or(0.0);
            if target_price <= 0.0 {
                return Ok(None);
            }

            // 取引コストを計算
            let trade_cost = calculate_trading_cost(
                ctx.current_amount,
                &ctx.config.fee_model,
                ctx.config.slippage_rate,
                ctx.config
                    .gas_cost
                    .to_string()
                    .parse::<f64>()
                    .unwrap_or(0.01),
            );

            let net_amount = ctx.current_amount - trade_cost;
            let new_amount = net_amount * ctx.current_price / target_price;

            // ポートフォリオ更新
            ctx.holdings.remove(ctx.current_token);
            ctx.holdings.insert(to.clone(), new_amount);

            let portfolio_before = ctx.current_amount * ctx.current_price;
            let portfolio_after = new_amount * target_price;

            Ok(Some(TradeExecution {
                timestamp: ctx.timestamp,
                from_token: ctx.current_token.to_string(),
                to_token: to,
                amount: ctx.current_amount,
                executed_price: target_price,
                cost: TradingCost {
                    protocol_fee: BigDecimal::from_f64(trade_cost * 0.7).unwrap_or_default(),
                    slippage: BigDecimal::from_f64(trade_cost * 0.2).unwrap_or_default(),
                    gas_fee: ctx.config.gas_cost.clone(),
                    total: BigDecimal::from_f64(trade_cost).unwrap_or_default(),
                },
                portfolio_value_before: portfolio_before,
                portfolio_value_after: portfolio_after,
                success: true,
                reason: "Momentum switch executed".to_string(),
            }))
        }
    }
}

/// 取引コストを計算
pub fn calculate_trading_cost(
    amount: f64,
    fee_model: &FeeModel,
    slippage_rate: f64,
    gas_cost: f64,
) -> f64 {
    let protocol_fee = match fee_model {
        FeeModel::Realistic => amount * 0.003, // 0.3%
        FeeModel::Zero => 0.0,
        FeeModel::Custom(rate) => amount * rate,
    };

    let slippage_cost = amount * slippage_rate;

    protocol_fee + slippage_cost + gas_cost
}

// Momentum アルゴリズム関数はcommon::algorithm::momentumから使用

/// Pure function for trading decisions with better testability
pub fn make_trading_decision(
    current_token: &str,
    current_return: f64,
    ranked_opportunities: &[TokenOpportunity],
    holding_amount: f64,
    config: &TradingConfig,
) -> TradingDecision {
    if ranked_opportunities.is_empty() {
        return TradingDecision::Hold;
    }

    let best_opportunity = &ranked_opportunities[0];

    if best_opportunity.token == current_token {
        return TradingDecision::Hold;
    }

    if holding_amount < config.min_trade_amount {
        return TradingDecision::Hold;
    }

    if current_return < config.min_profit_threshold {
        return TradingDecision::Sell {
            target_token: best_opportunity.token.clone(),
        };
    }

    let confidence_adjusted_return =
        best_opportunity.expected_return * best_opportunity.confidence.unwrap_or(0.5);

    if confidence_adjusted_return > current_return * config.switch_multiplier {
        return TradingDecision::Switch {
            from: current_token.to_string(),
            to: best_opportunity.token.clone(),
        };
    }

    TradingDecision::Hold
}

/// Helper function to convert old format to new format for gradual migration
pub fn convert_ranked_tokens_to_opportunities(
    ranked_tokens: &[(String, f64, Option<f64>)],
) -> Vec<TokenOpportunity> {
    ranked_tokens
        .iter()
        .map(|(token, expected_return, confidence)| TokenOpportunity {
            token: token.clone(),
            expected_return: *expected_return,
            confidence: *confidence,
        })
        .collect()
}

/// Helper function to convert TradingDecision back to TradingAction for backward compatibility
pub fn convert_decision_to_action(decision: TradingDecision, current_token: &str) -> TradingAction {
    match decision {
        TradingDecision::Hold => TradingAction::Hold,
        TradingDecision::Sell { target_token } => TradingAction::Sell {
            token: current_token.to_string(),
            target: target_token,
        },
        TradingDecision::Switch { from, to } => TradingAction::Switch { from, to },
    }
}

// Phase 2: Immutable data structure operations
impl ImmutablePortfolio {
    pub fn new(initial_capital: f64, initial_token: &str) -> Self {
        let mut holdings = HashMap::new();
        holdings.insert(initial_token.to_string(), initial_capital);

        Self {
            holdings,
            cash_balance: 0.0,
            timestamp: Utc::now(),
        }
    }

    pub fn total_value(&self, market: &MarketSnapshot) -> f64 {
        let mut total = self.cash_balance;

        for (token, amount) in &self.holdings {
            if let Some(&price) = market.prices.get(token) {
                total += amount * price;
            }
        }

        total
    }

    pub fn apply_trade(
        &self,
        decision: &TradingDecision,
        market: &MarketSnapshot,
        _config: &TradingConfig,
    ) -> Result<PortfolioTransition> {
        let mut new_holdings = self.holdings.clone();
        let mut cost = 0.0;

        let new_portfolio = match decision {
            TradingDecision::Hold => ImmutablePortfolio {
                holdings: new_holdings,
                cash_balance: self.cash_balance,
                timestamp: market.timestamp,
            },
            TradingDecision::Sell { target_token } => {
                // Sell current holding to target token
                if let Some((current_token, current_amount)) = new_holdings.iter().next() {
                    let current_token = current_token.clone();
                    let current_amount = *current_amount;

                    new_holdings.remove(&current_token);

                    if let Some(&target_price) = market.prices.get(target_token) {
                        let target_amount = current_amount / target_price;
                        cost = current_amount * 0.006; // Simple fee calculation
                        let net_amount = target_amount - (cost / target_price);

                        new_holdings.insert(target_token.clone(), net_amount);
                    }
                }

                ImmutablePortfolio {
                    holdings: new_holdings,
                    cash_balance: self.cash_balance,
                    timestamp: market.timestamp,
                }
            }
            TradingDecision::Switch { from, to } => {
                if let Some(&from_amount) = new_holdings.get(from) {
                    new_holdings.remove(from);

                    if let (Some(&from_price), Some(&to_price)) =
                        (market.prices.get(from), market.prices.get(to))
                    {
                        let from_value = from_amount * from_price;
                        cost = from_value * 0.006; // Simple fee calculation
                        let net_value = from_value - cost;
                        let to_amount = net_value / to_price;

                        new_holdings.insert(to.clone(), to_amount);
                    }
                }

                ImmutablePortfolio {
                    holdings: new_holdings,
                    cash_balance: self.cash_balance,
                    timestamp: market.timestamp,
                }
            }
        };

        Ok(PortfolioTransition {
            from: self.clone(),
            to: new_portfolio,
            action: decision.clone(),
            cost,
            reason: format!("Applied {:?}", decision),
        })
    }
}

impl MarketSnapshot {
    pub fn new(prices: HashMap<String, f64>) -> Self {
        let data_quality = Self::assess_data_quality(&prices);

        Self {
            prices,
            timestamp: Utc::now(),
            data_quality,
        }
    }

    pub fn from_price_data(
        price_data: &HashMap<String, Vec<ValueAtTime>>,
        target_time: DateTime<Utc>,
    ) -> Result<Self> {
        let prices = get_prices_at_time(price_data, target_time)?;
        let data_quality = Self::assess_data_quality(&prices);

        Ok(Self {
            prices,
            timestamp: target_time,
            data_quality,
        })
    }

    fn assess_data_quality(prices: &HashMap<String, f64>) -> DataQuality {
        if prices.is_empty() {
            return DataQuality::Poor;
        }

        // Simple heuristic: more tokens = better quality
        match prices.len() {
            0 => DataQuality::Poor,
            1 => DataQuality::Low,
            2..=5 => DataQuality::Medium,
            _ => DataQuality::High,
        }
    }

    pub fn get_price(&self, token: &str) -> Option<f64> {
        self.prices.get(token).copied()
    }

    pub fn is_reliable(&self) -> bool {
        matches!(self.data_quality, DataQuality::High | DataQuality::Medium)
    }
}

// Phase 3: Strategy Pattern implementations
impl TradingStrategy for MomentumStrategy {
    fn name(&self) -> &'static str {
        "Momentum"
    }

    fn make_decision(
        &self,
        portfolio: &ImmutablePortfolio,
        _market: &MarketSnapshot,
        opportunities: &[TokenOpportunity],
        config: &TradingConfig,
    ) -> Result<TradingDecision> {
        if opportunities.is_empty() {
            return Ok(TradingDecision::Hold);
        }

        // Get current token (assumes single token portfolio for momentum)
        let current_token = portfolio.holdings.keys().next().unwrap();
        let current_amount = portfolio.holdings.values().next().unwrap();

        if *current_amount < config.min_trade_amount {
            return Ok(TradingDecision::Hold);
        }

        let best_opportunity = &opportunities[0];

        // Skip if it's the same token
        if best_opportunity.token == *current_token {
            return Ok(TradingDecision::Hold);
        }

        // Check confidence threshold
        let confidence = best_opportunity.confidence.unwrap_or(0.5);
        if confidence < self.min_confidence {
            return Ok(TradingDecision::Hold);
        }

        // Calculate current return (simplified)
        // For momentum strategy, assume current position has baseline return
        let portfolio_return = config.min_profit_threshold; // Meet minimum threshold

        let confidence_adjusted_return = best_opportunity.expected_return * confidence;
        if confidence_adjusted_return > portfolio_return * config.switch_multiplier {
            return Ok(TradingDecision::Switch {
                from: current_token.clone(),
                to: best_opportunity.token.clone(),
            });
        }

        Ok(TradingDecision::Hold)
    }

    fn should_rebalance(&self, _portfolio: &ImmutablePortfolio, _market: &MarketSnapshot) -> bool {
        false // Momentum doesn't typically rebalance
    }
}

impl TradingStrategy for PortfolioStrategy {
    fn name(&self) -> &'static str {
        "Portfolio"
    }

    fn make_decision(
        &self,
        portfolio: &ImmutablePortfolio,
        market: &MarketSnapshot,
        opportunities: &[TokenOpportunity],
        config: &TradingConfig,
    ) -> Result<TradingDecision> {
        // Portfolio optimization strategy - simplified version
        if opportunities.is_empty() {
            return Ok(TradingDecision::Hold);
        }

        let total_value = portfolio.total_value(market);
        if total_value < config.min_trade_amount {
            return Ok(TradingDecision::Hold);
        }

        // Check if rebalancing is needed
        if !self.should_rebalance(portfolio, market) {
            return Ok(TradingDecision::Hold);
        }

        // Simple rebalancing: if portfolio has fewer positions than max, diversify
        if portfolio.holdings.len() < self.max_positions
            && opportunities.len() > portfolio.holdings.len()
        {
            let best_new_opportunity = opportunities
                .iter()
                .find(|opp| !portfolio.holdings.contains_key(&opp.token))
                .unwrap_or(&opportunities[0]);

            // Get the token with lowest performance to switch from
            if let Some((worst_token, _)) = portfolio.holdings.iter().next() {
                return Ok(TradingDecision::Switch {
                    from: worst_token.clone(),
                    to: best_new_opportunity.token.clone(),
                });
            }
        }

        Ok(TradingDecision::Hold)
    }

    fn should_rebalance(&self, portfolio: &ImmutablePortfolio, _market: &MarketSnapshot) -> bool {
        // Simple rebalancing logic: rebalance if we have uneven distribution
        if portfolio.holdings.len() <= 1 {
            return true;
        }

        let values: Vec<f64> = portfolio.holdings.values().copied().collect();
        let avg_value = values.iter().sum::<f64>() / values.len() as f64;

        // Check if any position deviates too much from average
        values
            .iter()
            .any(|&value| (value - avg_value).abs() / avg_value > self.rebalance_threshold)
    }
}

impl TradingStrategy for TrendFollowingStrategy {
    fn name(&self) -> &'static str {
        "TrendFollowing"
    }

    fn make_decision(
        &self,
        portfolio: &ImmutablePortfolio,
        market: &MarketSnapshot,
        opportunities: &[TokenOpportunity],
        config: &TradingConfig,
    ) -> Result<TradingDecision> {
        if opportunities.is_empty() {
            return Ok(TradingDecision::Hold);
        }

        let total_value = portfolio.total_value(market);
        if total_value < config.min_trade_amount {
            return Ok(TradingDecision::Hold);
        }

        // Find strongest trending opportunity
        let best_trend = opportunities
            .iter()
            .max_by(|a, b| {
                let a_strength = a.expected_return * a.confidence.unwrap_or(0.5);
                let b_strength = b.expected_return * b.confidence.unwrap_or(0.5);
                a_strength.partial_cmp(&b_strength).unwrap()
            })
            .unwrap();

        // Check if trend is strong enough
        let trend_strength = best_trend.expected_return * best_trend.confidence.unwrap_or(0.5);
        if trend_strength < self.volatility_threshold {
            return Ok(TradingDecision::Hold);
        }

        // Get current main holding
        if let Some((current_token, current_amount)) = portfolio.holdings.iter().next() {
            if current_token != &best_trend.token && *current_amount >= config.min_trade_amount {
                return Ok(TradingDecision::Switch {
                    from: current_token.clone(),
                    to: best_trend.token.clone(),
                });
            }
        }

        Ok(TradingDecision::Hold)
    }

    fn should_rebalance(&self, _portfolio: &ImmutablePortfolio, _market: &MarketSnapshot) -> bool {
        true // Trend following always monitors for new trends
    }
}

impl StrategyContext {
    pub fn new(strategy: Box<dyn TradingStrategy>) -> Self {
        Self { strategy }
    }

    pub fn execute_strategy(
        &self,
        portfolio: &ImmutablePortfolio,
        market: &MarketSnapshot,
        opportunities: &[TokenOpportunity],
        config: &TradingConfig,
    ) -> Result<TradingDecision> {
        self.strategy
            .make_decision(portfolio, market, opportunities, config)
    }

    pub fn strategy_name(&self) -> &'static str {
        self.strategy.name()
    }
}

// Portfolio 最適化アルゴリズム実装

/// Portfolio最適化のタイムステップシミュレーション
async fn run_portfolio_timestep_simulation(
    config: &SimulationConfig,
    price_data: &HashMap<String, Vec<ValueAtTime>>,
) -> Result<SimulationResult> {
    let mut portfolio_values = Vec::new();
    let mut trades = Vec::new();
    let mut current_holdings: HashMap<String, BigDecimal> = HashMap::new();
    let mut cash_balance = config.initial_capital.clone();
    let mut total_cost = BigDecimal::from(0);

    let simulation_start = config.start_date;
    let simulation_end = config.end_date;

    // 初期ポートフォリオを構築（均等分散）
    let num_tokens = config.target_tokens.len() as f64;
    let initial_allocation =
        config.initial_capital.clone() / BigDecimal::from_f64(num_tokens).unwrap();

    for token in &config.target_tokens {
        if let Some(token_prices) = price_data.get(token) {
            if let Some(initial_price) = token_prices.first() {
                let token_amount =
                    initial_allocation.clone() / BigDecimal::from_f64(initial_price.value).unwrap();
                current_holdings.insert(token.clone(), token_amount);
                cash_balance -= initial_allocation.clone();
            }
        }
    }

    // 日次リバランス
    let mut current_date = simulation_start;
    while current_date <= simulation_end {
        // 現在のポートフォリオ価値を計算
        let mut total_value = cash_balance.clone();
        let mut holdings_values = HashMap::new();

        for (token, amount) in &current_holdings {
            match get_price_at_time(price_data, token, current_date) {
                Ok(current_price) => {
                    let value = amount * BigDecimal::from_f64(current_price).unwrap();
                    holdings_values.insert(token.clone(), value.clone());
                    total_value += value;
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

        let unrealized_pnl = total_value.clone() - config.initial_capital.clone();

        // ポートフォリオスナップショットを記録
        portfolio_values.push(PortfolioValue {
            timestamp: current_date,
            total_value: total_value.to_string().parse::<f64>().unwrap_or(0.0),
            cash_balance: cash_balance.to_string().parse::<f64>().unwrap_or(0.0),
            holdings: holdings_values
                .iter()
                .map(|(k, v)| (k.clone(), v.to_string().parse::<f64>().unwrap_or(0.0)))
                .collect(),
            unrealized_pnl: unrealized_pnl.to_string().parse::<f64>().unwrap_or(0.0),
        });

        // リバランス判定（週次）
        if current_date.weekday().num_days_from_monday() == 0 {
            let rebalance_trades = calculate_optimal_portfolio_weights(
                &config.target_tokens,
                price_data,
                current_date,
                config.historical_days,
            )?;

            // リバランス実行
            let context = TradingContext {
                price_data,
                current_date,
                fee_model: &config.fee_model,
                slippage_rate: config.slippage_rate,
                gas_cost: &config.gas_cost,
            };
            let mut portfolio = PortfolioState {
                holdings: &mut current_holdings,
                cash_balance: &mut cash_balance,
                total_cost: &mut total_cost,
                trades: &mut trades,
            };
            execute_portfolio_rebalance(&mut portfolio, &context, &rebalance_trades)?;
        }

        current_date += config.rebalance_interval.as_duration();
    }

    // 最終パフォーマンス計算
    let initial_value = config
        .initial_capital
        .to_string()
        .parse::<f64>()
        .unwrap_or(0.0);
    let final_value = portfolio_values
        .last()
        .map(|pv| pv.total_value)
        .unwrap_or(initial_value);

    let performance = calculate_performance_metrics(
        initial_value,
        final_value,
        &portfolio_values,
        &trades,
        (simulation_end - simulation_start).num_days(),
    );

    let simulation_config = SimulationSummary {
        start_date: config.start_date,
        end_date: config.end_date,
        algorithm: config.algorithm.clone(),
        initial_capital: initial_value,
        final_value,
        total_return: (final_value - initial_value) / initial_value * 100.0,
        duration_days: (config.end_date - config.start_date).num_days(),
    };

    let total_trades = trades.len();
    let total_cost_f64 = total_cost.to_string().parse::<f64>().unwrap_or(0.0);

    Ok(SimulationResult {
        config: simulation_config,
        performance,
        trades,
        portfolio_values,
        execution_summary: ExecutionSummary {
            total_trades,
            successful_trades: total_trades, // 簡略化
            failed_trades: 0,
            success_rate: 1.0,
            total_cost: total_cost_f64,
            avg_cost_per_trade: if total_trades == 0 {
                0.0
            } else {
                total_cost_f64 / total_trades as f64
            },
        },
    })
}

/// 最適ポートフォリオウェイトを計算（単純化された実装）
fn calculate_optimal_portfolio_weights(
    tokens: &[String],
    price_data: &HashMap<String, Vec<ValueAtTime>>,
    current_date: DateTime<Utc>,
    historical_days: i64,
) -> Result<HashMap<String, f64>> {
    let mut weights = HashMap::new();

    // 各トークンのリターンとボラティリティを計算
    let mut returns = Vec::new();
    let mut volatilities = Vec::new();

    for token in tokens {
        let historical_data =
            get_historical_data(price_data, token, current_date, historical_days)?;

        if historical_data.len() < 2 {
            continue;
        }

        let prices: Vec<f64> = historical_data.iter().map(|v| v.value).collect();
        let token_returns: Vec<f64> = prices.windows(2).map(|w| (w[1] - w[0]) / w[0]).collect();

        let mean_return = token_returns.iter().sum::<f64>() / token_returns.len() as f64;
        let volatility = calculate_simple_volatility(&prices);

        returns.push(mean_return);
        volatilities.push(volatility);
    }

    // シンプルなリスク調整リターンによる重み付け
    let mut sharpe_ratios = Vec::new();
    for i in 0..returns.len() {
        let sharpe = if volatilities[i] > 0.0 {
            returns[i] / volatilities[i]
        } else {
            0.0
        };
        sharpe_ratios.push(sharpe.max(0.0)); // 負のシャープレシオは0にする
    }

    let total_sharpe: f64 = sharpe_ratios.iter().sum();

    // 重みを正規化
    for (i, token) in tokens.iter().enumerate() {
        let weight = if total_sharpe > 0.0 {
            sharpe_ratios[i] / total_sharpe
        } else {
            1.0 / tokens.len() as f64 // 均等分散フォールバック
        };
        weights.insert(token.clone(), weight);
    }

    Ok(weights)
}

/// ポートフォリオのリバランスを実行
fn execute_portfolio_rebalance(
    portfolio: &mut PortfolioState,
    context: &TradingContext,
    target_weights: &HashMap<String, f64>,
) -> Result<()> {
    // 現在のポートフォリオ価値を計算
    let mut total_portfolio_value = portfolio.cash_balance.clone();
    for (token, amount) in portfolio.holdings.iter() {
        match get_price_at_time(context.price_data, token, context.current_date) {
            Ok(current_price) => {
                total_portfolio_value += amount * BigDecimal::from_f64(current_price).unwrap();
            }
            Err(e) => {
                return Err(e);
            }
        }
    }

    // 各トークンの目標価値を計算
    for (token, target_weight) in target_weights {
        let target_value =
            total_portfolio_value.clone() * BigDecimal::from_f64(*target_weight).unwrap();
        let current_price = get_price_at_time(context.price_data, token, context.current_date)?;
        let target_amount = target_value.clone() / BigDecimal::from_f64(current_price).unwrap();

        let current_amount = portfolio
            .holdings
            .get(token)
            .cloned()
            .unwrap_or_else(|| BigDecimal::from(0));
        let amount_diff = target_amount.clone() - current_amount.clone();

        // 最小取引量チェック
        if amount_diff.abs() * BigDecimal::from_f64(current_price).unwrap() > BigDecimal::from(1) {
            let trade_value = amount_diff.abs() * BigDecimal::from_f64(current_price).unwrap();
            let cost = BigDecimal::from_f64(calculate_trading_cost(
                trade_value.to_string().parse::<f64>().unwrap_or(0.0),
                context.fee_model,
                context.slippage_rate,
                context.gas_cost.to_string().parse::<f64>().unwrap_or(0.0),
            ))
            .unwrap();

            // 取引実行
            if amount_diff > BigDecimal::from(0) {
                // 買い注文
                *portfolio.cash_balance -= trade_value.clone() + cost.clone();
                portfolio.holdings.insert(token.clone(), target_amount);
            } else {
                // 売り注文
                *portfolio.cash_balance += trade_value.clone() - cost.clone();
                portfolio.holdings.insert(token.clone(), target_amount);
            }

            *portfolio.total_cost += cost.clone();

            portfolio.trades.push(TradeExecution {
                timestamp: context.current_date,
                from_token: if amount_diff > BigDecimal::from(0) {
                    "wrap.near".to_string()
                } else {
                    token.clone()
                },
                to_token: if amount_diff > BigDecimal::from(0) {
                    token.clone()
                } else {
                    "wrap.near".to_string()
                },
                amount: amount_diff.abs().to_string().parse::<f64>().unwrap_or(0.0),
                executed_price: current_price,
                cost: TradingCost {
                    protocol_fee: cost.clone() * BigDecimal::from_f64(0.5).unwrap(),
                    slippage: cost.clone() * BigDecimal::from_f64(0.4).unwrap(),
                    gas_fee: cost.clone() * BigDecimal::from_f64(0.1).unwrap(),
                    total: cost.clone(),
                },
                portfolio_value_before: total_portfolio_value
                    .to_string()
                    .parse::<f64>()
                    .unwrap_or(0.0),
                portfolio_value_after: total_portfolio_value
                    .to_string()
                    .parse::<f64>()
                    .unwrap_or(0.0),
                success: true,
                reason: "Portfolio rebalance".to_string(),
            });
        }
    }

    Ok(())
}

// TrendFollowing アルゴリズム実装

/// TrendFollowingのタイムステップシミュレーション
async fn run_trend_following_timestep_simulation(
    config: &SimulationConfig,
    price_data: &HashMap<String, Vec<ValueAtTime>>,
) -> Result<SimulationResult> {
    let mut portfolio_values = Vec::new();
    let mut trades = Vec::new();
    let mut current_holdings: HashMap<String, BigDecimal> = HashMap::new();
    let mut cash_balance = config.initial_capital.clone();
    let mut total_cost = BigDecimal::from(0);
    let mut current_position = String::new(); // 現在のポジション

    let simulation_start = config.start_date;
    let simulation_end = config.end_date;

    let mut current_date = simulation_start;
    while current_date <= simulation_end {
        // 現在のポートフォリオ価値を計算
        let mut total_value = cash_balance.clone();
        let mut holdings_values = HashMap::new();

        for (token, amount) in &current_holdings {
            match get_price_at_time(price_data, token, current_date) {
                Ok(current_price) => {
                    let value = amount * BigDecimal::from_f64(current_price).unwrap();
                    holdings_values.insert(token.clone(), value.clone());
                    total_value += value;
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

        let unrealized_pnl = total_value.clone() - config.initial_capital.clone();

        // ポートフォリオスナップショットを記録
        portfolio_values.push(PortfolioValue {
            timestamp: current_date,
            total_value: total_value.to_string().parse::<f64>().unwrap_or(0.0),
            cash_balance: cash_balance.to_string().parse::<f64>().unwrap_or(0.0),
            holdings: holdings_values
                .iter()
                .map(|(k, v)| (k.clone(), v.to_string().parse::<f64>().unwrap_or(0.0)))
                .collect(),
            unrealized_pnl: unrealized_pnl.to_string().parse::<f64>().unwrap_or(0.0),
        });

        // トレンドフォローイング判定
        let trend_signal = calculate_trend_signal(
            &config.target_tokens,
            price_data,
            current_date,
            config.historical_days,
        )?;

        if let Some(signal) = trend_signal {
            let context = TradingContext {
                price_data,
                current_date,
                fee_model: &config.fee_model,
                slippage_rate: config.slippage_rate,
                gas_cost: &config.gas_cost,
            };
            let mut portfolio = PortfolioState {
                holdings: &mut current_holdings,
                cash_balance: &mut cash_balance,
                total_cost: &mut total_cost,
                trades: &mut trades,
            };
            execute_trend_following_trade(
                &mut portfolio,
                &context,
                &mut current_position,
                &signal,
            )?;
        }

        current_date += config.rebalance_interval.as_duration();
    }

    // 最終パフォーマンス計算
    let initial_value = config
        .initial_capital
        .to_string()
        .parse::<f64>()
        .unwrap_or(0.0);
    let final_value = portfolio_values
        .last()
        .map(|pv| pv.total_value)
        .unwrap_or(initial_value);

    let performance = calculate_performance_metrics(
        initial_value,
        final_value,
        &portfolio_values,
        &trades,
        (simulation_end - simulation_start).num_days(),
    );

    let simulation_config = SimulationSummary {
        start_date: config.start_date,
        end_date: config.end_date,
        algorithm: config.algorithm.clone(),
        initial_capital: initial_value,
        final_value,
        total_return: (final_value - initial_value) / initial_value * 100.0,
        duration_days: (config.end_date - config.start_date).num_days(),
    };

    let total_trades = trades.len();
    let total_cost_f64 = total_cost.to_string().parse::<f64>().unwrap_or(0.0);

    Ok(SimulationResult {
        config: simulation_config,
        performance,
        trades,
        portfolio_values,
        execution_summary: ExecutionSummary {
            total_trades,
            successful_trades: total_trades, // 簡略化
            failed_trades: 0,
            success_rate: 1.0,
            total_cost: total_cost_f64,
            avg_cost_per_trade: if total_trades == 0 {
                0.0
            } else {
                total_cost_f64 / total_trades as f64
            },
        },
    })
}

#[derive(Debug, Clone)]
struct TrendSignal {
    action: TrendAction,
    token: String,
    #[allow(dead_code)]
    strength: f64, // シグナルの強さ (0.0-1.0)
}

#[derive(Debug, Clone, PartialEq)]
enum TrendAction {
    Buy,
    Sell,
    #[allow(dead_code)]
    Hold,
}

/// トレンドシグナルを計算
fn calculate_trend_signal(
    tokens: &[String],
    price_data: &HashMap<String, Vec<ValueAtTime>>,
    current_date: DateTime<Utc>,
    lookback_days: i64,
) -> Result<Option<TrendSignal>> {
    let mut best_signal: Option<TrendSignal> = None;
    let mut max_strength = 0.0;

    for token in tokens {
        let historical_data = get_historical_data(price_data, token, current_date, lookback_days)?;

        if historical_data.len() < 20 {
            continue;
        }

        // 移動平均によるトレンド分析
        let short_ma = calculate_moving_average(&historical_data, 5)?;
        let long_ma = calculate_moving_average(&historical_data, 20)?;

        let trend_strength = (short_ma - long_ma).abs() / long_ma;

        if trend_strength > max_strength && trend_strength > 0.02 {
            // 2%以上の差
            let action = if short_ma > long_ma {
                TrendAction::Buy
            } else {
                TrendAction::Sell
            };

            best_signal = Some(TrendSignal {
                action,
                token: token.clone(),
                strength: trend_strength,
            });
            max_strength = trend_strength;
        }
    }

    Ok(best_signal)
}

/// 移動平均を計算
fn calculate_moving_average(data: &[&ValueAtTime], window: usize) -> Result<f64> {
    if data.len() < window {
        return Ok(0.0);
    }

    let recent_data = &data[data.len() - window..];
    let sum: f64 = recent_data.iter().map(|v| v.value).sum();
    Ok(sum / window as f64)
}

/// トレンドフォローイング取引を実行
fn execute_trend_following_trade(
    portfolio: &mut PortfolioState,
    context: &TradingContext,
    current_position: &mut String,
    signal: &TrendSignal,
) -> Result<()> {
    let current_price = get_price_at_time(context.price_data, &signal.token, context.current_date)?;

    match signal.action {
        TrendAction::Buy => {
            // 現在のポジションを清算してから新しいポジションを取る
            if !current_position.is_empty() && current_position != &signal.token {
                execute_sell_position(portfolio, context, current_position)?;
            }

            // 新しいポジションを構築
            let available_cash = portfolio.cash_balance.clone();
            if available_cash > BigDecimal::from(10) {
                // 最小取引額
                let trade_amount = available_cash.clone() * BigDecimal::from_f64(0.95).unwrap(); // 95%投資
                let token_amount =
                    trade_amount.clone() / BigDecimal::from_f64(current_price).unwrap();

                let cost = BigDecimal::from_f64(calculate_trading_cost(
                    trade_amount.to_string().parse::<f64>().unwrap_or(0.0),
                    context.fee_model,
                    context.slippage_rate,
                    context.gas_cost.to_string().parse::<f64>().unwrap_or(0.0),
                ))
                .unwrap();

                *portfolio.cash_balance -= trade_amount.clone() + cost.clone();
                portfolio
                    .holdings
                    .insert(signal.token.clone(), token_amount.clone());
                *current_position = signal.token.clone();
                *portfolio.total_cost += cost.clone();

                portfolio.trades.push(TradeExecution {
                    timestamp: context.current_date,
                    from_token: "wrap.near".to_string(),
                    to_token: signal.token.clone(),
                    amount: token_amount.to_string().parse::<f64>().unwrap_or(0.0),
                    executed_price: current_price,
                    cost: TradingCost {
                        protocol_fee: cost.clone() * BigDecimal::from_f64(0.5).unwrap(),
                        slippage: cost.clone() * BigDecimal::from_f64(0.4).unwrap(),
                        gas_fee: cost.clone() * BigDecimal::from_f64(0.1).unwrap(),
                        total: cost.clone(),
                    },
                    portfolio_value_before: available_cash
                        .to_string()
                        .parse::<f64>()
                        .unwrap_or(0.0),
                    portfolio_value_after: 0.0, // 計算は簡略化
                    success: true,
                    reason: "Trend following buy".to_string(),
                });
            }
        }
        TrendAction::Sell => {
            if current_position == &signal.token {
                execute_sell_position(portfolio, context, &signal.token)?;
                current_position.clear();
            }
        }
        TrendAction::Hold => {
            // 何もしない
        }
    }

    Ok(())
}

/// ポジションを売却
fn execute_sell_position(
    portfolio: &mut PortfolioState,
    context: &TradingContext,
    token: &str,
) -> Result<()> {
    if let Some(amount) = portfolio.holdings.remove(token) {
        let current_price = get_price_at_time(context.price_data, token, context.current_date)?;
        let trade_value = amount.clone() * BigDecimal::from_f64(current_price).unwrap();

        let cost = BigDecimal::from_f64(calculate_trading_cost(
            trade_value.to_string().parse::<f64>().unwrap_or(0.0),
            context.fee_model,
            context.slippage_rate,
            context.gas_cost.to_string().parse::<f64>().unwrap_or(0.0),
        ))
        .unwrap();

        *portfolio.cash_balance += trade_value.clone() - cost.clone();
        *portfolio.total_cost += cost.clone();

        portfolio.trades.push(TradeExecution {
            timestamp: context.current_date,
            from_token: token.to_string(),
            to_token: "wrap.near".to_string(),
            amount: amount.to_string().parse::<f64>().unwrap_or(0.0),
            executed_price: current_price,
            cost: TradingCost {
                protocol_fee: cost.clone() * BigDecimal::from_f64(0.5).unwrap(),
                slippage: cost.clone() * BigDecimal::from_f64(0.4).unwrap(),
                gas_fee: cost.clone() * BigDecimal::from_f64(0.1).unwrap(),
                total: cost.clone(),
            },
            portfolio_value_before: trade_value.to_string().parse::<f64>().unwrap_or(0.0),
            portfolio_value_after: 0.0, // 計算は簡略化
            success: true,
            reason: "Trend following sell".to_string(),
        });
    }
    Ok(())
}

/// ヘルパー関数：指定日時の価格を取得（前後1時間以内のデータが必要）
fn get_price_at_time(
    price_data: &HashMap<String, Vec<ValueAtTime>>,
    token: &str,
    target_date: DateTime<Utc>,
) -> Result<f64> {
    let token_data = price_data
        .get(token)
        .ok_or_else(|| anyhow::anyhow!("No price data found for token: {}", token))?;

    let one_hour = chrono::Duration::hours(1);
    let time_window_start = target_date - one_hour;
    let time_window_end = target_date + one_hour;

    // target_date の前後1時間以内のデータを検索
    let nearby_values: Vec<&ValueAtTime> = token_data
        .iter()
        .filter(|v| {
            let value_time: DateTime<Utc> = DateTime::from_naive_utc_and_offset(v.time, Utc);
            value_time >= time_window_start && value_time <= time_window_end
        })
        .collect();

    if nearby_values.is_empty() {
        return Err(anyhow::anyhow!(
            "No price data found for token '{}' within 1 hour of target time {}. \
             This indicates insufficient data quality for reliable simulation. \
             Please ensure continuous price data is available for the simulation period.",
            token,
            target_date.format("%Y-%m-%d %H:%M:%S UTC")
        ));
    }

    // 最も近い価格データを選択
    let closest_value = nearby_values
        .iter()
        .min_by_key(|v| {
            let value_time: DateTime<Utc> = DateTime::from_naive_utc_and_offset(v.time, Utc);
            (value_time - target_date).num_seconds().abs()
        })
        .unwrap();

    Ok(closest_value.value)
}

/// ヘルパー関数：過去データを取得
fn get_historical_data<'a>(
    price_data: &'a HashMap<String, Vec<ValueAtTime>>,
    token: &str,
    current_date: DateTime<Utc>,
    lookback_days: i64,
) -> Result<Vec<&'a ValueAtTime>> {
    let start_date = current_date - chrono::Duration::days(lookback_days);

    let historical_data: Vec<&ValueAtTime> = price_data
        .get(token)
        .ok_or_else(|| anyhow::anyhow!("No price data for token: {}", token))?
        .iter()
        .filter(|v| {
            let value_date = v.time.and_utc();
            value_date >= start_date && value_date <= current_date
        })
        .collect();

    Ok(historical_data)
}

/// 複数アルゴリズムの結果を比較
fn create_algorithm_comparison(results: &[SimulationResult]) -> AlgorithmComparison {
    let mut best_return = (AlgorithmType::Momentum, f64::NEG_INFINITY);
    let mut best_sharpe = (AlgorithmType::Momentum, f64::NEG_INFINITY);
    let mut lowest_drawdown = (AlgorithmType::Momentum, f64::INFINITY);
    let mut summary_table = Vec::new();

    for result in results {
        let total_return = result.performance.total_return_pct;
        let sharpe_ratio = result.performance.sharpe_ratio;
        let max_drawdown = result.performance.max_drawdown_pct;

        // ベストパフォーマンスを追跡
        if total_return > best_return.1 {
            best_return = (result.config.algorithm.clone(), total_return);
        }
        if sharpe_ratio > best_sharpe.1 {
            best_sharpe = (result.config.algorithm.clone(), sharpe_ratio);
        }
        if max_drawdown < lowest_drawdown.1 {
            lowest_drawdown = (result.config.algorithm.clone(), max_drawdown);
        }

        // サマリーテーブル行を作成
        summary_table.push(AlgorithmSummaryRow {
            algorithm: result.config.algorithm.clone(),
            total_return_pct: result.performance.total_return_pct,
            annualized_return: result.performance.annualized_return,
            sharpe_ratio: result.performance.sharpe_ratio,
            max_drawdown_pct: result.performance.max_drawdown_pct,
            total_trades: result.performance.total_trades,
            win_rate: result.performance.win_rate,
        });
    }

    AlgorithmComparison {
        best_return,
        best_sharpe,
        lowest_drawdown,
        summary_table,
    }
}

/// 複数アルゴリズムの結果を保存
fn save_multi_algorithm_result(
    result: &MultiAlgorithmSimulationResult,
    output_dir: &str,
) -> Result<()> {
    use std::fs;
    use std::path::PathBuf;

    // 結果保存ディレクトリを作成
    let base_dir = std::env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
    let start_date = result.results.first().unwrap().config.start_date;
    let end_date = result.results.first().unwrap().config.end_date;

    let start_date_str = start_date.format("%Y-%m-%d");
    let end_date_str = end_date.format("%Y-%m-%d");

    let final_output_dir = PathBuf::from(&base_dir)
        .join("simulation_results")
        .join(format!(
            "multi_algorithm_{}_{}",
            start_date_str, end_date_str
        ))
        .join(output_dir);

    fs::create_dir_all(&final_output_dir)?;

    // 全体の結果を保存
    let results_file = final_output_dir.join("multi_results.json");
    let json_content = serde_json::to_string_pretty(result)?;
    fs::write(&results_file, json_content)?;

    // 各アルゴリズムの個別結果も保存
    for individual_result in &result.results {
        let algorithm_name = format!("{:?}", individual_result.config.algorithm).to_lowercase();
        let algorithm_file = final_output_dir.join(format!("{}_results.json", algorithm_name));
        let algorithm_json = serde_json::to_string_pretty(individual_result)?;
        fs::write(&algorithm_file, algorithm_json)?;
    }

    println!(
        "📄 Multi-algorithm results saved to: {}",
        final_output_dir.display()
    );

    // 比較サマリーを出力
    println!("\n🏆 Algorithm Comparison Summary:");
    println!(
        "Best Total Return: {:?} ({:.2}%)",
        result.comparison.best_return.0, result.comparison.best_return.1
    );
    println!(
        "Best Sharpe Ratio: {:?} ({:.4})",
        result.comparison.best_sharpe.0, result.comparison.best_sharpe.1
    );
    println!(
        "Lowest Drawdown: {:?} ({:.2}%)",
        result.comparison.lowest_drawdown.0, result.comparison.lowest_drawdown.1
    );

    println!("\n📊 Algorithm Performance Table:");
    println!(
        "{:<15} {:>12} {:>12} {:>12} {:>12} {:>12} {:>10}",
        "Algorithm",
        "Total Return%",
        "Annual Return%",
        "Sharpe Ratio",
        "Max DD%",
        "Trades",
        "Win Rate%"
    );
    println!("{}", "-".repeat(100));

    for row in &result.comparison.summary_table {
        println!(
            "{:<15} {:>11.2}% {:>11.2}% {:>12.4} {:>11.2}% {:>8} {:>9.1}%",
            format!("{:?}", row.algorithm),
            row.total_return_pct,
            row.annualized_return * 100.0,
            row.sharpe_ratio,
            row.max_drawdown_pct,
            row.total_trades,
            row.win_rate * 100.0
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod api_integration_tests;
