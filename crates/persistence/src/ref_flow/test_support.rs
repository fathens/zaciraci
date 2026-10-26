//! Shared helpers for `ref_flow` DB tests. Every test that touches the
//! `ref_flow_*` tables must run under `#[serial(ref_flow_db)]` and start
//! with [`clean_tables`].

use crate::Result;
use crate::connection_pool;
use crate::schema::{ref_flow_receipts, ref_flow_trades, ref_flow_txs};
use anyhow::anyhow;
use diesel::prelude::*;

/// Remove every row from the three `ref_flow_*` tables.
pub async fn clean_tables() -> Result<()> {
    let conn = connection_pool::get_test_only().await?;
    conn.interact(|conn| -> QueryResult<()> {
        diesel::delete(ref_flow_trades::table).execute(conn)?;
        diesel::delete(ref_flow_txs::table).execute(conn)?;
        diesel::delete(ref_flow_receipts::table).execute(conn)?;
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
