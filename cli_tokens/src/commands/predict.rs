pub mod kick;
pub mod pull;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[clap(about = "Execute zeroshot prediction for specified token file")]
pub struct PredictCommand {
    #[clap(subcommand)]
    pub subcommand: PredictSubcommand,
}

#[derive(Subcommand)]
pub enum PredictSubcommand {
    /// Start an async prediction task and exit
    Kick(kick::KickArgs),
    /// Poll for prediction results
    Pull(pull::PullArgs),
}

pub async fn run(command: PredictCommand) -> anyhow::Result<()> {
    match command.subcommand {
        PredictSubcommand::Kick(args) => kick::run(args).await,
        PredictSubcommand::Pull(args) => pull::run(args).await,
    }
}
