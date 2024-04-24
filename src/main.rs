#![deny(warnings)]

mod config;
mod cron;
mod errors;
mod logging;
mod persistence;
mod ref_finance;
mod web;

#[macro_use]
extern crate slog_scope;

use crate::logging::DEFAULT;
pub use errors::Error;

type Result<T> = std::result::Result<T, Error>;

#[tokio::main]
async fn main() {
    let _guard = slog_scope::set_global_logger(DEFAULT.clone());
    info!("Starting up");
    debug!("log level check");
    trace!("log level check");
    error!("log level check");
    warn!("log level check");
    crit!("log level check");

    tokio::spawn(cron::run());
    web::run().await
}
