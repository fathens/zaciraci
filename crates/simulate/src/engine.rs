use crate::cli::Cli;
use crate::mock_client::SimulationClient;
use crate::mock_wallet::SimulationWallet;
use crate::output::SimulationResult;
use crate::portfolio_state::{DbRateProvider, PortfolioState};
use anyhow::Result;
use chrono::{NaiveTime, TimeZone, Utc};
use logging::*;
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn run_simulation(cli: &Cli) -> Result<SimulationResult> {
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

    // Initialize token decimals cache from DB
    if let Err(e) = trade::token_cache::load_from_db().await {
        warn!(log, "failed to load token decimals cache"; "error" => ?e);
    }

    // Convert initial capital NEAR -> yoctoNEAR
    let initial_capital_yocto = (cli.initial_capital * 1e24) as u128;

    // Initialize portfolio state
    let portfolio = Arc::new(Mutex::new(PortfolioState::new(initial_capital_yocto)));

    // Create mock client and wallet
    let sim_client = SimulationClient::new(Arc::clone(&portfolio), initial_capital_yocto);
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
        let sim_day = Utc
            .from_utc_datetime(&current_date.and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap()));

        info!(log, "simulation day"; "date" => %current_date, "day" => day_count);

        // Execute the full trading cycle via trade::strategy::start
        if let Err(e) = trade::strategy::start(&sim_client, &sim_wallet, sim_day).await {
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

        current_date += chrono::Duration::days(cli.rebalance_interval_days);
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

/// Apply CLI parameters to the config system
pub(crate) fn apply_config(cli: &Cli) {
    common::config::set("TRADE_TOP_TOKENS", &cli.top_tokens.to_string());
    common::config::set("TRADE_VOLATILITY_DAYS", &cli.volatility_days.to_string());
    common::config::set(
        "TRADE_PRICE_HISTORY_DAYS",
        &cli.price_history_days.to_string(),
    );
    common::config::set(
        "PORTFOLIO_REBALANCE_THRESHOLD",
        &cli.rebalance_threshold.to_string(),
    );
    common::config::set("TRADE_INITIAL_INVESTMENT", &cli.initial_capital.to_string());
    // Enable trading (mock client prevents real transactions)
    common::config::set("TRADE_ENABLED", "true");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
        let cli = Cli {
            start_date: "2025-01-01".to_string(),
            end_date: "2025-12-31".to_string(),
            initial_capital: 500.0,
            top_tokens: 20,
            volatility_days: 14,
            price_history_days: 60,
            rebalance_threshold: 0.25,
            rebalance_interval_days: 3,
            output: PathBuf::from("test.json"),
            sweep: None,
        };

        apply_config(&cli);

        assert_eq!(common::config::get("TRADE_TOP_TOKENS").unwrap(), "20");
        assert_eq!(common::config::get("TRADE_VOLATILITY_DAYS").unwrap(), "14");
        assert_eq!(
            common::config::get("TRADE_PRICE_HISTORY_DAYS").unwrap(),
            "60"
        );
        assert_eq!(
            common::config::get("PORTFOLIO_REBALANCE_THRESHOLD").unwrap(),
            "0.25"
        );
        assert_eq!(
            common::config::get("TRADE_INITIAL_INVESTMENT").unwrap(),
            "500"
        );
        assert_eq!(common::config::get("TRADE_ENABLED").unwrap(), "true");
    }
}
