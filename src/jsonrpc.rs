use crate::config;
use crate::logging::*;
use crate::Result;
use near_crypto::InMemorySigner;
use near_jsonrpc_client::{methods, JsonRpcClient};
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use near_primitives::action::{Action, FunctionCallAction};
use near_primitives::transaction::{SignedTransaction, Transaction, TransactionV0};
use near_primitives::types::{Balance, BlockId, Finality};
use near_primitives::views::{AccessKeyView, BlockView, CallResult, QueryRequest};
use near_sdk::{AccountId, CryptoHash, Gas};
use once_cell::sync::Lazy;

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

pub static CLIENT: Lazy<JsonRpcClient> = Lazy::new(|| {
    if *IS_MAINNET {
        JsonRpcClient::connect(near_jsonrpc_client::NEAR_MAINNET_RPC_URL)
    } else {
        JsonRpcClient::connect(near_jsonrpc_client::NEAR_TESTNET_RPC_URL)
    }
});

pub async fn get_recent_block() -> Result<BlockView> {
    let req = methods::block::RpcBlockRequest {
        block_reference: Finality::Final.into(),
    };
    let res = CLIENT.call(req).await?;
    Ok(res)
}

pub async fn get_native_amount() -> Result<Balance> {
    let req = methods::query::RpcQueryRequest {
        block_reference: Finality::Final.into(),
        request: QueryRequest::ViewAccount {
            account_id: "account.testnet".parse().unwrap(),
        },
    };
    let res = CLIENT.call(req).await?;
    if let QueryResponseKind::ViewAccount(am) = res.kind {
        Ok(am.amount)
    } else {
        panic!("View account is not view account")
    }
}

pub async fn get_gas_price(block: Option<BlockId>) -> Result<Balance> {
    let req = methods::gas_price::RpcGasPriceRequest { block_id: block };
    let res = CLIENT.call(req).await?;
    Ok(res.gas_price)
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
        "server" => CLIENT.server_addr(),
        "signer" => format!("{}", signer.account_id),
        "receiver" => format!("{}", receiver),
        "method_name" => format!("{}", method_name),
        "deposit" => deposit,
    ));

    let access_key = get_access_key_info(signer).await?;
    let block = get_recent_block().await?;
    let nonce = access_key.nonce + 1;
    let block_hash = block.header.hash;

    let action = Action::FunctionCall(
        FunctionCallAction {
            method_name: method_name.to_string(),
            args: serde_json::to_vec(&args)?,
            gas: Gas::from_tgas(300).as_gas(),
            deposit,
        }
        .into(),
    );

    let transaction = Transaction::V0(TransactionV0 {
        signer_id: signer.account_id.clone(),
        public_key: signer.public_key(),
        nonce,
        receiver_id: receiver.clone(),
        block_hash,
        actions: vec![action],
    });

    let (hash, _) = transaction.get_hash_and_size();
    let signature = signer.sign(hash.as_bytes());
    let signed_tx = SignedTransaction::new(signature, transaction);

    let req = methods::broadcast_tx_async::RpcBroadcastTxAsyncRequest {
        signed_transaction: signed_tx,
    };

    let res = CLIENT.call(req).await?;
    info!(log, "broadcasted";
        "response" => format!("{:?}", res),
        "nonce" => nonce,
        "block_hash" => format!("{}", block_hash),
        "public_key" => format!("{}", signer.public_key()),
    );
    Ok(res.0)
}
