#![deny(warnings)]

use logging::*;

#[tokio::main]
async fn main() {
    let log = DEFAULT.new(o!("function" => "main"));
    info!(log, "Starting up");

    // DB から設定をロード（失敗時はスキップ）
    if let Err(e) = persistence::config_store::reload_to_config().await {
        warn!(log, "failed to load config from DB, continuing with env/TOML"; "error" => %e);
    }

    let base = blockchain::wallet::new_wallet().derive(0).unwrap();
    let account_zero = base.derive(0).unwrap();
    info!(log, "Account 0 created"; "pubkey" => %account_zero.pub_base58());

    tokio::spawn(trade::run());
    tokio::spawn(arbitrage::run());
    tokio::signal::ctrl_c().await.ok();
}
