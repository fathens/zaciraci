use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "simulate", about = "Auto trade backtest simulation")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Run backtest simulation
    Run(RunArgs),
    /// Verify simulation accuracy against real trades
    Verify(VerifyArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct RunArgs {
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

    /// Generate and evaluate predictions for the simulation period before running
    #[arg(long)]
    pub generate_predictions: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct VerifyArgs {
    /// Analysis start date (YYYY-MM-DD)
    #[arg(long)]
    pub start_date: String,

    /// Analysis end date (YYYY-MM-DD)
    #[arg(long)]
    pub end_date: String,

    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

fn parse_date(s: &str, label: &str) -> anyhow::Result<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|e| anyhow::anyhow!("Invalid {} '{}': {}", label, s, e))
}

impl RunArgs {
    pub fn parse_start_date(&self) -> anyhow::Result<chrono::NaiveDate> {
        parse_date(&self.start_date, "start-date")
    }

    pub fn parse_end_date(&self) -> anyhow::Result<chrono::NaiveDate> {
        parse_date(&self.end_date, "end-date")
    }
}

impl VerifyArgs {
    pub fn parse_start_date(&self) -> anyhow::Result<chrono::NaiveDate> {
        parse_date(&self.start_date, "start-date")
    }

    pub fn parse_end_date(&self) -> anyhow::Result<chrono::NaiveDate> {
        parse_date(&self.end_date, "end-date")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_run_args(start: &str, end: &str) -> RunArgs {
        RunArgs {
            start_date: start.to_string(),
            end_date: end.to_string(),
            initial_capital: 100.0,
            top_tokens: 10,
            price_history_days: 30,
            rebalance_threshold: 0.1,
            rebalance_interval_days: 1,
            output: PathBuf::from("test.json"),
            sweep: None,
            generate_predictions: false,
        }
    }

    #[test]
    fn parse_valid_start_date() {
        let args = make_run_args("2025-06-01", "2025-12-31");
        let date = args.parse_start_date().unwrap();
        assert_eq!(date, chrono::NaiveDate::from_ymd_opt(2025, 6, 1).unwrap());
    }

    #[test]
    fn parse_valid_end_date() {
        let args = make_run_args("2025-06-01", "2025-12-31");
        let date = args.parse_end_date().unwrap();
        assert_eq!(date, chrono::NaiveDate::from_ymd_opt(2025, 12, 31).unwrap());
    }

    #[test]
    fn parse_invalid_start_date() {
        let args = make_run_args("not-a-date", "2025-12-31");
        let err = args.parse_start_date().unwrap_err();
        assert!(err.to_string().contains("Invalid start-date"));
    }

    #[test]
    fn parse_invalid_end_date() {
        let args = make_run_args("2025-06-01", "31-12-2025");
        let err = args.parse_end_date().unwrap_err();
        assert!(err.to_string().contains("Invalid end-date"));
    }

    #[test]
    fn parse_empty_date() {
        let args = make_run_args("", "2025-12-31");
        assert!(args.parse_start_date().is_err());
    }

    fn make_verify_args(start: &str, end: &str) -> VerifyArgs {
        VerifyArgs {
            start_date: start.to_string(),
            end_date: end.to_string(),
            format: OutputFormat::Text,
        }
    }

    #[test]
    fn verify_parse_valid_dates() {
        let args = make_verify_args("2025-01-01", "2025-06-30");
        assert!(args.parse_start_date().is_ok());
        assert!(args.parse_end_date().is_ok());
    }

    #[test]
    fn verify_parse_invalid_date() {
        let args = make_verify_args("bad", "2025-06-30");
        assert!(args.parse_start_date().is_err());
    }
}
