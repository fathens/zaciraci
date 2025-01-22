#![deny(warnings)]

mod config;
mod cron;
mod errors;
mod jsonrpc;
mod logging;
mod persistence;
mod ref_finance;
mod types;
mod wallet;
mod web;

use crate::logging::DEFAULT;
use crate::ref_finance::token_account::TokenInAccount;
use errors::Error;
use slog::{error, info, o};
use std::time::Duration;
use tokio::time::sleep;

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

    let base = wallet::WALLET.derive(0).unwrap();
    let account_zero = base.derive(0).unwrap();
    info!(log, "Account 0 created"; "pubkey" => %account_zero.pub_base58());

    tokio::spawn(cron::run());
    tokio::spawn(web::run());

    match main_loop().await {
        Ok(_) => info!(log, "shutting down"),
        Err(err) => error!(log, "shutting down: {}", err),
    }
}

async fn main_loop() -> Result<()> {
    let log = DEFAULT.new(o!("function" => "main_loop"));
    loop {
        match single_loop().await {
            Ok(_) => info!(log, "success, go next"),
            Err(err) => {
                error!(log, "failure: {}", err);
                sleep(Duration::from_secs(10)).await;
            }
        }
    }
}

async fn single_loop() -> Result<()> {
    let log = DEFAULT.new(o!("function" => "single_loop"));

    let (token, balance) = ref_finance::balances::start().await?;
    let start: &TokenInAccount = &token.into();

    let pools = ref_finance::pool_info::PoolInfoList::read_from_node().await?;
    let gas_price = jsonrpc::get_gas_price(None).await?;
    let previews = ref_finance::path::pick_previews(&pools, start, balance, gas_price)?;

    if let Some(previews) = previews {
        let (pre_path, tokens) = previews.into_with_path(start).await?;

        let account = wallet::WALLET.account_id();
        ref_finance::storage::check_and_deposit(account, &tokens).await?;

        for (preview, path) in pre_path {
            info!(log, "run swap";
                "preview" => ?preview.gain,
                "path" => ?path.len(),
            );
            // TODO: run swap
        }
    } else {
        info!(log, "previews not found");
        sleep(Duration::from_secs(10)).await;
    }

    Ok(())
}
