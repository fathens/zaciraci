pub mod api;
pub mod commands;
pub mod models;
pub mod utils;

#[cfg(test)]
mod tests;

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
    Verify(commands::verify::VerifyArgs),
    Chart(commands::chart::ChartArgs),
}

pub async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Top(args) => commands::top::run(args).await,
        Commands::History(args) => commands::history::run_history(args).await,
        Commands::Predict(command) => commands::predict::run(command).await,
        Commands::Verify(args) => commands::verify::run(args).await,
        Commands::Chart(args) => commands::chart::run_chart(args).await,
    }
}
