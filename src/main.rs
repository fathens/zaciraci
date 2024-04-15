#![deny(warnings)]

mod cron;
use tokio::spawn;
mod persistence;
mod web;

#[tokio::main]
async fn main() {
    spawn(cron::run());
    web::run().await;
}
