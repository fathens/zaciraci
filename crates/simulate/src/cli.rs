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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cli(start: &str, end: &str) -> Cli {
        Cli {
            start_date: start.to_string(),
            end_date: end.to_string(),
            initial_capital: 100.0,
            top_tokens: 10,
            volatility_days: 7,
            price_history_days: 30,
            rebalance_threshold: 0.1,
            rebalance_interval_days: 1,
            output: PathBuf::from("test.json"),
            sweep: None,
        }
    }

    #[test]
    fn parse_valid_start_date() {
        let cli = make_cli("2025-06-01", "2025-12-31");
        let date = cli.parse_start_date().unwrap();
        assert_eq!(date, chrono::NaiveDate::from_ymd_opt(2025, 6, 1).unwrap());
    }

    #[test]
    fn parse_valid_end_date() {
        let cli = make_cli("2025-06-01", "2025-12-31");
        let date = cli.parse_end_date().unwrap();
        assert_eq!(date, chrono::NaiveDate::from_ymd_opt(2025, 12, 31).unwrap());
    }

    #[test]
    fn parse_invalid_start_date() {
        let cli = make_cli("not-a-date", "2025-12-31");
        let err = cli.parse_start_date().unwrap_err();
        assert!(err.to_string().contains("Invalid start-date"));
    }

    #[test]
    fn parse_invalid_end_date() {
        let cli = make_cli("2025-06-01", "31-12-2025");
        let err = cli.parse_end_date().unwrap_err();
        assert!(err.to_string().contains("Invalid end-date"));
    }

    #[test]
    fn parse_empty_date() {
        let cli = make_cli("", "2025-12-31");
        assert!(cli.parse_start_date().is_err());
    }
}
