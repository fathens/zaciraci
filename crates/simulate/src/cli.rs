use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(name = "simulate", about = "Auto trade backtest simulation")]
pub struct Cli {
    /// Simulation start date (YYYY-MM-DD)
    #[arg(long)]
    pub start_date: String,

    /// Simulation end date (YYYY-MM-DD)
    #[arg(long)]
    pub end_date: String,

    /// Initial capital in NEAR
    #[arg(long, default_value = "100")]
    pub initial_capital: f64,

    /// Number of top volatility tokens to select
    #[arg(long, default_value = "10")]
    pub top_tokens: usize,

    /// Days for volatility calculation
    #[arg(long, default_value = "7")]
    pub volatility_days: i64,

    /// Days of price history for prediction
    #[arg(long, default_value = "30")]
    pub price_history_days: i64,

    /// Rebalance threshold (0.0-1.0)
    #[arg(long, default_value = "0.1")]
    pub rebalance_threshold: f64,

    /// Days between rebalance attempts
    #[arg(long, default_value = "1")]
    pub rebalance_interval_days: i64,

    /// Output file path for results JSON
    #[arg(long, default_value = "simulation_result.json")]
    pub output: PathBuf,

    /// Sweep config file (JSON) for parameter sweep mode
    #[arg(long)]
    pub sweep: Option<PathBuf>,
}

impl Cli {
    pub fn parse_start_date(&self) -> anyhow::Result<chrono::NaiveDate> {
        chrono::NaiveDate::parse_from_str(&self.start_date, "%Y-%m-%d")
            .map_err(|e| anyhow::anyhow!("Invalid start-date '{}': {}", self.start_date, e))
    }

    pub fn parse_end_date(&self) -> anyhow::Result<chrono::NaiveDate> {
        chrono::NaiveDate::parse_from_str(&self.end_date, "%Y-%m-%d")
            .map_err(|e| anyhow::anyhow!("Invalid end-date '{}': {}", self.end_date, e))
    }
}
