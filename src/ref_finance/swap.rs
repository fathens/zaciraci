use crate::logging::*;
use crate::ref_finance::pool_info::{TokenPair, TokenPairLike};
use crate::ref_finance::token_account::TokenAccount;
use crate::ref_finance::CONTRACT_ADDRESS;
use crate::wallet::Wallet;
use crate::{jsonrpc, Result};
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

/// 単一のスワップアクションを生成する関数
pub fn create_swap_action<T>(
    pair: &T,
    prev_amount: Balance,
    is_first_swap: bool,
    min_out_ratio: f32,
) -> Result<(SwapAction, Balance)>
where
    T: TokenPairLike,
{
    let amount_in = is_first_swap.then_some(U128(prev_amount));
    let pool_id = pair.pool_id();
    let token_in = pair.token_in_id();
    let token_out = pair.token_out_id();
    let next_out = pair.estimate_return(prev_amount)?;
    let min_out = ((next_out as f32) * min_out_ratio) as Balance;

    let action = SwapAction {
        pool_id,
        token_in: token_in.as_id().to_owned(),
        amount_in,
        token_out: token_out.as_id().to_owned(),
        min_amount_out: U128(min_out),
    };

    Ok((action, next_out))
}

/// パスに沿って複数のスワップアクションを生成する関数
pub fn build_swap_actions<T>(
    path: &[T],
    initial: Balance,
    min_out_ratio: f32,
) -> Result<(Vec<SwapAction>, Balance)>
where
    T: TokenPairLike,
{
    let mut actions = Vec::new();
    let out = path
        .iter()
        .try_fold(initial, |prev_out, pair| -> Result<Balance> {
            let is_first = prev_out == initial;
            let (action, next_out) = create_swap_action(pair, prev_out, is_first, min_out_ratio)?;
            actions.push(action);
            Ok(next_out)
        })?;

    Ok((actions, out))
}

pub async fn run_swap<A, W>(
    client: &A,
    wallet: &W,
    path: &[TokenPair],
    initial: Balance,
    min_out_ratio: f32,
) -> Result<(A::Output, Balance)>
where
    A: jsonrpc::SendTx,
    W: Wallet,
{
    let log = DEFAULT.new(o!(
        "function" => "run_swap",
        "path.length" => format!("{}", path.len()),
        "initial" => initial,
        "min_out_ratio" => min_out_ratio,
    ));
    info!(log, "entered");

    // ジェネリックな build_swap_actions を使用
    let (actions, out) = build_swap_actions(path, initial, min_out_ratio)?;

    // デバッグログ
    if log.is_debug_enabled() {
        for action in &actions {
            debug!(log, "swap action";
                "pool_id" => action.pool_id,
                "token_in" => format!("{}", action.token_in),
                "amount_in" => action.amount_in.map_or(0, |u| u.0),
                "token_out" => format!("{}", action.token_out),
                "min_amount_out" => action.min_amount_out.0,
            );
        }
    }

    let args = json!({
        "actions": actions,
    });

    let deposit = 1;
    let signer = wallet.signer();

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
