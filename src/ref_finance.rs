pub mod errors;
pub mod path;
pub mod pool_info;
pub mod token_account;
mod token_index;

use crate::config;
use crate::logging::*;
use near_jsonrpc_client::JsonRpcClient;
use near_sdk::AccountId;
use once_cell::sync::Lazy;

static IS_TESTNET: Lazy<bool> = Lazy::new(|| {
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

static CONTRACT_ADDRESS: Lazy<AccountId> = Lazy::new(|| {
    if *IS_TESTNET {
        "ref-finance-101.testnet".parse().unwrap()
    } else {
        "v2.ref-finance.near".parse().unwrap()
    }
});
static CLIENT: Lazy<JsonRpcClient> = Lazy::new(|| {
    if *IS_TESTNET {
        JsonRpcClient::connect(near_jsonrpc_client::NEAR_TESTNET_RPC_URL)
    } else {
        JsonRpcClient::connect(near_jsonrpc_client::NEAR_MAINNET_RPC_URL)
    }
});
