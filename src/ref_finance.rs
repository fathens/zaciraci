pub mod deposit;
pub mod errors;
mod history;
pub mod path;
pub mod pool_info;
pub mod storage;
pub mod swap;
pub mod token_account;
mod token_index;

use crate::logging::*;
use near_sdk::AccountId;
use once_cell::sync::Lazy;

static CONTRACT_ADDRESS: Lazy<AccountId> = Lazy::new(|| {
    let log = DEFAULT.new(o!("function" => "ref_finance::CONTRACT_ADDRESS"));
    let account_id = if *crate::jsonrpc::IS_MAINNET {
        "v2.ref-finance.near".parse().unwrap()
    } else {
        "ref-finance-101.testnet".parse().unwrap()
    };
    info!(log, "contract address"; "account_id" => %account_id);
    account_id
});
