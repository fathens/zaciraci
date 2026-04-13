#![deny(warnings)]

use blockchain::jsonrpc;
use blockchain::jsonrpc::SentTx;
use blockchain::ref_finance;
use blockchain::ref_finance::path::preview::Preview;
use blockchain::ref_finance::token_account::WNEAR_TOKEN;
use blockchain::types::MicroNear;
use blockchain::wallet;
use common::config::ConfigAccess;
use dex::TokenPath;
use dex::errors::Error;
use logging::*;

use std::time::Duration;

type Result<T> = anyhow::Result<T>;

fn token_not_found_wait(cfg: &impl ConfigAccess) -> Duration {
    cfg.arbitrage_token_not_found_wait()
}

fn other_error_wait(cfg: &impl ConfigAccess) -> Duration {
    cfg.arbitrage_other_error_wait()
}

fn preview_not_found_wait(cfg: &impl ConfigAccess) -> Duration {
    cfg.arbitrage_preview_not_found_wait()
}

fn is_needed(cfg: &impl ConfigAccess) -> bool {
    cfg.arbitrage_needed()
}

pub async fn run(cfg: common::config::ConfigResolver) {
    let log = DEFAULT.new(o!("function" => "main_loop"));
    if !is_needed(&cfg) {
        info!(log, "Arbitrage is not needed.");
        return;
    }
    let client = jsonrpc::new_client();
    let wallet = wallet::new_wallet();
    loop {
        match single_loop(&client, &wallet, &cfg).await {
            Ok(_) => info!(log, "success, go next"),
            Err(err) => {
                warn!(log, "failure: {:?}", err);
                // WNEAR_TOKENのエラーは特別扱い
                if let Some(Error::TokenNotFound(name)) = err.downcast_ref::<Error>()
                    && WNEAR_TOKEN.to_string().eq(name)
                {
                    let wait = token_not_found_wait(&cfg);
                    info!(log, "token not found, retrying after {:?}", wait);
                    tokio::time::sleep(wait).await;
                    continue;
                }
                // その他のエラーは長めの待機
                let wait = other_error_wait(&cfg);
                warn!(log, "non-jsonrpc error, retrying after {:?}", wait);
                tokio::time::sleep(wait).await;
                continue;
            }
        }
    }
}

async fn single_loop<C, W>(client: &C, wallet: &W, cfg: &impl ConfigAccess) -> crate::Result<()>
where
    C: jsonrpc::AccountInfo + jsonrpc::SendTx + jsonrpc::ViewContract + jsonrpc::GasInfo,
    <C as jsonrpc::SendTx>::Output: std::fmt::Display,
    W: wallet::Wallet,
{
    let log = DEFAULT.new(o!("function" => "single_loop"));

    let balance = ref_finance::balances::start(client, wallet, &WNEAR_TOKEN, None, cfg).await?;
    let start = WNEAR_TOKEN.to_in();
    let balance_yocto = balance.as_yoctonear();
    let start_balance = MicroNear::from_yocto(balance_yocto);
    info!(log, "start";
        "start.token" => ?start,
        "start.balance" => balance_yocto,
        "start.balance_in_micro" => ?start_balance,
    );

    let pools = persistence::pool_info::read_from_db(None).await?;

    let graph = ref_finance::path::graph::TokenGraph::new(pools);
    let gas_price = client.get_gas_price(None).await?;
    let previews = ref_finance::path::pick_previews(&graph, &start, start_balance, gas_price)?;

    if let Some(previews) = previews {
        let (pre_path, tokens) = previews.into_with_path(&graph, &start).await?;
        for (_, path) in &pre_path {
            path.validate_length()?;
        }

        let max_top_up =
            near_sdk::NearToken::from_yoctonear(cfg.ref_storage_max_top_up_yoctonear());
        // keep: 裁定取引は毎回異なるパスを使うため、基軸通貨の WNEAR のみ保持
        let keep = vec![WNEAR_TOKEN.clone()];
        ref_finance::storage::ensure_ref_storage_setup(client, wallet, &tokens, &keep, max_top_up)
            .await?;

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
        tokio::time::sleep(preview_not_found_wait(cfg)).await;
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
    A: Into<u128> + Copy,
    C: jsonrpc::SendTx,
    <C as jsonrpc::SendTx>::Output: std::fmt::Display,
    W: wallet::Wallet,
{
    let log = DEFAULT.new(o!(
        "function" => "swap_each",
        "preview.output_value" => preview.output_value,
        "preview.gain" => preview.gain,
        "path.len" => format!("{}", path.len()),
    ));

    let arg = ref_finance::swap::SwapArg {
        initial_in: preview.input_value.into(),
        min_out: preview.output_value.saturating_sub(preview.gain),
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
