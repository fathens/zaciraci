pub mod api;
pub mod commands;
pub mod models;
pub mod utils;

#[cfg(test)]
mod tests;

// Add integration tests module to verify simulate-report compatibility
#[cfg(test)]
mod integration_tests {
    include!("commands/integration_tests.rs");
}

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[clap(name = "cli_tokens")]
#[clap(about = "CLI tool for volatility tokens analysis and prediction")]
#[clap(version)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Top(commands::top::TopArgs),
    History(commands::history::HistoryArgs),
    Predict(commands::predict::PredictCommand),
    Chart(commands::chart::ChartArgs),
    Simulate(commands::simulate::SimulateArgs),
    Report(commands::report::ReportArgs),
}

pub async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Top(args) => commands::top::run(args).await,
        Commands::History(args) => commands::history::run_history(args).await,
        Commands::Predict(command) => commands::predict::run(command).await,
        Commands::Chart(args) => commands::chart::run_chart(args).await,
        Commands::Simulate(args) => commands::simulate::run(args).await,
        Commands::Report(args) => commands::report::run_report(args),
    }
}
