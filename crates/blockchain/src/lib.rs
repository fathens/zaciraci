#![deny(warnings)]
#![allow(async_fn_in_trait)]

pub mod jsonrpc;
pub mod ref_finance;
pub mod types;
pub mod wallet;

pub(crate) use common::config;

pub type Result<T> = anyhow::Result<T>;
