#![deny(warnings)]

mod config;
mod cron;
mod errors;
mod logging;
mod persistence;
mod ref_finance;
mod web;

pub use errors::Error;
type Result<T> = std::result::Result<T, Error>;

#[tokio::main]
async fn main() {
    tokio::spawn(cron::run());
    web::run().await;
}
