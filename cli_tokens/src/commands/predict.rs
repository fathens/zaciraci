pub mod kick;
mod pull;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[clap(about = "Execute zeroshot prediction for specified token file")]
pub struct PredictCommand {
    #[clap(subcommand)]
    pub subcommand: PredictSubcommand,
}

#[derive(Subcommand)]
pub enum PredictSubcommand {
    /// Execute prediction and save results
    Kick(kick::KickArgs),
}

pub async fn run(command: PredictCommand) -> anyhow::Result<()> {
    match command.subcommand {
        PredictSubcommand::Kick(args) => kick::run(args).await,
    }
}
