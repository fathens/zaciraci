#![deny(warnings)]

mod config;
mod cron;
mod jsonrpc;
mod logging;
mod ref_finance;
mod types;
mod wallet;
mod web;

use crate::jsonrpc::{GasInfo, SentTx};
use crate::logging::*;
use crate::ref_finance::errors::Error;
use crate::ref_finance::path::preview::Preview;
use crate::ref_finance::pool_info::TokenPair;
use crate::ref_finance::token_account::{TokenInAccount, WNEAR_TOKEN};
use crate::types::MicroNear;
use crate::wallet::Wallet;
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

    let base = wallet::new_wallet().derive(0).unwrap();
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
    let client = jsonrpc::new_client();
    let wallet = wallet::new_wallet();
    loop {
        match single_loop(&client, &wallet).await {
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

async fn single_loop<A, W>(client: &A, wallet: &W) -> Result<()>
where
    A: jsonrpc::AccountInfo + jsonrpc::SendTx + jsonrpc::ViewContract,
    A: GasInfo,
    A: 'static,
    A: Clone,
    A: Send + Sync,
    W: Wallet,
{
    let log = DEFAULT.new(o!("function" => "single_loop"));

    let token = WNEAR_TOKEN.clone();

    let balance = ref_finance::balances::start(client, wallet, &token).await?;
    let start: &TokenInAccount = &token.into();
    let start_balance = MicroNear::from_yocto(balance);
    info!(log, "start";
        "start.token" => ?start,
        "start.balance" => ?balance,
        "start.balance_in_micro" => ?start_balance,
    );

    let pools = ref_finance::pool_info::PoolInfoList::read_from_node(client).await?;
    let graph = ref_finance::path::graph::TokenGraph::new(pools);
    let gas_price = client.get_gas_price(None).await?;
    let previews = ref_finance::path::pick_previews(&graph, start, start_balance, gas_price)?;

    if let Some(previews) = previews {
        let (pre_path, tokens) = previews.into_with_path(&graph, start).await?;

        ref_finance::storage::check_and_deposit(client, wallet, &tokens).await?;

        let swaps = pre_path
            .into_iter()
            .map(|(p, v)| swap_each(client, wallet, p, v));
        join_all(swaps).await;
    } else {
        info!(log, "previews not found");
        tokio::time::sleep(Duration::from_secs(10)).await;
    }

    Ok(())
}

async fn swap_each<A, C, W>(
    client: &C,
    wallet: &W,
    preview: Preview<A>,
    path: Vec<TokenPair>,
) -> Result<()>
where
    A: Into<Balance> + Copy,
    C: jsonrpc::SendTx,
    C: 'static,
    W: Wallet,
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
    let (tx_hash, out) = ref_finance::swap::run_swap(
        client,
        wallet,
        &path,
        preview.input_value.into(),
        ratio_by_step,
    )
    .await?;
    tx_hash.wait_for_success().await?;

    info!(log, "swap done";
        "out_balance" => out,
    );
    Ok(())
}
