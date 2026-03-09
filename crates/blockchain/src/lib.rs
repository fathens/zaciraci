#![deny(warnings)]
#![allow(async_fn_in_trait)]

pub mod jsonrpc;
pub mod ref_finance;
pub mod test_utils;
pub mod types;
pub mod wallet;

pub type Result<T> = anyhow::Result<T>;
