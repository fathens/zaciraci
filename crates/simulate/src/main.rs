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
        let perf = &result.performance;
        let return_pct = perf.total_return * 100.0;
        let return_sign = if return_pct >= 0.0 { "+" } else { "" };

        println!("\n=== Simulation Results ===");
        println!("Period: {} to {}", cli.start_date, cli.end_date);
        println!("Initial capital: {:.4} NEAR", cli.initial_capital);
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
