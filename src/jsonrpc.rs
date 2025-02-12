mod client;

use crate::config;
use crate::logging::*;
use crate::types::gas_price::GasPrice;
use crate::Result;
use anyhow::anyhow;
use near_crypto::InMemorySigner;
use near_jsonrpc_client::methods;
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use near_jsonrpc_primitives::types::transactions::RpcTransactionResponse;
use near_primitives::action::{Action, FunctionCallAction, TransferAction};
use near_primitives::hash::CryptoHash;
use near_primitives::transaction::{SignedTransaction, Transaction, TransactionV0};
use near_primitives::types::{Balance, BlockId, Finality};
use near_primitives::views::{
    AccessKeyView, BlockView, CallResult, FinalExecutionOutcomeViewEnum, QueryRequest,
    TxExecutionStatus,
};
use near_sdk::{AccountId, Gas};
use once_cell::sync::Lazy;

use client::CLIENT;

pub static IS_MAINNET: Lazy<bool> = Lazy::new(|| {
    let str = config::get("USE_MAINNET").unwrap_or_default();
    let log = DEFAULT.new(o!(
        "function" => "IS_MAINNET",
        "given_value" => format!("{}", str),
    ));
    let value = str.parse().unwrap_or_default();
    if value {
        info!(log, "Using mainnet");
    } else {
        info!(log, "Using testnet");
    }
    value
});

pub async fn get_recent_block() -> Result<BlockView> {
    let req = methods::block::RpcBlockRequest {
        block_reference: Finality::Final.into(),
    };
    let res = CLIENT.call(req).await?;
    Ok(res)
}

pub async fn get_native_amount(account: &AccountId) -> Result<Balance> {
    let req = methods::query::RpcQueryRequest {
        block_reference: Finality::Final.into(),
        request: QueryRequest::ViewAccount {
            account_id: account.clone(),
        },
    };
    let res = CLIENT.call(req).await?;
    if let QueryResponseKind::ViewAccount(am) = res.kind {
        Ok(am.amount)
    } else {
        panic!("View account is not view account")
    }
}

pub async fn get_gas_price(block: Option<BlockId>) -> Result<GasPrice> {
    let req = methods::gas_price::RpcGasPriceRequest { block_id: block };
    let res = CLIENT.call(req).await?;
    Ok(GasPrice::from_balance(res.gas_price))
}

pub async fn get_access_key_info(signer: &InMemorySigner) -> Result<AccessKeyView> {
    let req = methods::query::RpcQueryRequest {
        block_reference: Finality::Final.into(),
        request: QueryRequest::ViewAccessKey {
            account_id: signer.account_id.clone(),
            public_key: signer.public_key(),
        },
    };
    let res = CLIENT.call(req).await?;
    match res.kind {
        QueryResponseKind::AccessKey(access_key) => Ok(access_key),
        _ => panic!("unexpected response"),
    }
}

pub async fn wait_tx_result(
    sender: &AccountId,
    tx_hash: &CryptoHash,
    wait_until: TxExecutionStatus,
) -> Result<RpcTransactionResponse> {
    let log = DEFAULT.new(o!(
        "function" => "wait_tx_result",
        "sender" => format!("{}", sender),
        "tx_hash" => format!("{}", tx_hash),
        "wait_until" => format!("{:?}", wait_until),
    ));
    info!(log, "asking for transaction status");
    let req = methods::tx::RpcTransactionStatusRequest {
        transaction_info: methods::tx::TransactionInfo::TransactionId {
            tx_hash: tx_hash.to_owned(),
            sender_account_id: sender.clone(),
        },
        wait_until,
    };
    let res = CLIENT.call(req).await?;
    info!(log, "Transaction status";
        "outcome" => format!("{:?}", res.final_execution_outcome),
        "status" => format!("{:?}", res.final_execution_status),
    );
    Ok(res)
}

pub async fn wait_tx_executed(
    sender: &AccountId,
    tx_hash: &CryptoHash,
) -> Result<FinalExecutionOutcomeViewEnum> {
    let res = wait_tx_result(sender, tx_hash, TxExecutionStatus::Executed).await?;
    res.final_execution_outcome
        .ok_or_else(|| anyhow!("No outcome of tx: {}", tx_hash))
}

pub async fn view_contract<T>(
    receiver: &AccountId,
    method_name: &str,
    args: &T,
) -> Result<CallResult>
where
    T: ?Sized + serde::Serialize,
{
    let req = methods::query::RpcQueryRequest {
        block_reference: Finality::Final.into(),
        request: QueryRequest::CallFunction {
            account_id: receiver.clone(),
            method_name: method_name.to_string(),
            args: serde_json::to_vec(args)?.into(),
        },
    };
    let res = CLIENT.call(req).await?;
    match res.kind {
        QueryResponseKind::CallResult(r) => Ok(r),
        _ => panic!("unexpected response"),
    }
}

pub async fn transfer_native_token(
    signer: &InMemorySigner,
    receiver: &AccountId,
    amount: Balance,
) -> Result<CryptoHash> {
    let log = DEFAULT.new(o!(
        "function" => "transfer_native_token",
        "signer" => format!("{}", signer.account_id),
        "receiver" => format!("{}", receiver),
        "amount" => amount,
    ));
    info!(log, "transferring native token");
    let action = Action::Transfer(TransferAction { deposit: amount });

    send_tx(signer, receiver, &[action]).await
}

pub async fn exec_contract<T>(
    signer: &InMemorySigner,
    receiver: &AccountId,
    method_name: &str,
    args: &T,
    deposit: Balance,
) -> Result<CryptoHash>
where
    T: ?Sized + serde::Serialize,
{
    let log = DEFAULT.new(o!(
        "function" => "exec_contract",
        "signer" => format!("{}", signer.account_id),
        "receiver" => format!("{}", receiver),
        "method_name" => format!("{}", method_name),
        "deposit" => deposit,
    ));
    info!(log, "executing contract");

    let action = Action::FunctionCall(
        FunctionCallAction {
            method_name: method_name.to_string(),
            args: serde_json::to_vec(&args)?,
            gas: Gas::from_tgas(300).as_gas(),
            deposit,
        }
        .into(),
    );

    send_tx(signer, receiver, &[action]).await
}

async fn send_tx(
    signer: &InMemorySigner,
    receiver: &AccountId,
    actions: &[Action],
) -> Result<CryptoHash> {
    let log = DEFAULT.new(o!(
        "function" => "exec_contract",
        "signer" => format!("{}", signer.account_id),
        "receiver" => format!("{}", receiver),
    ));

    let access_key = get_access_key_info(signer).await?;
    let block = get_recent_block().await?;
    let nonce = access_key.nonce + 1;
    let block_hash = block.header.hash;

    let transaction = Transaction::V0(TransactionV0 {
        signer_id: signer.account_id.clone(),
        public_key: signer.public_key(),
        nonce,
        receiver_id: receiver.clone(),
        block_hash,
        actions: actions.to_vec(),
    });

    let (hash, _) = transaction.get_hash_and_size();
    let signature = signer.sign(hash.as_bytes());
    let signed_tx = SignedTransaction::new(signature, transaction);

    let req = methods::broadcast_tx_async::RpcBroadcastTxAsyncRequest {
        signed_transaction: signed_tx,
    };

    let res = CLIENT.call(req).await?;
    info!(log, "broadcasted";
        "response" => %res,
        "nonce" => nonce,
        "block_hash" => %block_hash,
        "public_key" => %signer.public_key(),
    );
    Ok(res)
}
