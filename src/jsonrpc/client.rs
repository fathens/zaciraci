use crate::jsonrpc::IS_MAINNET;
use near_jsonrpc_client::JsonRpcClient;
use once_cell::sync::Lazy;

pub static CLIENT: Lazy<JsonRpcClient> = Lazy::new(|| {
    if *IS_MAINNET {
        JsonRpcClient::connect(near_jsonrpc_client::NEAR_MAINNET_RPC_URL)
    } else {
        JsonRpcClient::connect(near_jsonrpc_client::NEAR_TESTNET_RPC_URL)
    }
});
