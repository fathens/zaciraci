//! Persistence for the Ref Finance flow collected from NearBlocks.
//!
//! Tables (see `migrations/2026-10-26-000000_create_ref_flow_tables`):
//!
//! - `ref_flow_receipts` — append-only index of listed receipts
//! - `ref_flow_txs` — one row per successful transaction; the enrichment
//!   marker and the coverage denominator
//! - `ref_flow_trades` — executed trades, one per Ref receipt
//! - `ref_flow_collector_state`, `ref_flow_ledger`, `ref_flow_runs` — the
//!   collector's cursor state (compare-and-swap), daily request ledger and
//!   per-pass run log

pub mod ledger;
pub mod receipt;
pub mod runs;
pub mod state;
pub mod trade;
pub mod tx;

use crate::Result;
use crate::connection_pool;
use anyhow::anyhow;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use logging::*;

/// Minimum retention. The NearBlocks monthly quota is accounted over a
/// rolling 31-day window, and the collector's gap handling may need to
/// re-list up to that far back, so nothing younger is ever deleted.
pub const MIN_RETENTION_DAYS: u32 = 31;

/// Rows deleted per statement. Keeps each DELETE's lock hold and WAL
/// burst bounded on the shared production instance.
const CLEANUP_BATCH_ROWS: i64 = 50_000;

/// Outcome of [`cleanup_old_records`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CleanupStats {
    pub txs_deleted: usize,
    pub receipts_deleted: usize,
}

/// Delete receipts and frame rows older than `retention_days` (trades go
/// with their frame row via `ON DELETE CASCADE`). Both tables use the same
/// cutoff so the coverage denominator and the trades never drift apart.
pub async fn cleanup_old_records(retention_days: u32) -> Result<CleanupStats> {
    let log = DEFAULT.new(o!(
        "function" => "ref_flow::cleanup_old_records",
        "retention_days" => retention_days,
    ));
    if retention_days == 0 {
        warn!(
            log,
            "retention_days is 0, skipping cleanup to prevent deleting all records"
        );
        return Ok(CleanupStats::default());
    }
    let effective_days = retention_days.max(MIN_RETENTION_DAYS);
    if effective_days != retention_days {
        warn!(log, "retention_days below minimum, using minimum";
            "requested" => retention_days, "effective" => effective_days);
    }
    let cutoff =
        chrono::Utc::now().naive_utc() - chrono::TimeDelta::days(i64::from(effective_days));
    let stats = cleanup_before(cutoff, CLEANUP_BATCH_ROWS).await?;
    info!(log, "finish";
        "txs_deleted" => stats.txs_deleted,
        "receipts_deleted" => stats.receipts_deleted,
    );
    Ok(stats)
}

/// Delete rows with `block_time < cutoff` in batches of `batch_rows`.
async fn cleanup_before(cutoff: NaiveDateTime, batch_rows: i64) -> Result<CleanupStats> {
    use diesel::sql_types::{BigInt, Timestamp};

    // Bind parameters only; the table names are literals.
    const DELETE_TXS: &str = "DELETE FROM ref_flow_txs WHERE tx_hash IN \
        (SELECT tx_hash FROM ref_flow_txs WHERE block_time < $1 LIMIT $2)";
    const DELETE_RECEIPTS: &str = "DELETE FROM ref_flow_receipts WHERE receipt_id IN \
        (SELECT receipt_id FROM ref_flow_receipts WHERE block_time < $1 LIMIT $2)";

    let conn = connection_pool::get().await?;
    let stats = conn
        .interact(move |conn| -> QueryResult<CleanupStats> {
            let mut stats = CleanupStats::default();
            for (statement, counter) in [
                (DELETE_TXS, &mut stats.txs_deleted),
                (DELETE_RECEIPTS, &mut stats.receipts_deleted),
            ] {
                loop {
                    let n = diesel::sql_query(statement)
                        .bind::<Timestamp, _>(cutoff)
                        .bind::<BigInt, _>(batch_rows)
                        .execute(conn)?;
                    *counter += n;
                    if n == 0 {
                        break;
                    }
                }
            }
            Ok(stats)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    Ok(stats)
}

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod tests;
