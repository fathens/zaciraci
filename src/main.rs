#![deny(warnings)]

mod config;
mod cron;
mod jsonrpc;
mod logging;
mod ref_finance;
mod types;
mod wallet;
mod web;

use crate::jsonrpc::SentTx;
use crate::logging::*;
use crate::ref_finance::errors::Error;
use crate::ref_finance::path::preview::Preview;
use crate::ref_finance::pool_info::TokenPair;
use crate::ref_finance::token_account::{TokenInAccount, WNEAR_TOKEN};
use crate::types::MicroNear;
use crate::wallet::Wallet;
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

        ref_finance::storage::check_and_deposit(client, wallet, &tokens).await?;

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

    let under_limit = calculate_under_limit(preview.output_value, preview.gain);
    let ratio_by_step = calculate_ratio_by_step(preview.output_value, under_limit, path.len());

    let swap_result = ref_finance::swap::run_swap(
        client,
        wallet,
        &path,
        preview.input_value.into(),
        ratio_by_step,
    )
    .await;

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
        "out_balance" => out,
    );
    Ok(())
}

// output_valueとgainからunder_limitを計算する関数
fn calculate_under_limit(output_value: Balance, gain: Balance) -> f32 {
    output_value as f32 - gain as f32 * 0.99
}

// output_valueとgainから、ratio_by_stepを計算する関数
fn calculate_ratio_by_step(output_value: Balance, under_limit: f32, steps: usize) -> f32 {
    let log = DEFAULT.new(o!(
        "function" => "calculate_ratio_by_step",
        "output_value" => format!("{}", output_value),
        "under_limit" => format!("{}", under_limit),
        "steps" => format!("{}", steps),
    ));

    let under_ratio = under_limit / (output_value as f32);

    // stepsの乗根を計算（steps乗してunder_ratioになるような値）
    let ratio_by_step = under_ratio.powf(1.0 / steps as f32);

    info!(log, "calculated";
        "under_limit" => ?under_limit,
        "ratio_by_step" => ?ratio_by_step,
    );
    ratio_by_step
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use proptest::prop_oneof;
    use proptest::strategy::Just;

    proptest! {
        // calculate_under_limitのプロパティテスト
        #[test]
        fn prop_under_limit_respects_gain_ratio(
            output_value in 1_000_000_000_000_000_000_000_000..10_000_000_000_000_000_000_000_000u128,
            gain in prop_oneof![Just(0u128), 1..1_000_000_000_000_000_000_000_000u128]
        ) {
            let under_limit = calculate_under_limit(output_value, gain);
            let min_gain = (output_value as f32) - under_limit;

            // 浮動小数点の精度の問題を考慮して、許容誤差を大きくする
            let epsilon = (gain as f32) / 1000_f32; // 0.1%の許容誤差

            // gainの99%に近いことを確認（許容誤差を考慮）
            if gain > 0 {
                prop_assert!((min_gain - (gain as f32) * 0.99).abs() <= epsilon);
            } else {
                // gainが0の場合、under_limitはoutput_valueと等しい
                prop_assert_eq!(under_limit, output_value as f32);
            }
        }

        // calculate_ratio_by_stepのプロパティテスト
        #[test]
        fn prop_ratio_by_step_composes_correctly(
            output_value in 1_000_000_000_000_000_000_000_000..10_000_000_000_000_000_000_000_000u128,
            under_limit_ratio in 0.5f32..0.99f32,
            steps in prop_oneof![Just(1usize), 2usize..10usize]
        ) {
            let under_limit = (output_value as f32) * under_limit_ratio;

            let ratio_by_step = calculate_ratio_by_step(output_value, under_limit, steps);

            // ratio_by_stepをsteps回掛け合わせるとunder_ratioになることを確認
            // steps = 1 の場合は計算不要
            let total_ratio = if steps == 1 {
                ratio_by_step
            } else {
                ratio_by_step.powi(steps as i32)
            };
            let expected_ratio = under_limit / (output_value as f32);

            // 浮動小数点の精度の問題を考慮して、許容誤差を設定
            let epsilon = 0.001f32;

            // total_ratioがexpected_ratioに近いことを確認
            prop_assert!((total_ratio - expected_ratio).abs() <= epsilon);

            // 最終的な出力値がunder_limitに近いことを確認
            let final_output = (output_value as f32) * total_ratio;
            prop_assert!((final_output - under_limit).abs() <= epsilon * (output_value as f32));
        }
    }
}
