use crate::api::backend::BackendClient;
use anyhow::Result;
use bigdecimal::BigDecimal;
use bigdecimal::FromPrimitive;
use chrono::{DateTime, Utc};
use clap::Args;
use common::stats::ValueAtTime;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Momentum アルゴリズム関連の構造体と定数

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionData {
    pub token: String,
    pub current_price: BigDecimal,
    pub predicted_price_24h: BigDecimal,
    pub timestamp: DateTime<Utc>,
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TradingAction {
    Hold,
    Sell { token: String, target: String },
    Switch { from: String, to: String },
}

const MIN_PROFIT_THRESHOLD: f64 = 0.05;
const SWITCH_MULTIPLIER: f64 = 1.5;
const TOP_N_TOKENS: usize = 3;

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
    pub historical_days: i64, // 予測に使用する過去データ期間
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

    println!(
        "📊 Simulation period: {} to {}",
        config.start_date.format("%Y-%m-%d"),
        config.end_date.format("%Y-%m-%d")
    );
    println!("🎯 Target tokens: {:?}", config.target_tokens);

    // 2. 簡単なbuy-and-holdシミュレーション（Phase 1実装）
    let result = run_buy_and_hold_simulation(&config).await?;

    // 3. 結果の保存
    save_simulation_result(&result, &config).await?;

    println!("✅ Simulation completed!");
    println!(
        "📈 Total Return: {:.2}%",
        result.performance.total_return_pct
    );
    println!(
        "📊 Final Value: {:.2} {}",
        result.config.final_value, config.quote_token
    );

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
            "No price data available for simulation period"
        ));
    }

    // 2. Momentumアルゴリズムでシミュレーション実行
    let simulation_result = run_momentum_timestep_simulation(config, &price_data).await?;

    Ok(simulation_result)
}

async fn run_portfolio_simulation(config: &SimulationConfig) -> Result<SimulationResult> {
    println!("📊 Running portfolio optimization simulation");
    // TODO: Portfolio最適化アルゴリズムの実装
    run_simple_simulation(config).await
}

async fn run_trend_following_simulation(config: &SimulationConfig) -> Result<SimulationResult> {
    println!("📉 Running trend following simulation");
    // TODO: Trend followingアルゴリズムの実装
    run_simple_simulation(config).await
}

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

async fn save_simulation_result(
    result: &SimulationResult,
    config: &SimulationConfig,
) -> Result<()> {
    use crate::utils::file::ensure_directory_exists;
    use std::path::PathBuf;

    // 出力ディレクトリの作成
    let base_dir = std::env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
    let output_dir = PathBuf::from(&base_dir)
        .join("simulation_results")
        .join(format!(
            "{}_{}_{}",
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

        let values = match backend_client
            .get_price_history(
                token,
                &config.quote_token,
                data_start_date.naive_utc(),
                data_end_date.naive_utc(),
            )
            .await
        {
            Ok(values) => values,
            Err(e) => {
                println!("  ⚠️ Failed to fetch real data for {}: {}", token, e);
                println!("  🔧 Generating mock data for testing");
                generate_mock_price_data(data_start_date, data_end_date)?
            }
        };

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
    let time_step = match config.rebalance_frequency {
        RebalanceFrequency::Hourly => chrono::Duration::hours(1),
        RebalanceFrequency::Daily => chrono::Duration::days(1),
        RebalanceFrequency::Weekly => chrono::Duration::days(7),
    };

    let mut current_time = config.start_date;
    let mut portfolio_values = Vec::new();
    let mut trades = Vec::new();
    let mut current_holdings = HashMap::new();

    // 初期ポートフォリオ設定（均等分散）
    let tokens_count = config.target_tokens.len() as f64;
    let initial_per_token = initial_value / tokens_count;

    for token in &config.target_tokens {
        current_holdings.insert(token.clone(), initial_per_token);
    }

    let mut step_count = 0;
    let max_steps = 1000; // 無限ループ防止

    while current_time <= config.end_date && step_count < max_steps {
        step_count += 1;

        // 現在時点での価格を取得
        let current_prices = get_prices_at_time(price_data, current_time)?;

        // 過去データから予測を生成
        let predictions = generate_momentum_predictions(
            price_data,
            &config.target_tokens,
            current_time,
            config.historical_days,
            config.prediction_horizon,
        )?;

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

            // 取引決定
            let action = make_trading_decision(
                &token,
                current_return,
                &ranked_tokens,
                &BigDecimal::from(amount as i64),
            );

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

/// 指定時刻での価格を取得
fn get_prices_at_time(
    price_data: &HashMap<String, Vec<ValueAtTime>>,
    target_time: DateTime<Utc>,
) -> Result<HashMap<String, f64>> {
    let mut prices = HashMap::new();

    for (token, values) in price_data {
        // target_time に最も近い価格データを見つける
        let closest_value = values.iter().min_by_key(|v| {
            let value_time: DateTime<Utc> = DateTime::from_naive_utc_and_offset(v.time, Utc);
            (value_time - target_time).num_seconds().abs()
        });

        if let Some(value) = closest_value {
            prices.insert(token.clone(), value.value);
        }
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
    let winning_trades = trades
        .iter()
        .filter(|t| t.portfolio_value_after > t.portfolio_value_before)
        .count();
    let losing_trades = trades.len() - winning_trades;
    let win_rate = if trades.is_empty() {
        0.0
    } else {
        winning_trades as f64 / trades.len() as f64
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
        winning_trades,
        losing_trades,
        win_rate,
        profit_factor: 0.0, // TODO: 実装
        total_costs,
        cost_ratio,
        simulation_days: duration_days,
        active_trading_days: if trades.is_empty() { 0 } else { duration_days },
    }
}

/// Momentumアルゴリズム用の予測を生成
fn generate_momentum_predictions(
    price_data: &HashMap<String, Vec<ValueAtTime>>,
    target_tokens: &[String],
    current_time: DateTime<Utc>,
    historical_days: i64,
    prediction_horizon: chrono::Duration,
) -> Result<Vec<PredictionData>> {
    let mut predictions = Vec::new();

    let history_start = current_time - chrono::Duration::days(historical_days);
    let prediction_target = current_time + prediction_horizon;

    for token in target_tokens {
        if let Some(token_data) = price_data.get(token) {
            // 履歴データの取得
            let historical_data: Vec<&ValueAtTime> = token_data
                .iter()
                .filter(|v| {
                    let value_time: DateTime<Utc> =
                        DateTime::from_naive_utc_and_offset(v.time, Utc);
                    value_time >= history_start && value_time <= current_time
                })
                .collect();

            if historical_data.len() < 2 {
                continue;
            }

            // 現在価格
            let current_price = historical_data.last().map(|v| v.value).unwrap_or(0.0);

            // シンプルなトレンド予測（直線外挿）
            let predicted_price = if historical_data.len() >= 5 {
                predict_price_trend(&historical_data, prediction_target)?
            } else {
                current_price // データ不足の場合は現在価格のまま
            };

            // 信頼度計算（データ量と直近のボラティリティに基づく）
            let confidence = calculate_prediction_confidence(&historical_data);

            predictions.push(PredictionData {
                token: token.clone(),
                current_price: BigDecimal::from_f64(current_price).unwrap_or_default(),
                predicted_price_24h: BigDecimal::from_f64(predicted_price).unwrap_or_default(),
                timestamp: current_time,
                confidence: Some(confidence),
            });
        }
    }

    Ok(predictions)
}

/// 価格トレンドを予測
fn predict_price_trend(
    historical_data: &[&ValueAtTime],
    _target_time: DateTime<Utc>,
) -> Result<f64> {
    if historical_data.len() < 2 {
        return Ok(0.0);
    }

    // 直近5データポイントの平均変化率を使用
    let recent_data = &historical_data[historical_data.len().saturating_sub(5)..];

    if recent_data.len() < 2 {
        return Ok(recent_data.last().unwrap().value);
    }

    let mut total_return = 0.0;
    let mut count = 0;

    for i in 1..recent_data.len() {
        let prev = recent_data[i - 1].value;
        let curr = recent_data[i].value;

        if prev > 0.0 {
            let return_rate = (curr - prev) / prev;
            total_return += return_rate;
            count += 1;
        }
    }

    if count == 0 {
        return Ok(recent_data.last().unwrap().value);
    }

    let avg_return = total_return / count as f64;
    let current_price = recent_data.last().unwrap().value;

    // 予測価格 = 現在価格 * (1 + 平均リターン)
    Ok(current_price * (1.0 + avg_return))
}

/// 予測の信頼度を計算
fn calculate_prediction_confidence(historical_data: &[&ValueAtTime]) -> f64 {
    if historical_data.len() < 2 {
        return 0.1;
    }

    // データ量に基づく基本信頼度
    let data_confidence = (historical_data.len() as f64 / 30.0).min(1.0);

    // ボラティリティに基づく調整
    let prices: Vec<f64> = historical_data.iter().map(|v| v.value).collect();
    let volatility = calculate_simple_volatility(&prices);

    // 高ボラティリティは信頼度を下げる
    let volatility_factor = if volatility > 0.5 {
        0.5
    } else {
        1.0 - volatility
    };

    (data_confidence * volatility_factor).clamp(0.1, 0.9)
}

/// シンプルなボラティリティ計算
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
fn calculate_trading_cost(
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

// Momentum アルゴリズム関数

fn calculate_confidence_adjusted_return(prediction: &PredictionData) -> f64 {
    let current = prediction
        .current_price
        .to_string()
        .parse::<f64>()
        .unwrap_or(0.0);
    let predicted = prediction
        .predicted_price_24h
        .to_string()
        .parse::<f64>()
        .unwrap_or(0.0);

    if current == 0.0 {
        return 0.0;
    }

    let raw_return = (predicted - current) / current;
    let confidence = prediction.confidence.unwrap_or(0.5);

    // 取引コストを考慮し、信頼度で調整
    (raw_return - 0.006 - 0.02) * confidence // 0.6% fee + 2% slippage
}

fn rank_tokens_by_momentum(predictions: Vec<PredictionData>) -> Vec<(String, f64, Option<f64>)> {
    let mut ranked: Vec<_> = predictions
        .iter()
        .map(|p| {
            let return_val = calculate_confidence_adjusted_return(p);
            (p.token.clone(), return_val, p.confidence)
        })
        .filter(|(_, return_val, _)| *return_val > 0.0)
        .collect();

    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(TOP_N_TOKENS);
    ranked
}

fn make_trading_decision(
    current_token: &str,
    current_return: f64,
    ranked_tokens: &[(String, f64, Option<f64>)],
    holding_amount: &BigDecimal,
) -> TradingAction {
    if ranked_tokens.is_empty() {
        return TradingAction::Hold;
    }

    let best_token = &ranked_tokens[0];

    if best_token.0 == current_token {
        return TradingAction::Hold;
    }

    let amount = holding_amount.to_string().parse::<f64>().unwrap_or(0.0);
    if amount < 1.0 {
        // MIN_TRADE_AMOUNT
        return TradingAction::Hold;
    }

    if current_return < MIN_PROFIT_THRESHOLD {
        return TradingAction::Sell {
            token: current_token.to_string(),
            target: best_token.0.clone(),
        };
    }

    let confidence_factor = best_token.2.unwrap_or(0.5);
    if best_token.1 > current_return * SWITCH_MULTIPLIER * confidence_factor {
        return TradingAction::Switch {
            from: current_token.to_string(),
            to: best_token.0.clone(),
        };
    }

    TradingAction::Hold
}

/// モック価格データを生成（テスト用）
fn generate_mock_price_data(
    start_date: DateTime<Utc>,
    end_date: DateTime<Utc>,
) -> Result<Vec<ValueAtTime>> {
    use rand::Rng;

    let mut rng = rand::thread_rng();
    let mut values = Vec::new();
    let mut current_time = start_date;
    let mut current_price: f64 = 1.0; // 初期価格

    while current_time <= end_date {
        // ランダムウォーク（±2%の変動）
        let change = rng.gen_range(-0.02..0.02);
        current_price *= 1.0 + change;
        current_price = current_price.max(0.1); // 最低価格0.1

        values.push(ValueAtTime {
            time: current_time.naive_utc(),
            value: current_price,
        });

        // 1時間毎のデータを生成
        current_time += chrono::Duration::hours(1);
    }

    Ok(values)
}
