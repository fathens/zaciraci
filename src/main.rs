#![deny(warnings)]

mod config;
mod cron;
mod errors;
mod logging;
mod persistence;
mod ref_finance;
mod web;

use errors::Error;
type Result<T> = std::result::Result<T, Error>;

#[tokio::main]
async fn main() {
    use logging::*;

    let log = DEFAULT.new(o!("function" => "main"));
    info!(log, "Starting up");
    debug!(log, "log level check");
    trace!(log, "log level check");
    error!(log, "log level check");
    warn!(log, "log level check");
    crit!(log, "log level check");

    tokio::spawn(cron::run());
    web::run().await
}
