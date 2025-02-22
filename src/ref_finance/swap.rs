use crate::logging::*;
use crate::ref_finance::pool_info::TokenPair;
use crate::ref_finance::token_account::TokenAccount;
use crate::ref_finance::CONTRACT_ADDRESS;
use crate::{jsonrpc, wallet, Result};
use near_primitives::types::Balance;
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

pub async fn run_swap<A>(
    client: &A,
    path: &[TokenPair],
    initial: Balance,
    min_out_ratio: f32,
) -> Result<(A::Output, Balance)>
where
    A: jsonrpc::SendTx,
    A: 'static,
{
    let log = DEFAULT.new(o!(
        "function" => "run_swap",
        "path.length" => format!("{}", path.len()),
        "initial" => initial,
    ));
    info!(log, "entered");

    let mut actions = Vec::new();
    let out = path
        .iter()
        .try_fold(initial, |prev, pair| -> Result<Balance> {
            let amount_in = (prev == initial).then_some(U128(prev));
            let pool_id = pair.pool_id() as u64;
            let token_in = pair.token_in_id();
            let token_out = pair.token_out_id();
            let next_out = pair.estimate_return(prev)?;
            let min_out = ((next_out as f32) * min_out_ratio) as Balance;
            debug!(log, "adding swap action";
                "pool_id" => pool_id,
                "token_in" => format!("{}", token_in),
                "amount_in" => prev,
                "token_out" => format!("{}", token_out),
                "next_out" => next_out,
                "min_out" => min_out,
            );
            let action = SwapAction {
                pool_id,
                token_in: token_in.as_id().to_owned(),
                amount_in,
                token_out: token_out.as_id().to_owned(),
                min_amount_out: U128(min_out),
            };
            actions.push(action);
            Ok(min_out)
        })?;
    let args = json!({
        "actions": actions,
    });

    let deposit = 1;
    let signer = wallet::WALLET.signer();

    let tx_hash = client
        .exec_contract(signer, &CONTRACT_ADDRESS, METHOD_NAME, args, deposit)
        .await?;

    Ok((tx_hash, out))
}

pub fn gather_token_accounts(pairs_list: &[&[TokenPair]]) -> Vec<TokenAccount> {
    let mut tokens = Vec::new();
    for pairs in pairs_list.iter() {
        for pair in pairs.iter() {
            tokens.push(pair.token_in_id().into());
            tokens.push(pair.token_out_id().into());
        }
    }
    tokens.sort();
    tokens.dedup();
    tokens
}
