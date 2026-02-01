use clap::Parser;
use cli_tokens::Cli;
use std::process;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    if let Err(err) = cli_tokens::run(cli).await {
        eprintln!("Error: {}", err);

        // Print the full error chain
        let mut source = err.source();
        while let Some(err) = source {
            eprintln!("Caused by: {}", err);
            source = err.source();
        }

        // Print backtrace if available
        let backtrace = err.backtrace();
        eprintln!("Backtrace: {}", backtrace);

        process::exit(1);
    }
}
