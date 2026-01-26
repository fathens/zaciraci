use crate::config;
use crate::jsonrpc;
use crate::jsonrpc::SentTx;
use crate::logging::*;
use crate::ref_finance;
use crate::ref_finance::errors::Error;
use crate::ref_finance::path::preview::Preview;
use crate::ref_finance::pool_info::TokenPath;
use crate::ref_finance::token_account::WNEAR_TOKEN;
use crate::types::MicroNear;
use crate::wallet;
use anyhow::bail;
use humantime::parse_duration;
use near_primitives::types::Balance;
use once_cell::sync::Lazy;
use std::time::Duration;

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

fn is_needed() -> bool {
    config::get("ARBITRAGE_NEEDED")
        .ok()
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(false)
}

pub async fn run() {
    let log = DEFAULT.new(o!("function" => "main_loop"));
    if !is_needed() {
        info!(log, "Arbitrage is not needed.");
        return;
    }
    let client = jsonrpc::new_client();
    let wallet = wallet::new_wallet();
    loop {
        match single_loop(&client, &wallet).await {
            Ok(_) => info!(log, "success, go next"),
            Err(err) => {
                warn!(log, "failure: {:?}", err);
                // WNEAR_TOKENのエラーは特別扱い
                if let Some(Error::TokenNotFound(name)) = err.downcast_ref::<Error>()
                    && WNEAR_TOKEN.to_string().eq(name)
                {
                    info!(
                        log,
                        "token not found, retrying after {:?}", *TOKEN_NOT_FOUND_WAIT
                    );
                    tokio::time::sleep(*TOKEN_NOT_FOUND_WAIT).await;
                    continue;
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

async fn single_loop<C, W>(client: &C, wallet: &W) -> crate::Result<()>
where
    C: jsonrpc::AccountInfo + jsonrpc::SendTx + jsonrpc::ViewContract + jsonrpc::GasInfo,
    <C as jsonrpc::SendTx>::Output: std::fmt::Display,
    W: wallet::Wallet,
{
    let log = DEFAULT.new(o!("function" => "single_loop"));

    let balance = ref_finance::balances::start(client, wallet, &WNEAR_TOKEN, None).await?;
    let start = WNEAR_TOKEN.to_in();
    let start_balance = MicroNear::from_yocto(balance);
    info!(log, "start";
        "start.token" => ?start,
        "start.balance" => ?balance,
        "start.balance_in_micro" => ?start_balance,
    );

    let pools = ref_finance::pool_info::PoolInfoList::read_from_db(None).await?;

    let graph = ref_finance::path::graph::TokenGraph::new(pools);
    let gas_price = client.get_gas_price(None).await?;
    let previews = ref_finance::path::pick_previews(&graph, &start, start_balance, gas_price)?;

    if let Some(previews) = previews {
        let (pre_path, tokens) = previews.into_with_path(&graph, &start).await?;

        let res = ref_finance::storage::check_and_deposit(client, wallet, &tokens).await?;
        if res.is_none() {
            bail!("no account to deposit");
        }

        // スワップを順次実行（nonce衝突を回避）
        let mut success_count = 0;
        let total_count = pre_path.len();

        for (preview, path) in pre_path {
            match swap_each(client, wallet, preview, path).await {
                Ok(_) => {
                    success_count += 1;
                    // Arbitrageの場合は1つ成功したら終了
                    info!(log, "arbitrage swap successful, stopping further attempts");
                    break;
                }
                Err(e) => {
                    warn!(log, "swap attempt failed, trying next path if available"; "error" => %e);
                }
            }
        }

        info!(log, "swaps completed";
            "success" => format!("{}/{}", success_count, total_count),
        );
    } else {
        info!(log, "previews not found");
        tokio::time::sleep(*PREVIEW_NOT_FOUND_WAIT).await;
    }

    Ok(())
}

#[cfg(test)]
mod tests;

async fn swap_each<A, C, W>(
    client: &C,
    wallet: &W,
    preview: Preview<A>,
    path: TokenPath,
) -> crate::Result<()>
where
    A: Into<Balance> + Copy,
    C: jsonrpc::SendTx,
    <C as jsonrpc::SendTx>::Output: std::fmt::Display,
    W: wallet::Wallet,
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
    let swap_result = ref_finance::swap::run_swap(client, wallet, &path.0, arg).await;

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
