#![deny(warnings)]

mod cli;
mod engine;
mod mock_client;
mod mock_wallet;
mod output;
mod portfolio_state;
mod prediction;
mod sweep;
mod verify;

use clap::Parser;
use cli::Command;
use logging::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let log = DEFAULT.new(o!("function" => "main"));

    let cli = cli::Cli::parse();

    match cli.command {
        Command::Run(ref args) => run_simulation_command(args, &log).await,
        Command::Verify(ref args) => verify::run_verify(args).await,
    }
}

async fn run_simulation_command(args: &cli::RunArgs, log: &slog::Logger) -> anyhow::Result<()> {
    if let Some(sweep_path) = &args.sweep {
        info!(log, "running parameter sweep mode");
        sweep::run_sweep(args, sweep_path).await?;
    } else {
        info!(log, "running single simulation");
        let result = engine::run_simulation(args).await?;
        result.write_to_file(&args.output)?;
        info!(log, "results written"; "path" => args.output.display().to_string());

        // Print summary
        let perf = &result.performance;
        let return_pct = perf.total_return * 100.0;
        let return_sign = if return_pct >= 0.0 { "+" } else { "" };

        println!("\n=== Simulation Results ===");
        println!("Period: {} to {}", args.start_date, args.end_date);
        println!("Initial capital: {:.4} NEAR", args.initial_capital);
        println!(
            "Final balance:   {:.4} NEAR ({}{:.2}%)",
            perf.final_balance_near, return_sign, return_pct
        );
        println!("---");
        let pnl_sign = if perf.total_realized_pnl_near >= 0.0 {
            "+"
        } else {
            ""
        };
        println!(
            "Realized P&L:     {}{:.4} NEAR",
            pnl_sign, perf.total_realized_pnl_near
        );
        println!("Sharpe ratio:       {:.3}", perf.sharpe_ratio);
        println!("Sortino ratio:      {:.3}", perf.sortino_ratio);
        println!("Max drawdown:       {:.2}%", perf.max_drawdown * 100.0);
        println!("Win rate:          {:.2}%", perf.win_rate * 100.0);
        println!("---");
        if perf.liquidation_count > 0 {
            println!(
                "Trades: {} (+ {} liquidations)",
                perf.trade_count, perf.liquidation_count
            );
        } else {
            println!("Trades: {}", perf.trade_count);
        }
    }

    Ok(())
}
