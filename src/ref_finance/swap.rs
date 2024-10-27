use crate::logging::*;
use crate::ref_finance::pool_info::PoolInfoList;
use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount};
use crate::ref_finance::{path, CLIENT, CONTRACT_ADDRESS};
use crate::{wallet, Result};
use near_jsonrpc_client::methods;
use near_primitives::action::{Action, FunctionCallAction};
use near_primitives::hash::CryptoHash;
use near_primitives::transaction::{SignedTransaction, Transaction, TransactionV1};
use near_sdk::json_types::U128;
use near_sdk::{AccountId, Gas};
use serde::{Deserialize, Serialize};

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

pub async fn run_swap(
    pools: PoolInfoList,
    start: TokenInAccount,
    goal: TokenOutAccount,
    initial: u128,
) -> Result<u128> {
    let log = DEFAULT.new(o!(
        "function" => "run_swap",
        "start" => format!("{}", start),
        "goal" => format!("{}", goal),
        "initial" => initial,
    ));
    info!(log, "entered");
    let path = path::swap_path(pools, start.clone(), goal.clone())?;
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
                token_in: token_in.as_id().clone(),
                amount_in: Some(U128(amount_in)),
                token_out: token_out.as_id().clone(),
                min_amount_out: U128(next_out),
            };
            actions.push(action);
            Ok(next_out)
        })?;

    let action = Action::FunctionCall(
        FunctionCallAction {
            method_name: "swap".to_string(),
            args: serde_json::to_vec(&actions)?,
            gas: Gas::from_tgas(1).as_gas(),
            deposit: 0,
        }
        .into(),
    );

    let signer = wallet::WALLET.signer();

    let transaction = Transaction::V1(TransactionV1 {
        signer_id: signer.account_id.clone(),
        public_key: signer.public_key(),
        nonce: 0,
        receiver_id: CONTRACT_ADDRESS.clone(),
        block_hash: CryptoHash::default(),
        actions: vec![action],
        priority_fee: 0,
    });

    let (hash, _) = transaction.get_hash_and_size();
    let signature = signer.sign(hash.as_bytes());
    let _signed_tx = SignedTransaction::new(signature, transaction);

    let request = methods::broadcast_tx_async::RpcBroadcastTxAsyncRequest {
        signed_transaction: _signed_tx,
    };

    let response = CLIENT.call(request).await?;
    info!(log, "broadcasted"; "response" => format!("{:?}", response));

    Ok(out)
}
