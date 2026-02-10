#![deny(warnings)]

mod cli;
mod engine;
mod mock_client;
mod mock_wallet;
mod output;
mod portfolio_state;
mod sweep;

use clap::Parser;
use logging::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let log = DEFAULT.new(o!("function" => "main"));

    let cli = cli::Cli::parse();

    if let Some(sweep_path) = &cli.sweep {
        info!(log, "running parameter sweep mode");
        sweep::run_sweep(&cli, sweep_path).await?;
    } else {
        info!(log, "running single simulation");
        let result = engine::run_simulation(&cli).await?;
        result.write_to_file(&cli.output)?;
        info!(log, "results written"; "path" => cli.output.display().to_string());

        // Print summary
        println!("\n=== Simulation Results ===");
        println!("Period: {} to {}", cli.start_date, cli.end_date);
        println!("Initial capital: {} NEAR", cli.initial_capital);
        println!(
            "Total return: {:.2}%",
            result.performance.total_return * 100.0
        );
        println!(
            "Annualized return: {:.2}%",
            result.performance.annualized_return * 100.0
        );
        println!("Sharpe ratio: {:.3}", result.performance.sharpe_ratio);
        println!("Sortino ratio: {:.3}", result.performance.sortino_ratio);
        println!(
            "Max drawdown: {:.2}%",
            result.performance.max_drawdown * 100.0
        );
        println!("Win rate: {:.2}%", result.performance.win_rate * 100.0);
        println!("Trades: {}", result.trades.len());
    }

    Ok(())
}
