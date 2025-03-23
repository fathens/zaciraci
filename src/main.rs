#![deny(warnings)]

mod config;
mod trade;
mod jsonrpc;
mod logging;
mod ollama;
mod persistence;
mod ref_finance;
mod types;
mod wallet;
mod rpc;

use crate::jsonrpc::SentTx;
use crate::logging::*;
use crate::ref_finance::errors::Error;
use crate::ref_finance::path::preview::Preview;
use crate::ref_finance::pool_info::TokenPair;
use crate::ref_finance::token_account::{TokenInAccount, WNEAR_TOKEN};
use crate::types::MicroNear;
use crate::wallet::Wallet;
use anyhow::bail;
use futures_util::future::join_all;
use humantime::parse_duration;
use near_primitives::types::Balance;
use once_cell::sync::Lazy;
use std::time::Duration;

type Result<T> = anyhow::Result<T>;

static TOKEN_NOT_FOUND_WAIT: Lazy<Duration> = Lazy::new(|| {
    config::get("TOKEN_NOT_FOUND_WAIT")
        .and_then(|v| Ok(parse_duration(&v)?))
        .unwrap_or_else(|_| Duration::from_secs(1)) // デフォルト: 1秒
});

static OTHER_ERROR_WAIT: Lazy<Duration> = Lazy::new(|| {
    config::get("OTHER_ERROR_WAIT")
        .and_then(|v| Ok(parse_duration(&v)?))
        .unwrap_or_else(|_| Duration::from_secs(30)) // デフォルト: 30秒
});

static PREVIEW_NOT_FOUND_WAIT: Lazy<Duration> = Lazy::new(|| {
    config::get("PREVIEW_NOT_FOUND_WAIT")
        .and_then(|v| Ok(parse_duration(&v)?))
        .unwrap_or_else(|_| Duration::from_secs(10)) // デフォルト: 10秒
});

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

    let db_client = persistence::new_client();
    info!(log, "Database client created"; "db_client" => ?db_client);

    let base = wallet::new_wallet().derive(0).unwrap();
    let account_zero = base.derive(0).unwrap();
    info!(log, "Account 0 created"; "pubkey" => %account_zero.pub_base58());

    tokio::spawn(trade::run());
    tokio::spawn(rpc::run());

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
                warn!(log, "failure: {:?}", err);
                // WNEAR_TOKENのエラーは特別扱い
                if let Some(Error::TokenNotFound(name)) = err.downcast_ref::<Error>() {
                    if WNEAR_TOKEN.to_string().eq(name) {
                        info!(
                            log,
                            "token not found, retrying after {:?}", *TOKEN_NOT_FOUND_WAIT
                        );
                        tokio::time::sleep(*TOKEN_NOT_FOUND_WAIT).await;
                        continue;
                    }
                }
                // その他のエラーは長めの待機
                warn!(
                    log,
                    "non-jsonrpc error, retrying after {:?}", *OTHER_ERROR_WAIT
                );
                tokio::time::sleep(*OTHER_ERROR_WAIT).await;
                continue;
            }
        }
    }
}

async fn single_loop<C, W>(client: &C, wallet: &W) -> Result<()>
where
    C: jsonrpc::AccountInfo + jsonrpc::SendTx + jsonrpc::ViewContract + jsonrpc::GasInfo,
    <C as jsonrpc::SendTx>::Output: std::fmt::Display,
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

        let res = ref_finance::storage::check_and_deposit(client, wallet, &tokens).await?;
        if res.is_none() {
            bail!("no account to deposit");
        }

        let swaps = pre_path
            .into_iter()
            .map(|(p, v)| swap_each(client, wallet, p, v));
        let results = join_all(swaps).await;
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        info!(log, "swaps completed";
            "success" => format!("{}/{}", success_count, results.len()),
        );
    } else {
        info!(log, "previews not found");
        tokio::time::sleep(*PREVIEW_NOT_FOUND_WAIT).await;
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
    <C as jsonrpc::SendTx>::Output: std::fmt::Display,
    W: Wallet,
{
    let log = DEFAULT.new(o!(
        "function" => "swap_each",
        "preview.output_value" => format!("{}", preview.output_value),
        "preview.gain" => format!("{}", preview.gain),
        "path.len" => format!("{}", path.len()),
    ));

    let arg = ref_finance::swap::SwapArg {
        initial_in: preview.input_value.into(),
        min_out: preview.output_value - preview.gain,
    };
    let swap_result = ref_finance::swap::run_swap(client, wallet, &path, arg).await;

    let (sent_tx, out) = match swap_result {
        Ok(result) => result,
        Err(e) => {
            error!(log, "swap operation failed"; "error" => ?e);
            return Err(e);
        }
    };

    if let Err(e) = sent_tx.wait_for_success().await {
        error!(log, "transaction failed"; "tx" => %sent_tx, "error" => %e);
        return Err(e);
    }

    info!(log, "swap done";
        "estimated_output" => out,
    );
    Ok(())
}
