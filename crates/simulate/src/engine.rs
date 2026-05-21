use crate::cli::RunArgs;
use crate::mock_client::SimulationClient;
use crate::mock_wallet::SimulationWallet;
use crate::output::SimulationResult;
use crate::portfolio_state::{DbRateProvider, PortfolioState};
use anyhow::Result;
use bigdecimal::BigDecimal;
use chrono::{NaiveTime, TimeZone, Utc};
use common::types::YoctoValue;
use logging::*;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn run_simulation(cli: &RunArgs) -> Result<SimulationResult> {
    let log = DEFAULT.new(o!("function" => "run_simulation"));

    let start_date = cli.parse_start_date()?;
    let end_date = cli.parse_end_date()?;

    if start_date >= end_date {
        return Err(anyhow::anyhow!(
            "start-date must be before end-date: {} >= {}",
            start_date,
            end_date
        ));
    }

    // Apply CLI parameters to config
    apply_config(cli);

    // Generate predictions if requested
    if cli.generate_predictions {
        let cfg = common::config::ConfigResolver;
        info!(log, "generating predictions for simulation period");
        crate::prediction::generate_predictions_for_range(start_date, end_date, &cfg).await?;
    }

    // Initialize token decimals cache from DB
    if let Err(e) = trade::token_cache::load_from_db().await {
        warn!(log, "failed to load token decimals cache"; "error" => ?e);
    }

    // Convert initial capital NEAR -> yoctoNEAR (via BigDecimal for precision)
    let initial_capital_yocto = {
        let capital = BigDecimal::from_str(&cli.initial_capital.to_string())
            .unwrap_or_else(|_| BigDecimal::from(cli.initial_capital as i64));
        let yocto_per_near = BigDecimal::from(10u128.pow(24));
        YoctoValue::from_yocto(&capital * &yocto_per_near)
    };

    // Initialize portfolio state
    let portfolio = Arc::new(Mutex::new(PortfolioState::new(
        initial_capital_yocto.clone(),
    )));

    // Shared simulation date (updated each iteration, read by SimulationClient)
    let sim_day_shared = Arc::new(Mutex::new(Utc::now()));

    // Create mock client and wallet
    let sim_client = SimulationClient::new(
        Arc::clone(&portfolio),
        initial_capital_yocto,
        Arc::clone(&sim_day_shared),
    );
    let sim_wallet = SimulationWallet::new();

    info!(log, "starting simulation";
        "start_date" => %start_date,
        "end_date" => %end_date,
        "initial_capital" => cli.initial_capital,
        "rebalance_interval_days" => cli.rebalance_interval_days,
    );

    // Simulation loop: step through dates
    let mut current_date = start_date;
    let mut day_count = 0u32;

    while current_date <= end_date {
        // Locate sim_day at the moment a fresh prediction first becomes visible
        // on this date. Production's daily cron fires at midnight but the trade
        // cycle only succeeds once the day's prediction lands in the DB
        // (typically 00:00–00:42 UTC); pinning sim_day to midnight would
        // emulate a state production never traded from. Using the actual
        // earliest-fresh-prediction timestamp keeps simulate's clock in lock
        // step with the data production actually saw at runtime, including the
        // late-prediction days where production had to wait.
        let day_start = current_date.and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        let day_end = (current_date + chrono::TimeDelta::days(1))
            .and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        let earliest = persistence::prediction_record::PredictionRecord::earliest_fresh_visible_in(
            day_start, day_end,
        )
        .await?;

        let sim_day = match earliest {
            Some(t) => Utc.from_utc_datetime(&t),
            None => {
                info!(log, "skipping day: no fresh predictions available";
                    "date" => %current_date, "day" => day_count);
                current_date += chrono::TimeDelta::days(cli.rebalance_interval_days);
                day_count += 1;
                continue;
            }
        };

        // Update shared simulation date so SimulationClient uses correct rates
        *sim_day_shared.lock().await = sim_day;

        info!(log, "simulation day"; "date" => %current_date, "day" => day_count, "sim_day" => %sim_day);

        // Execute the full trading cycle via trade::strategy::start
        let cfg = common::config::ConfigResolver;
        if let Err(e) = trade::strategy::start(&sim_client, &sim_wallet, sim_day, &cfg).await {
            warn!(log, "trading cycle failed"; "date" => %current_date, "error" => ?e);
        }

        // Record snapshot
        {
            let rate_provider = DbRateProvider;
            let mut state = portfolio.lock().await;
            if let Err(e) = state.record_snapshot(sim_day, &rate_provider).await {
                warn!(log, "failed to record snapshot"; "date" => %current_date, "error" => ?e);
            }
        }

        current_date += chrono::TimeDelta::days(cli.rebalance_interval_days);
        day_count += 1;
    }

    // Liquidate all remaining holdings
    {
        let rate_provider = DbRateProvider;
        let mut state = portfolio.lock().await;

        let end_day =
            Utc.from_utc_datetime(&end_date.and_time(NaiveTime::from_hms_opt(23, 59, 59).unwrap()));
        if let Err(e) = state.liquidate_all(end_day, &rate_provider).await {
            warn!(log, "failed to liquidate"; "error" => ?e);
        }
        if let Err(e) = state.record_snapshot(end_day, &rate_provider).await {
            warn!(log, "failed to record final snapshot"; "error" => ?e);
        }
    }

    // Build result
    let state = portfolio.lock().await;
    let result = SimulationResult::from_state(cli, &state)?;

    info!(log, "simulation completed";
        "days" => day_count,
        "total_return" => result.performance.total_return,
        "sharpe_ratio" => result.performance.sharpe_ratio,
    );

    Ok(result)
}

/// Apply CLI parameters to the config system.
///
/// # Boundary assumptions (read before reusing this function)
///
/// This function mutates **process-wide** config state via
/// `common::config::store::set`, which is backed by a process-shared
/// `OnceLock`. The simulate binary is designed to run in **isolation** with
/// these assumptions:
///
/// 1. **Mock client only.** All trading paths must be routed through
///    `SimulationClient` / `SimulationWallet`; setting `TRADE_ENABLED=true`
///    here is safe *only* because no real RPC calls are issued. Mixing this
///    with a production `near-jsonrpc-client` in the same process would
///    enable live trades unintentionally.
/// 2. **No coexistence with the production binary.** The `OnceLock`-backed
///    overrides written here persist for the lifetime of the process and are
///    visible to every consumer of `common::config::ConfigResolver`. Running
///    simulate alongside production trading code (or running multiple
///    simulations concurrently in the same process) would cross-contaminate
///    config state.
/// 3. **One-shot per process.** Sweep / parameter-grid runs that need
///    different configs must be executed as separate child processes.
///
/// # TODO: lift these constraints
///
/// - **Mid-term:** introduce `common::config::store::with_scoped(...)` as an
///   RAII guard that pushes overrides on entry and pops them on drop, so
///   `apply_config` can be made scope-local.
/// - **Long-term:** replace process-wide config reads with explicit
///   dependency injection (e.g. a simulate-specific `ConfigSnapshot` passed
///   into the engine, the trade strategy, and the prediction generator).
///
/// Until those land, treat `apply_config` as a hard process boundary.
pub(crate) fn apply_config(cli: &RunArgs) {
    common::config::store::set("TRADE_TOP_TOKENS", &cli.top_tokens.to_string());
    common::config::store::set(
        "TRADE_PRICE_HISTORY_DAYS",
        &cli.price_history_days.to_string(),
    );
    common::config::store::set(
        "PORTFOLIO_REBALANCE_THRESHOLD",
        &cli.rebalance_threshold.to_string(),
    );
    common::config::store::set("TRADE_INITIAL_INVESTMENT", &cli.initial_capital.to_string());
    // Enable trading (mock client prevents real transactions)
    common::config::store::set("TRADE_ENABLED", "true");

    // Improvement flags (default off; CLI flips them per A/B run)
    common::config::store::set(
        "TRADE_BIAS_CORRECTION_ENABLED",
        &cli.bias_correction.to_string(),
    );
    common::config::store::set(
        "PORTFOLIO_PRED_ERR_DIAGONAL_ENABLED",
        &cli.pred_err_diagonal.to_string(),
    );
    common::config::store::set(
        "PORTFOLIO_PRED_ERR_DIAGONAL_K",
        &cli.pred_err_diagonal_k.to_string(),
    );
    common::config::store::set(
        "PORTFOLIO_PRED_ERR_DIAGONAL_MODE",
        cli.pred_err_diagonal_mode.as_str(),
    );
    common::config::store::set(
        "TRADE_COST_AWARE_RETURN_ENABLED",
        &cli.cost_aware_return.to_string(),
    );
    common::config::store::set(
        "PORTFOLIO_COST_ITERATIONS_MAX",
        &cli.cost_iterations_max.to_string(),
    );
    common::config::store::set(
        "TRADE_ALL_PREDICTED_ENABLED",
        &cli.all_predicted.to_string(),
    );
    common::config::store::set(
        "TRADE_TOP_N_AFTER_PREDICTION",
        &cli.top_n_after_prediction.to_string(),
    );
    common::config::store::set(
        "TRADE_PREDICTION_SHRINKAGE_LAMBDA",
        &cli.shrinkage_lambda.to_string(),
    );
    common::config::store::set(
        "PORTFOLIO_VOLATILITY_TARGET_ENABLED",
        &cli.vol_target.to_string(),
    );
    common::config::store::set(
        "PORTFOLIO_REGIME_BREADTH_ENABLED",
        &cli.regime_breadth.to_string(),
    );
    common::config::store::set("PORTFOLIO_HALF_KELLY_ENABLED", &cli.half_kelly.to_string());
    common::config::store::set("PORTFOLIO_STOP_LOSS_ENABLED", &cli.stop_loss.to_string());
    common::config::store::set(
        "TRADE_DD_CIRCUIT_BREAKER_ENABLED",
        &cli.dd_circuit_breaker.to_string(),
    );
    common::config::store::set("TRADE_ALPHA_GATE_ENABLED", &cli.alpha_gate.to_string());
    common::config::store::set(
        "TRADE_ALPHA_GATE_MULTIPLIER",
        &cli.alpha_gate_multiplier.to_string(),
    );
    common::config::store::set(
        "TRADE_ALPHA_GATE_HOLD_CYCLES",
        &cli.alpha_gate_hold_cycles.to_string(),
    );
    common::config::store::set(
        "TRADE_ALPHA_GATE_MIN_PASS_COUNT",
        &cli.alpha_gate_min_pass_count.to_string(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::algorithm::portfolio::PredErrDiagonalMode;
    use std::path::PathBuf;

    fn make_cli(start: &str, end: &str) -> RunArgs {
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
            alpha_gate: false,
            alpha_gate_multiplier: 2.0,
            alpha_gate_hold_cycles: 1,
            alpha_gate_min_pass_count: 5,
        }
    }

    #[tokio::test]
    async fn run_simulation_rejects_start_after_end() {
        let cli = make_cli("2025-06-15", "2025-06-01");
        let err = run_simulation(&cli).await.unwrap_err();
        assert!(
            err.to_string()
                .contains("start-date must be before end-date"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn apply_config_sets_expected_values() {
        let cli = RunArgs {
            start_date: "2025-01-01".to_string(),
            end_date: "2025-12-31".to_string(),
            initial_capital: 500.0,
            top_tokens: 20,
            price_history_days: 60,
            rebalance_threshold: 0.25,
            rebalance_interval_days: 3,
            output: PathBuf::from("test.json"),
            sweep: None,
            generate_predictions: false,
            bias_correction: true,
            pred_err_diagonal: true,
            pred_err_diagonal_k: 2.0,
            pred_err_diagonal_mode: PredErrDiagonalMode::Max,
            cost_aware_return: true,
            cost_iterations_max: 5,
            all_predicted: true,
            top_n_after_prediction: 30,
            shrinkage_lambda: 0.1,
            vol_target: true,
            regime_breadth: true,
            half_kelly: true,
            stop_loss: true,
            dd_circuit_breaker: true,
            alpha_gate: true,
            alpha_gate_multiplier: 3.0,
            alpha_gate_hold_cycles: 4,
            alpha_gate_min_pass_count: 6,
        };

        apply_config(&cli);

        assert_eq!(
            common::config::store::get("TRADE_TOP_TOKENS").unwrap(),
            "20"
        );
        assert_eq!(
            common::config::store::get("TRADE_PRICE_HISTORY_DAYS").unwrap(),
            "60"
        );
        assert_eq!(
            common::config::store::get("PORTFOLIO_REBALANCE_THRESHOLD").unwrap(),
            "0.25"
        );
        assert_eq!(
            common::config::store::get("TRADE_INITIAL_INVESTMENT").unwrap(),
            "500"
        );
        assert_eq!(common::config::store::get("TRADE_ENABLED").unwrap(), "true");
        assert_eq!(
            common::config::store::get("TRADE_BIAS_CORRECTION_ENABLED").unwrap(),
            "true"
        );
        assert_eq!(
            common::config::store::get("PORTFOLIO_PRED_ERR_DIAGONAL_ENABLED").unwrap(),
            "true"
        );
        assert_eq!(
            common::config::store::get("PORTFOLIO_PRED_ERR_DIAGONAL_K").unwrap(),
            "2"
        );
        assert_eq!(
            common::config::store::get("PORTFOLIO_PRED_ERR_DIAGONAL_MODE").unwrap(),
            "max"
        );
        assert_eq!(
            common::config::store::get("TRADE_COST_AWARE_RETURN_ENABLED").unwrap(),
            "true"
        );
        assert_eq!(
            common::config::store::get("PORTFOLIO_COST_ITERATIONS_MAX").unwrap(),
            "5"
        );
        assert_eq!(
            common::config::store::get("TRADE_ALL_PREDICTED_ENABLED").unwrap(),
            "true"
        );
        assert_eq!(
            common::config::store::get("TRADE_TOP_N_AFTER_PREDICTION").unwrap(),
            "30"
        );
        assert_eq!(
            common::config::store::get("TRADE_PREDICTION_SHRINKAGE_LAMBDA").unwrap(),
            "0.1"
        );
        assert_eq!(
            common::config::store::get("PORTFOLIO_VOLATILITY_TARGET_ENABLED").unwrap(),
            "true"
        );
        assert_eq!(
            common::config::store::get("PORTFOLIO_REGIME_BREADTH_ENABLED").unwrap(),
            "true"
        );
        assert_eq!(
            common::config::store::get("PORTFOLIO_HALF_KELLY_ENABLED").unwrap(),
            "true"
        );
        assert_eq!(
            common::config::store::get("PORTFOLIO_STOP_LOSS_ENABLED").unwrap(),
            "true"
        );
        assert_eq!(
            common::config::store::get("TRADE_DD_CIRCUIT_BREAKER_ENABLED").unwrap(),
            "true"
        );
        assert_eq!(
            common::config::store::get("TRADE_ALPHA_GATE_ENABLED").unwrap(),
            "true"
        );
        assert_eq!(
            common::config::store::get("TRADE_ALPHA_GATE_MULTIPLIER").unwrap(),
            "3"
        );
        assert_eq!(
            common::config::store::get("TRADE_ALPHA_GATE_HOLD_CYCLES").unwrap(),
            "4"
        );
        assert_eq!(
            common::config::store::get("TRADE_ALPHA_GATE_MIN_PASS_COUNT").unwrap(),
            "6"
        );
    }
}
