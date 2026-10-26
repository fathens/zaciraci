//! Shared helpers for `ref_flow` DB tests. Every test that touches the
//! `ref_flow_*` tables must run under `#[serial(ref_flow_db)]` and start
//! with [`clean_tables`].

use crate::Result;
use crate::connection_pool;
use crate::schema::{
    ref_flow_collector_state, ref_flow_ledger, ref_flow_receipts, ref_flow_runs, ref_flow_trades,
    ref_flow_txs,
};
use anyhow::anyhow;
use chrono::NaiveDateTime;
use common::types::{ReceiptId, TxHash};
use diesel::prelude::*;

/// Remove every row from all `ref_flow_*` tables.
pub async fn clean_tables() -> Result<()> {
    let conn = connection_pool::get_test_only().await?;
    conn.interact(|conn| -> QueryResult<()> {
        diesel::delete(ref_flow_trades::table).execute(conn)?;
        diesel::delete(ref_flow_txs::table).execute(conn)?;
        diesel::delete(ref_flow_receipts::table).execute(conn)?;
        diesel::delete(ref_flow_collector_state::table).execute(conn)?;
        diesel::delete(ref_flow_ledger::table).execute(conn)?;
        diesel::delete(ref_flow_runs::table).execute(conn)?;
        Ok(())
    })
    .await
    .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    Ok(())
}

/// Row count of `ref_flow_txs`.
pub async fn count_txs() -> Result<i64> {
    let conn = connection_pool::get_test_only().await?;
    let n = conn
        .interact(|conn| ref_flow_txs::table.count().get_result::<i64>(conn))
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    Ok(n)
}

/// Row count of `ref_flow_receipts`.
pub async fn count_receipts() -> Result<i64> {
    let conn = connection_pool::get_test_only().await?;
    let n = conn
        .interact(|conn| ref_flow_receipts::table.count().get_result::<i64>(conn))
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    Ok(n)
}

/// Deterministic 32-byte hash encodings for test ids: `hash(n)` is the
/// base58 form of `[n; 32]`-like bytes, unique per `n`.
pub fn hash(n: u32) -> String {
    let mut bytes = [0u8; 32];
    bytes[..4].copy_from_slice(&n.to_be_bytes());
    bytes[31] = 1; // avoid the all-zero / leading-'1' degenerate case
    bs58::encode(bytes).into_string()
}

pub fn receipt_id(n: u32) -> ReceiptId {
    ReceiptId::new(&hash(1_000_000 + n)).unwrap()
}

pub fn tx_hash(n: u32) -> TxHash {
    TxHash::new(&hash(n)).unwrap()
}

pub fn ts(secs: i64) -> NaiveDateTime {
    chrono::DateTime::from_timestamp(1_792_800_000 + secs, 0)
        .unwrap()
        .naive_utc()
}
