//! Shared DB helpers for `ref_flow` tests, also exported to downstream
//! crates under the `test-helpers` feature. Every test that touches the
//! `ref_flow_*` tables must run under `#[serial(ref_flow_db)]` and start
//! with [`clean_tables`].

use crate::Result;
use crate::connection_pool;
use crate::schema::{
    pool_info, ref_flow_collector_state, ref_flow_ledger, ref_flow_receipts, ref_flow_runs,
    ref_flow_trades, ref_flow_txs,
};
use anyhow::anyhow;
use chrono::NaiveDateTime;
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

/// Pool ids at or above this value are reserved for fee-lookup fixtures;
/// [`clear_pool_fee_snapshots`] removes them.
pub const FEE_FIXTURE_POOL_ID_MIN: u32 = 900_000;

/// Insert one `pool_info` snapshot directly. `pool_info::batch_insert` is
/// avoided on purpose: it spawns the retention DELETE over the whole
/// (multi-million row) table, which makes tests slow and flaky. Tests
/// using this must also hold `#[serial(pool_info)]`.
pub async fn insert_pool_fee_snapshot(pool_id: u32, fee: u32, at: NaiveDateTime) -> Result<()> {
    let pool_id = i32::try_from(pool_id).map_err(|_| anyhow!("pool_id {pool_id} exceeds i32"))?;
    let fee = i32::try_from(fee).map_err(|_| anyhow!("fee {fee} exceeds i32"))?;
    let conn = connection_pool::get_test_only().await?;
    conn.interact(move |conn| {
        diesel::insert_into(pool_info::table)
            .values((
                pool_info::pool_id.eq(pool_id),
                pool_info::pool_kind.eq("SIMPLE_POOL"),
                pool_info::token_account_ids.eq(serde_json::json!(["a.near", "b.near"])),
                pool_info::amounts.eq(serde_json::json!(["1", "1"])),
                pool_info::total_fee.eq(fee),
                pool_info::shares_total_supply.eq(serde_json::json!("1")),
                pool_info::amp.eq(0i64),
                pool_info::timestamp.eq(at),
            ))
            .execute(conn)
    })
    .await
    .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    Ok(())
}

/// Remove every fixture snapshot (pool ids >= [`FEE_FIXTURE_POOL_ID_MIN`]).
pub async fn clear_pool_fee_snapshots() -> Result<()> {
    let conn = connection_pool::get_test_only().await?;
    conn.interact(|conn| {
        diesel::delete(pool_info::table.filter(
            pool_info::pool_id.ge(
                i32::try_from(FEE_FIXTURE_POOL_ID_MIN).expect("fixture pool id range fits i32"),
            ),
        ))
        .execute(conn)
    })
    .await
    .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    Ok(())
}
