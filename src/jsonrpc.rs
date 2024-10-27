use crate::config;
use crate::logging::DEFAULT;
use crate::Result;
use near_crypto::InMemorySigner;
use near_jsonrpc_client::{methods, JsonRpcClient};
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use near_primitives::types::Finality;
use near_primitives::views::{AccessKeyView, BlockView, QueryRequest};
use once_cell::sync::Lazy;
use slog::{info, o};

pub static IS_TESTNET: Lazy<bool> = Lazy::new(|| {
    let str = config::get("USE_TESTNET").unwrap_or_default();
    let log = DEFAULT.new(o!(
        "function" => "IS_TESTNET",
        "given_value" => format!("{}", str),
    ));
    let value = str.parse().unwrap_or_default();
    if value {
        info!(log, "Using testnet");
    } else {
        info!(log, "Using mainnet");
    }
    value
});

pub static CLIENT: Lazy<JsonRpcClient> = Lazy::new(|| {
    if *IS_TESTNET {
        JsonRpcClient::connect(near_jsonrpc_client::NEAR_TESTNET_RPC_URL)
    } else {
        JsonRpcClient::connect(near_jsonrpc_client::NEAR_MAINNET_RPC_URL)
    }
});

pub async fn get_recent_block() -> Result<BlockView> {
    let req = methods::block::RpcBlockRequest {
        block_reference: Finality::Final.into(),
    };
    let res = CLIENT.call(req).await?;
    Ok(res)
}

pub async fn get_access_key_info(signer: InMemorySigner) -> Result<AccessKeyView> {
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
