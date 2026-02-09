#![deny(warnings)]

use logging::*;

#[tokio::main]
async fn main() {
    let log = DEFAULT.new(o!("function" => "main"));
    info!(log, "Starting up");

    let base = blockchain::wallet::new_wallet().derive(0).unwrap();
    let account_zero = base.derive(0).unwrap();
    info!(log, "Account 0 created"; "pubkey" => %account_zero.pub_base58());

    tokio::spawn(trade::run());
    tokio::spawn(arbitrage::run());
    tokio::signal::ctrl_c().await.ok();
}
