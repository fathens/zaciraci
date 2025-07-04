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
    Predict(commands::predict::PredictArgs),
}

pub async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Top(args) => commands::top::run(args).await,
        Commands::Predict(args) => commands::predict::run(args).await,
    }
}
