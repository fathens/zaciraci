use clap::Parser;
use cli_tokens::Cli;
use std::process;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    if let Err(err) = cli_tokens::run(cli).await {
        eprintln!("Error: {}", err);
        process::exit(1);
    }
}
