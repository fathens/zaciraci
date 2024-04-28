pub mod errors;
pub mod pool;

use near_jsonrpc_client::JsonRpcClient;
use near_sdk::AccountId;
use once_cell::sync::Lazy;

static CONTRACT_ADDRESS: Lazy<AccountId> = Lazy::new(|| "v2.ref-finance.near".parse().unwrap());
static CLIENT: Lazy<JsonRpcClient> =
    Lazy::new(|| JsonRpcClient::connect(near_jsonrpc_client::NEAR_MAINNET_RPC_URL));
