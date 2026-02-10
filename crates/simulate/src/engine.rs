use crate::cli::Cli;
use crate::mock_client::SimulationClient;
use crate::mock_wallet::SimulationWallet;
use crate::output::SimulationResult;
use crate::portfolio_state::PortfolioState;
use anyhow::Result;
use chrono::{DateTime, NaiveTime, TimeZone, Utc};
use logging::*;
use std::sync::Arc;
use tokio::sync::Mutex;
use trade::predict::PredictionService;

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

    // Create prediction service (real chronos-rs)
    let prediction_service = PredictionService::new();

    info!(log, "starting simulation";
        "start_date" => %start_date,
        "end_date" => %end_date,
        "initial_capital" => cli.initial_capital,
        "rebalance_interval_days" => cli.rebalance_interval_days,
    );

    // Simulation loop: step through dates
    let mut current_date = start_date;
    let mut is_first_day = true;
    let mut day_count = 0u32;

    while current_date <= end_date {
        let sim_day: DateTime<Utc> = Utc
            .from_utc_datetime(&current_date.and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap()));

        info!(log, "simulation day"; "date" => %current_date, "day" => day_count);

        // Step 1: Select top volatility tokens
        let tokens = match trade::strategy::select_top_volatility_tokens(
            &prediction_service,
            sim_day,
        )
        .await
        {
            Ok(tokens) => tokens,
            Err(e) => {
                warn!(log, "failed to select tokens, skipping day"; "date" => %current_date, "error" => ?e);
                current_date += chrono::Duration::days(cli.rebalance_interval_days);
                day_count += 1;
                continue;
            }
        };

        if tokens.is_empty() {
            warn!(log, "no tokens selected, skipping day"; "date" => %current_date);
            current_date += chrono::Duration::days(cli.rebalance_interval_days);
            day_count += 1;
            continue;
        }

        info!(log, "selected tokens"; "count" => tokens.len());

        // Step 2: Execute portfolio strategy
        let period_id = format!("sim_{}", current_date);
        let available_funds = if is_first_day {
            initial_capital_yocto
        } else {
            0u128 // continuing, not a new period
        };

        let actions = match trade::strategy::execute_portfolio_strategy(
            &prediction_service,
            &tokens,
            available_funds,
            is_first_day,
            &period_id,
            &sim_client,
            &sim_wallet,
            sim_day,
        )
        .await
        {
            Ok(actions) => actions,
            Err(e) => {
                warn!(log, "failed to execute strategy, skipping day"; "date" => %current_date, "error" => ?e);
                current_date += chrono::Duration::days(cli.rebalance_interval_days);
                day_count += 1;
                continue;
            }
        };

        info!(log, "strategy actions"; "count" => actions.len());

        // Step 3: Apply actions to portfolio state
        {
            let mut state = portfolio.lock().await;
            if let Err(e) = state.apply_actions(&actions, sim_day).await {
                warn!(log, "failed to apply actions"; "date" => %current_date, "error" => ?e);
            }

            // Step 4: Record snapshot
            if let Err(e) = state.record_snapshot(sim_day).await {
                warn!(log, "failed to record snapshot"; "date" => %current_date, "error" => ?e);
            }
        }

        is_first_day = false;
        current_date += chrono::Duration::days(cli.rebalance_interval_days);
        day_count += 1;
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
fn apply_config(cli: &Cli) {
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
    // Disable actual trading
    common::config::set("TRADE_ENABLED", "false");
}
