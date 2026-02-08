pub mod balances;
pub mod deposit;
pub mod errors;
mod history;
pub mod path;
pub mod pool_info;
pub mod storage;
pub mod swap;
pub mod token_account;

use logging::*;
use near_sdk::AccountId;
use once_cell::sync::Lazy;

pub static CONTRACT_ADDRESS: Lazy<AccountId> = Lazy::new(|| {
    let log = DEFAULT.new(o!("function" => "ref_finance::CONTRACT_ADDRESS"));
    let account_id = if *crate::jsonrpc::IS_MAINNET {
        "v2.ref-finance.near"
            .parse()
            .expect("valid AccountId literal")
    } else {
        "ref-finance-101.testnet"
            .parse()
            .expect("valid AccountId literal")
    };
    info!(log, "contract address"; "account_id" => %account_id);
    account_id
});
