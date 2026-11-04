#![deny(warnings)]

//! NearBlocks Ref Finance flow collector.
//!
//! Lists the receipts of `v2.ref-finance.near`, enriches every successful
//! transaction with its executed trades and stores both through
//! `persistence::ref_flow`. This crate owns the policy that was spread over
//! the Python collector's `budget.py` / `collect.py`: request budgeting,
//! rate limiting, response parsing and the cycle orchestration.
//!
//! Operations (configuration, budget, cutover, monitoring) are documented
//! on the wiki: <https://github.com/fathens/zaciraci/wiki/NearBlock-同期>.

pub mod accounting;
pub mod budget;
pub mod client;
pub mod collector;
pub mod credit;
#[cfg(feature = "importer")]
pub mod import;
pub mod parse;
pub mod rate_limit;
pub mod runner;
pub mod state;

pub use runner::run;
