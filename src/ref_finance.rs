pub mod errors;
pub mod path;
pub mod pool_info;
pub mod token_account;
mod token_index;

use near_jsonrpc_client::JsonRpcClient;
use near_sdk::AccountId;
use once_cell::sync::Lazy;

static CONTRACT_ADDRESS: Lazy<AccountId> = Lazy::new(|| {
    if is_testnet() {
        "ref-finance-101.testnet".parse().unwrap()
    } else {
        "v2.ref-finance.near".parse().unwrap()
    }
});
static CLIENT: Lazy<JsonRpcClient> = Lazy::new(|| {
    if is_testnet() {
        JsonRpcClient::connect(near_jsonrpc_client::NEAR_TESTNET_RPC_URL)
    } else {
        JsonRpcClient::connect(near_jsonrpc_client::NEAR_MAINNET_RPC_URL)
    }
});

pub fn is_testnet() -> bool {
    std::env::var("USE_TESTNET").is_ok()
}
