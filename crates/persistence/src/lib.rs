#![deny(warnings)]

pub mod connection_pool;
pub mod evaluation_period;
pub mod pool_info;
pub mod prediction_record;
pub mod schema;
pub mod token_rate;
pub mod trade_transaction;

type Result<T> = anyhow::Result<T>;
