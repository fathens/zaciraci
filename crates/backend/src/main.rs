#![deny(warnings)]

mod arbitrage;
mod jsonrpc;
mod logging;
mod persistence;
mod ref_finance;
mod trade;
mod types;
mod wallet;

use crate::logging::*;
pub use common::config;

type Result<T> = anyhow::Result<T>;

#[tokio::main]
async fn main() {
    use logging::*;

    let log = DEFAULT.new(o!("function" => "main"));
    info!(log, "Starting up");

    let base = wallet::new_wallet().derive(0).unwrap();
    let account_zero = base.derive(0).unwrap();
    info!(log, "Account 0 created"; "pubkey" => %account_zero.pub_base58());

    tokio::spawn(trade::run());
    tokio::spawn(arbitrage::run());
    tokio::signal::ctrl_c().await.ok();
}
