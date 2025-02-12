#![deny(warnings)]

mod config;
mod cron;
mod jsonrpc;
mod logging;
mod ref_finance;
mod types;
mod wallet;
mod web;

use crate::logging::*;
use crate::ref_finance::errors::Error;
use crate::ref_finance::path::preview::Preview;
use crate::ref_finance::pool_info::TokenPair;
use crate::ref_finance::token_account::{TokenInAccount, WNEAR_TOKEN};
use crate::types::MicroNear;
use futures_util::future::join_all;
use near_primitives::types::Balance;
use std::time::Duration;

type Result<T> = anyhow::Result<T>;

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
        Err(err) => error!(log, "shutting down: {:?}", err),
    }
}

async fn main_loop() -> Result<()> {
    let log = DEFAULT.new(o!("function" => "main_loop"));
    loop {
        match single_loop().await {
            Ok(_) => info!(log, "success, go next"),
            Err(err) => {
                warn!(log, "failure: {}", err);
                if let Some(Error::TokenNotFound(name)) = err.downcast_ref::<Error>() {
                    if WNEAR_TOKEN.to_string().eq(name) {
                        info!(log, "token not found, retry");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                }
                return Err(err);
            }
        }
    }
}

async fn single_loop() -> Result<()> {
    let log = DEFAULT.new(o!("function" => "single_loop"));

    let (token, balance) = ref_finance::balances::start().await?;
    let start: &TokenInAccount = &token.into();
    let start_balance = MicroNear::from_yocto(balance);
    info!(log, "start";
        "start.token" => ?start,
        "start.balance" => ?balance,
        "start.balance_in_micro" => ?start_balance,
    );

    let pools = ref_finance::pool_info::PoolInfoList::read_from_node().await?;
    let graph = ref_finance::path::graph::TokenGraph::new(pools);
    let gas_price = jsonrpc::get_gas_price(None).await?;
    let previews = ref_finance::path::pick_previews(&graph, start, start_balance, gas_price)?;

    if let Some(previews) = previews {
        let (pre_path, tokens) = previews.into_with_path(&graph, start).await?;

        let account = wallet::WALLET.account_id();
        ref_finance::storage::check_and_deposit(account, &tokens).await?;

        let swaps = pre_path
            .into_iter()
            .map(|(p, v)| tokio::spawn(async move { swap_each(p, v).await }));
        join_all(swaps).await;
    } else {
        info!(log, "previews not found");
        tokio::time::sleep(Duration::from_secs(10)).await;
    }

    Ok(())
}

async fn swap_each<A>(preview: Preview<A>, path: Vec<TokenPair>) -> Result<()>
where
    A: Into<Balance> + Copy,
{
    let log = DEFAULT.new(o!(
        "function" => "swap_each",
        "preview.output_value" => format!("{}", preview.output_value),
        "preview.gain" => format!("{}", preview.gain),
        "path.len" => format!("{}", path.len()),
    ));

    let under_limit = (preview.output_value as f32) - (preview.gain as f32) * 0.99;
    let under_ratio = under_limit / (preview.output_value as f32);
    let ratio_by_step = under_ratio.powf(path.len() as f32);

    info!(log, "run swap";
        "under_limit" => ?under_limit,
        "ratio_by_step" => ?ratio_by_step,
    );
    let (tx_hash, out) =
        ref_finance::swap::run_swap(&path, preview.input_value.into(), ratio_by_step).await?;
    let outcome = jsonrpc::wait_tx_executed(wallet::WALLET.account_id(), &tx_hash).await?;

    info!(log, "swap done";
        "out_balance" => out,
        "outcome" => ?outcome,
    );
    Ok(())
}
