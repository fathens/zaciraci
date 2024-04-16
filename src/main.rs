#![deny(warnings)]

mod cron;
mod error;
mod logging;
mod persistence;
mod web;

pub use error::Error;
type Result<T> = std::result::Result<T, Error>;

#[tokio::main]
async fn main() {
    tokio::spawn(cron::run());
    web::run().await;
}
