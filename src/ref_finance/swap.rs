use crate::logging::*;
use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount};
use crate::ref_finance::{path, CONTRACT_ADDRESS};
use crate::{jsonrpc, wallet, Result};
use near_sdk::json_types::U128;
use near_sdk::AccountId;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Single swap action.
#[derive(Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct SwapAction {
    /// Pool which should be used for swapping.
    pub pool_id: u64,
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

pub async fn run_swap(start: TokenInAccount, goal: TokenOutAccount, initial: u128) -> Result<u128> {
    let log = DEFAULT.new(o!(
        "function" => "run_swap",
        "start" => format!("{}", start),
        "goal" => format!("{}", goal),
        "initial" => initial,
    ));
    info!(log, "entered");
    let path = path::swap_path(start.clone(), goal.clone()).await?;
    let mut actions = Vec::new();
    let out = path
        .into_iter()
        .try_fold(initial, |prev, pair| -> Result<u128> {
            let amount_in = prev;
            let pool_id = pair.pool_id() as u64;
            let token_in = pair.token_in_id();
            let token_out = pair.token_out_id();
            let next_out = pair.estimate_return(initial)?;
            let action = SwapAction {
                pool_id,
                token_in: token_in.as_id().to_owned(),
                amount_in: Some(U128(amount_in)),
                token_out: token_out.as_id().to_owned(),
                min_amount_out: U128(next_out),
            };
            actions.push(action);
            Ok(next_out)
        })?;
    let args = json!({
        "actions": actions,
    });

    let deposit = 0;

    let signer = wallet::WALLET.signer();

    jsonrpc::exec_contract(&signer, &CONTRACT_ADDRESS, METHOD_NAME, &args, deposit).await?;

    Ok(out)
}
