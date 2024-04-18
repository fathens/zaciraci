mod errors;
pub mod pool;

use near_sdk::AccountId;
use once_cell::sync::Lazy;

pub type Result<T> = std::result::Result<T, errors::Error>;

static CONTRACT_ADDRESS: Lazy<AccountId> = Lazy::new(|| "v2.ref-finance.near".parse().unwrap());
