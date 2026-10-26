//! Persistence for the Ref Finance flow collected from NearBlocks.
//!
//! Tables (see `migrations/2026-10-26-000000_create_ref_flow_tables`):
//!
//! - `ref_flow_receipts` — append-only index of listed receipts
//! - `ref_flow_txs` — one row per successful transaction; the enrichment
//!   marker and the coverage denominator
//! - `ref_flow_trades` — executed trades, one per Ref receipt

pub mod receipt;
pub mod trade;
pub mod tx;

#[cfg(test)]
pub(crate) mod test_support;
