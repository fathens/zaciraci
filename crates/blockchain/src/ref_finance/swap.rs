use crate::ref_finance::CONTRACT_ADDRESS;
use crate::wallet::Wallet;
use crate::{Result, jsonrpc};
use dex::{TokenPair, TokenPairLike};
use logging::*;
use near_primitives::views::{FinalExecutionOutcomeView, FinalExecutionStatus};
use near_sdk::json_types::U128;
use near_sdk::{AccountId, NearToken};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Single swap action.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct SwapAction {
    /// Pool which should be used for swapping.
    pub pool_id: u32,
    /// Token to swap from.
    pub token_in: AccountId,
    /// Amount to exchange.
    /// If amount_in is None, it will take amount_out from previous step.
    /// Will fail if amount_in is None on the first step.
    pub amount_in: Option<U128>,
    /// Token to swap into.
    pub token_out: AccountId,
    /// Required minimum amount of token_out.
    pub min_amount_out: U128,
}
const METHOD_NAME: &str = "swap";

#[derive(Debug, Clone)]
pub struct SwapArg {
    pub initial_in: u128,
    pub min_out: u128,
}

/// パスに沿って複数のスワップアクションを生成する関数
fn build_swap_actions<T>(path: &[T], arg: SwapArg) -> Result<(Vec<SwapAction>, u128)>
where
    T: TokenPairLike,
{
    let log = DEFAULT.new(o!(
        "function" => "build_swap_actions",
        "path.length" => format!("{}", path.len()),
        "initial" => arg.initial_in,
        "min_out" => arg.min_out,
    ));
    trace!(log, "building swap actions");
    if path.is_empty() {
        return Ok((Vec::new(), arg.initial_in));
    }

    let first_id = path[0].pool_id();
    let last_id = path[path.len() - 1].pool_id();

    let mut actions = Vec::new();
    let out = path
        .iter()
        .try_fold(arg.initial_in, |prev_out, pair| -> Result<u128> {
            let is_first = pair.pool_id() == first_id;
            let min_out = if pair.pool_id() == last_id {
                arg.min_out
            } else {
                0
            };
            let next_out = pair.estimate_return(prev_out)?;

            let action = SwapAction {
                pool_id: pair.pool_id(),
                token_in: pair.token_in_id().as_account_id().to_owned(),
                amount_in: is_first.then_some(U128(prev_out)),
                token_out: pair.token_out_id().as_account_id().to_owned(),
                min_amount_out: U128(min_out),
            };

            debug!(log, "created swap action";
                "pool_id" => ?action,
                "estimated_out" => next_out,
            );

            actions.push(action);
            Ok(next_out)
        })?;

    trace!(log, "finished building swap actions";
        o!("estimated_total_out" => format!("{:?}", out)),
    );
    Ok((actions, out))
}

pub async fn run_swap<A, W>(
    client: &A,
    wallet: &W,
    path: &[TokenPair],
    arg: SwapArg,
) -> Result<(A::Output, u128)>
where
    A: jsonrpc::SendTx,
    W: Wallet,
{
    let log = DEFAULT.new(o!(
        "function" => "run_swap",
        "path.length" => format!("{}", path.len()),
        "initial" => arg.initial_in,
        "min_out" => arg.min_out,
    ));
    trace!(log, "entered");

    let (actions, out) = build_swap_actions(path, arg)?;

    let args = json!({
        "actions": actions,
    });

    let deposit = NearToken::from_yoctonear(1);
    let signer = wallet.signer();

    let tx_hash = client
        .exec_contract(signer, &CONTRACT_ADDRESS, METHOD_NAME, args, deposit)
        .await?;

    Ok((tx_hash, out))
}

/// Extract the actual output amount from a successful swap transaction outcome.
///
/// # Contract assumption
///
/// REF Finance's `swap()` contract function returns the actual output token amount
/// as a JSON-encoded `U128` in `FinalExecutionStatus::SuccessValue`, even for
/// multi-hop swaps. If the contract changes this format (e.g. after an upgrade),
/// parsing will fail with a `warn` log and the caller should treat the result as
/// `None` (i.e. `actual_to_amount` = NULL in the database).
pub fn extract_actual_output(view: &FinalExecutionOutcomeView) -> Result<u128> {
    match &view.status {
        FinalExecutionStatus::SuccessValue(bytes) => {
            let amount: U128 = serde_json::from_slice(bytes).map_err(|e| {
                let log = DEFAULT.new(o!("function" => "extract_actual_output"));
                let raw_str = String::from_utf8_lossy(bytes);
                let hex_str: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
                warn!(log, "failed to parse SuccessValue as U128";
                    "error" => %e,
                    "raw_bytes_utf8" => %raw_str,
                    "raw_bytes_hex" => hex_str,
                );
                e
            })?;
            Ok(amount.0)
        }
        FinalExecutionStatus::Failure(err) => Err(anyhow::anyhow!("Transaction failed: {:?}", err)),
        _ => Err(anyhow::anyhow!(
            "Transaction did not complete: {:?}",
            view.status
        )),
    }
}

#[cfg(test)]
mod tests;
