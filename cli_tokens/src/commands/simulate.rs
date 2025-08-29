use anyhow::Result;
use chrono::{DateTime, Utc};
use clap::Args;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use bigdecimal::BigDecimal;

#[derive(Args)]
pub struct SimulateArgs {
    /// シミュレーション開始日 (YYYY-MM-DD)
    #[clap(short, long)]
    pub start: Option<String>,

    /// シミュレーション終了日 (YYYY-MM-DD)
    #[clap(short, long)]
    pub end: Option<String>,

    /// 使用するアルゴリズム [デフォルト: momentum]
    #[clap(short, long, default_value = "momentum")]
    pub algorithm: String,

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

    /// リバランス頻度 [デフォルト: daily]
    #[clap(long, default_value = "daily")]
    pub rebalance_freq: String,

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

    /// レポート形式 [デフォルト: json]
    #[clap(long, default_value = "json")]
    pub report_format: String,

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
    pub rebalance_frequency: RebalanceFrequency,
    pub fee_model: FeeModel,
    pub slippage_rate: f64,
    pub gas_cost: BigDecimal,
    pub min_trade_amount: BigDecimal,
    pub prediction_horizon: chrono::Duration,
    pub historical_days: i64,  // 予測に使用する過去データ期間
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlgorithmType {
    Momentum,
    Portfolio,
    TrendFollowing,
}

#[derive(Debug, Clone)]
pub enum RebalanceFrequency {
    Hourly,
    Daily,
    Weekly,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub config: SimulationSummary,
    pub performance: PerformanceMetrics,
    pub trades: Vec<TradeExecution>,
    pub portfolio_values: Vec<PortfolioValue>,
    pub execution_summary: ExecutionSummary,
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

impl From<&str> for RebalanceFrequency {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "hourly" => RebalanceFrequency::Hourly,
            "daily" => RebalanceFrequency::Daily,
            "weekly" => RebalanceFrequency::Weekly,
            _ => RebalanceFrequency::Daily, // デフォルト
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
    
    if args.verbose {
        println!("📋 Configuration:");
        println!("  Algorithm: {}", args.algorithm);
        println!("  Capital: {} {}", args.capital, args.quote_token);
        println!("  Fee Model: {}", args.fee_model);
        println!("  Output: {}", args.output);
    }

    // 1. 設定の検証と変換
    let config = validate_and_convert_args(args).await?;
    
    if config.target_tokens.is_empty() {
        return Err(anyhow::anyhow!("No target tokens specified"));
    }

    println!("📊 Simulation period: {} to {}", 
        config.start_date.format("%Y-%m-%d"),
        config.end_date.format("%Y-%m-%d"));
    println!("🎯 Target tokens: {:?}", config.target_tokens);

    // 2. 簡単なbuy-and-holdシミュレーション（Phase 1実装）
    let result = run_buy_and_hold_simulation(&config).await?;

    // 3. 結果の保存
    save_simulation_result(&result, &config).await?;

    println!("✅ Simulation completed!");
    println!("📈 Total Return: {:.2}%", result.performance.total_return_pct);
    println!("📊 Final Value: {:.2} {}", result.config.final_value, config.quote_token);

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
        tokens_str.split(',').map(|s| s.trim().to_string()).collect()
    } else {
        // TODO: 自動でtop volatility tokensを取得
        vec!["usdc.tether-token.near".to_string()] // 暫定的にUSDCを使用
    };

    Ok(SimulationConfig {
        start_date,
        end_date,
        algorithm: AlgorithmType::from(args.algorithm.as_str()),
        initial_capital: BigDecimal::from_str(&args.capital.to_string())?,
        quote_token: args.quote_token,
        target_tokens,
        rebalance_frequency: RebalanceFrequency::from(args.rebalance_freq.as_str()),
        fee_model: FeeModel::from((args.fee_model.as_str(), args.custom_fee)),
        slippage_rate: args.slippage,
        gas_cost: BigDecimal::from_str(&args.gas_cost.to_string())?,
        min_trade_amount: BigDecimal::from_str(&args.min_trade.to_string())?,
        prediction_horizon: chrono::Duration::hours(args.prediction_horizon as i64),
        historical_days: args.historical_days as i64,
    })
}

async fn run_buy_and_hold_simulation(config: &SimulationConfig) -> Result<SimulationResult> {
    println!("💰 Running buy-and-hold simulation for token: {}", config.target_tokens[0]);

    // 暫定的なbuy-and-hold実装
    // Phase 1では簡単な計算のみ行う
    let duration = config.end_date - config.start_date;
    let duration_days = duration.num_days();
    
    // 暫定的な価格変動（実際の実装では実データを取得）
    let mock_return = 0.15; // 15%のリターンと仮定
    let initial_value = config.initial_capital.to_string().parse::<f64>().unwrap_or(1000.0);
    let final_value = initial_value * (1.0 + mock_return);

    // 簡単なパフォーマンス指標
    let performance = PerformanceMetrics {
        total_return: mock_return,
        annualized_return: mock_return * 365.0 / duration_days as f64,
        total_return_pct: mock_return * 100.0,
        volatility: 0.25, // 25%と仮定
        max_drawdown: -0.1, // -10%と仮定
        max_drawdown_pct: -10.0,
        sharpe_ratio: 0.8,
        sortino_ratio: 1.2,
        total_trades: 1, // buy-and-holdなので1取引のみ
        winning_trades: 1,
        losing_trades: 0,
        win_rate: 1.0,
        profit_factor: 0.0, // buy-and-holdでは該当しない
        total_costs: 30.0, // 仮の取引コスト
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
        trades: vec![], // Phase 1では空
        portfolio_values: vec![], // Phase 1では空
        execution_summary,
    })
}

async fn save_simulation_result(result: &SimulationResult, config: &SimulationConfig) -> Result<()> {
    use crate::utils::file::ensure_directory_exists;
    use std::path::PathBuf;

    // 出力ディレクトリの作成
    let base_dir = std::env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
    let output_dir = PathBuf::from(&base_dir)
        .join("simulation_results")
        .join(format!("{}_{}_{}", 
            format!("{:?}", config.algorithm).to_lowercase(),
            config.start_date.format("%Y-%m-%d"),
            config.end_date.format("%Y-%m-%d")
        ));

    ensure_directory_exists(&output_dir)?;

    // 結果をJSONファイルに保存
    let result_file = output_dir.join("results.json");
    let json_content = serde_json::to_string_pretty(result)?;
    std::fs::write(&result_file, json_content)?;

    println!("💾 Results saved to: {}", result_file.display());

    Ok(())
}