#![deny(warnings)]
#![allow(async_fn_in_trait)]

pub mod jsonrpc;
pub mod mock;
pub mod ref_finance;
pub mod types;
pub mod wallet;

pub type Result<T> = anyhow::Result<T>;
