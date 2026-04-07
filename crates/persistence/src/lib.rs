#![deny(warnings)]

pub mod authorized_users;
pub mod config_store;
pub mod connection_pool;
pub mod evaluation_period;
pub mod maintenance;
pub mod pool_info;
pub mod portfolio_holding;
pub mod prediction_record;
pub mod schema;
pub mod token_rate;
pub mod trade_transaction;

type Result<T> = anyhow::Result<T>;
