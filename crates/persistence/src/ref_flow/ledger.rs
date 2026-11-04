//! `ref_flow_ledger`: NearBlocks requests charged per UTC day.
//!
//! The collector charges requests *before* sending them, so the stored
//! count is an upper bound of what the provider has seen. The budget
//! policy (rolling 31-day and daily walls) reads this table; nothing ever
//! deletes from it.

use crate::Result;
use crate::connection_pool;
use crate::schema::ref_flow_ledger;
use anyhow::{Context, anyhow};
use chrono::NaiveDate;
use diesel::prelude::*;
use diesel::upsert::excluded;
use std::collections::BTreeMap;

/// Add `requests` to `day` and return the day's total afterwards.
pub async fn add_requests(day: NaiveDate, requests: u32) -> Result<u64> {
    let delta = i64::from(requests);
    let conn = connection_pool::get().await?;
    let total = conn
        .interact(move |conn| {
            diesel::insert_into(ref_flow_ledger::table)
                .values((
                    ref_flow_ledger::day.eq(day),
                    ref_flow_ledger::requests.eq(delta),
                ))
                .on_conflict(ref_flow_ledger::day)
                .do_update()
                .set(
                    ref_flow_ledger::requests
                        .eq(ref_flow_ledger::requests + excluded(ref_flow_ledger::requests)),
                )
                .returning(ref_flow_ledger::requests)
                .get_result::<i64>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    u64::try_from(total).context("negative ledger total")
}

/// Requests charged on `day` (0 when the day has no row).
pub async fn requests_on(day: NaiveDate) -> Result<u64> {
    let conn = connection_pool::get().await?;
    let total = conn
        .interact(move |conn| {
            ref_flow_ledger::table
                .filter(ref_flow_ledger::day.eq(day))
                .select(ref_flow_ledger::requests)
                .first::<i64>(conn)
                .optional()
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    total
        .map(|t| u64::try_from(t).context("negative ledger total"))
        .transpose()
        .map(|t| t.unwrap_or(0))
}

/// Requests per day for `from <= day <= to`, days without a row omitted.
pub async fn window(from: NaiveDate, to: NaiveDate) -> Result<BTreeMap<NaiveDate, u64>> {
    let conn = connection_pool::get().await?;
    let rows = conn
        .interact(move |conn| {
            ref_flow_ledger::table
                .filter(ref_flow_ledger::day.ge(from))
                .filter(ref_flow_ledger::day.le(to))
                .order(ref_flow_ledger::day.asc())
                .select((ref_flow_ledger::day, ref_flow_ledger::requests))
                .load::<(NaiveDate, i64)>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    rows.into_iter()
        .map(|(day, n)| Ok((day, u64::try_from(n).context("negative ledger total")?)))
        .collect()
}

#[cfg(test)]
mod tests;
