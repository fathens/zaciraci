use clap::{Parser, Subcommand};
use common::algorithm::portfolio::{ParsePredErrDiagonalModeError, PredErrDiagonalMode};
use std::path::PathBuf;

fn parse_pred_err_diagonal_mode(
    s: &str,
) -> Result<PredErrDiagonalMode, ParsePredErrDiagonalModeError> {
    s.parse()
}

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

    /// Enable per-token bias correction (improvement C). Defaults to false to
    /// match the production config (commit reverted in 2026-05); pass
    /// `--bias-correction true` for A/B comparison runs.
    #[arg(long, action = clap::ArgAction::Set, default_value_t = false)]
    pub bias_correction: bool,

    /// Enable prediction-error variance diagonal inflation (improvement 3).
    /// Defaults to false; pass `--pred-err-diagonal true` for A/B comparison.
    #[arg(long, action = clap::ArgAction::Set, default_value_t = false)]
    pub pred_err_diagonal: bool,

    /// Scale factor `k` for the diagonal inflation rule (default 1.0)
    #[arg(long, default_value = "1.0")]
    pub pred_err_diagonal_k: f64,

    /// Diagonal composition mode: "additive" or "max"
    #[arg(long, default_value = "max", value_parser = parse_pred_err_diagonal_mode)]
    pub pred_err_diagonal_mode: PredErrDiagonalMode,

    /// Enable cost-aware iterative optimization (improvement D). Defaults to
    /// false to match the production config; pass `--cost-aware-return true`
    /// for A/B comparison runs.
    #[arg(long, action = clap::ArgAction::Set, default_value_t = false)]
    pub cost_aware_return: bool,

    /// Maximum iterations for cost-aware optimization (default 3)
    #[arg(long, default_value = "3")]
    pub cost_iterations_max: u32,

    /// Use all-predicted-token + held-tokens union as the candidate set on
    /// every cycle instead of locking in the top-N volatility tokens at
    /// period start. Defaults to false (legacy fixed-set behavior); pass
    /// `--all-predicted true` to compare against the legacy baseline.
    #[arg(long, action = clap::ArgAction::Set, default_value_t = false)]
    pub all_predicted: bool,

    /// Cap on the candidate count fed to the portfolio optimizer in
    /// all-token mode. `0` disables Top-N pruning (every candidate that
    /// survives confidence + liquidity filters reaches the optimizer);
    /// `> 0` keeps the top N by composite score plus all held tokens.
    /// Has no effect when `--all-predicted false`.
    #[arg(long, default_value = "0")]
    pub top_n_after_prediction: u32,

    /// Soft-threshold shrinkage strength applied to expected returns:
    /// `μ_adj = sign(μ) × max(0, |μ| - λ × √MSRE)`. `0.0` disables
    /// shrinkage (identical to the legacy behavior); typical production
    /// range is `[0.05, 0.3]`. Clamped to `[0.0, 1.0]` at the typed-config
    /// layer.
    #[arg(long, default_value = "0.0")]
    pub shrinkage_lambda: f64,

    /// PR-A Phase 1: enable volatility targeting (Moreira & Muir 2017).
    /// Adds a `Volatility(cap)` signal where `cap = σ_target / σ_portfolio`.
    #[arg(long, action = clap::ArgAction::Set, default_value_t = false)]
    pub vol_target: bool,

    /// PR-A Phase 2: enable market-breadth regime detection.
    /// Adds a `Breadth(cap)` signal based on the fraction of tokens above
    /// their own SMA(20).
    #[arg(long, action = clap::ArgAction::Set, default_value_t = false)]
    pub regime_breadth: bool,

    /// PR-A Phase 3a: enable per-asset half-Kelly upper bound.
    /// Tightens BoxBounds via `apply_half_kelly` using the typed-config
    /// fraction (default Quarter Kelly, 0.25).
    #[arg(long, action = clap::ArgAction::Set, default_value_t = false)]
    pub half_kelly: bool,

    /// PR-A Phase 3b: enable per-token stop-loss override.
    /// Zeroes out the optimizer weight for any held token whose realised
    /// drawdown exceeds the typed-config threshold (default 10%).
    #[arg(long, action = clap::ArgAction::Set, default_value_t = false)]
    pub stop_loss: bool,

    /// PR-B: enable period-mid drawdown circuit breaker.
    /// Triggers a force-liquidate when current portfolio value falls more
    /// than `--dd-threshold` below the period's initial value.
    #[arg(long, action = clap::ArgAction::Set, default_value_t = false)]
    pub dd_circuit_breaker: bool,
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
            bias_correction: true,
            pred_err_diagonal: true,
            pred_err_diagonal_k: 1.0,
            pred_err_diagonal_mode: PredErrDiagonalMode::Max,
            cost_aware_return: true,
            cost_iterations_max: 3,
            all_predicted: false,
            top_n_after_prediction: 0,
            shrinkage_lambda: 0.0,
            vol_target: false,
            regime_breadth: false,
            half_kelly: false,
            stop_loss: false,
            dd_circuit_breaker: false,
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
    fn cli_accepts_valid_pred_err_diagonal_mode() {
        let cli = Cli::try_parse_from([
            "simulate",
            "run",
            "--start-date",
            "2025-01-01",
            "--end-date",
            "2025-01-02",
            "--pred-err-diagonal-mode",
            "additive",
        ])
        .unwrap();
        let Command::Run(args) = cli.command else {
            panic!("expected Run subcommand");
        };
        assert_eq!(args.pred_err_diagonal_mode, PredErrDiagonalMode::Additive);
    }

    #[test]
    fn cli_rejects_pred_err_diagonal_mode_typo() {
        let err = Cli::try_parse_from([
            "simulate",
            "run",
            "--start-date",
            "2025-01-01",
            "--end-date",
            "2025-01-02",
            "--pred-err-diagonal-mode",
            "addative",
        ])
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("invalid PredErrDiagonalMode") || msg.contains("addative"),
            "expected typo error, got: {msg}"
        );
    }

    #[test]
    fn verify_parse_invalid_date() {
        let args = make_verify_args("bad", "2025-06-30");
        assert!(args.parse_start_date().is_err());
    }
}
