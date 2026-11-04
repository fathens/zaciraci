//! `ref_flow_runs`: one row per pass of a collection cycle. Feeds the
//! budget's forward-demand estimate and the operational watchdog.

use crate::Result;
use crate::connection_pool;
use crate::schema::ref_flow_runs;
use anyhow::{Context, anyhow};
use chrono::{NaiveDate, NaiveDateTime};
use diesel::prelude::*;
use diesel::sql_types::{BigInt, Date, Text};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

/// Which pass of the cycle a run row describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RunStream {
    /// Forward listing from the saved cursor.
    Fwd,
    /// Gap catch-up listing.
    Gap,
    /// Transaction enrichment.
    Enrich,
}

impl RunStream {
    pub const ALL: [RunStream; 3] = [RunStream::Fwd, RunStream::Gap, RunStream::Enrich];

    // SYNC: CHECK (stream IN ('fwd','gap','enrich')) on ref_flow_runs
    pub fn as_str(self) -> &'static str {
        match self {
            RunStream::Fwd => "fwd",
            RunStream::Gap => "gap",
            RunStream::Enrich => "enrich",
        }
    }
}

impl fmt::Display for RunStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unknown run stream: {0:?}")]
pub struct ParseRunStreamError(String);

impl FromStr for RunStream {
    type Err = ParseRunStreamError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        // SYNC: CHECK (stream IN ('fwd','gap','enrich')) on ref_flow_runs
        match s {
            "fwd" => Ok(RunStream::Fwd),
            "gap" => Ok(RunStream::Gap),
            "enrich" => Ok(RunStream::Enrich),
            other => Err(ParseRunStreamError(other.to_owned())),
        }
    }
}

/// One pass of a cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRecord {
    pub run_at: NaiveDateTime,
    pub stream: RunStream,
    pub pages: u32,
    pub receipts: u32,
    pub trades: u32,
    /// Oldest / newest receipt time touched by the pass.
    pub from_time: Option<NaiveDateTime>,
    pub to_time: Option<NaiveDateTime>,
    /// Requests charged to the ledger by this pass.
    pub credits: u32,
    pub http429: u32,
    pub failures: u32,
    pub note: String,
    /// Parser statistics (reject counts etc.).
    pub parse_stats: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = ref_flow_runs)]
#[diesel(check_for_backend(diesel::pg::Pg))]
struct DbRun {
    run_at: NaiveDateTime,
    stream: String,
    pages: i32,
    receipts: i32,
    trades: i32,
    from_time: Option<NaiveDateTime>,
    to_time: Option<NaiveDateTime>,
    credits: i32,
    http429: i32,
    failures: i32,
    note: String,
    parse_stats: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = ref_flow_runs)]
struct NewDbRun {
    run_at: NaiveDateTime,
    stream: String,
    pages: i32,
    receipts: i32,
    trades: i32,
    from_time: Option<NaiveDateTime>,
    to_time: Option<NaiveDateTime>,
    credits: i32,
    http429: i32,
    failures: i32,
    note: String,
    parse_stats: Option<serde_json::Value>,
}

fn to_i32(value: u32, what: &str) -> Result<i32> {
    i32::try_from(value).with_context(|| format!("{what} {value} exceeds i32"))
}

fn to_u32(value: i32, what: &str) -> Result<u32> {
    u32::try_from(value).with_context(|| format!("negative {what} {value}"))
}

impl RunRecord {
    fn to_new_db(&self) -> Result<NewDbRun> {
        Ok(NewDbRun {
            run_at: self.run_at,
            stream: self.stream.as_str().to_owned(),
            pages: to_i32(self.pages, "pages")?,
            receipts: to_i32(self.receipts, "receipts")?,
            trades: to_i32(self.trades, "trades")?,
            from_time: self.from_time,
            to_time: self.to_time,
            credits: to_i32(self.credits, "credits")?,
            http429: to_i32(self.http429, "http429")?,
            failures: to_i32(self.failures, "failures")?,
            note: self.note.clone(),
            parse_stats: self.parse_stats.clone(),
        })
    }

    fn from_db(row: DbRun) -> Result<Self> {
        Ok(Self {
            run_at: row.run_at,
            stream: RunStream::from_str(&row.stream)?,
            pages: to_u32(row.pages, "pages")?,
            receipts: to_u32(row.receipts, "receipts")?,
            trades: to_u32(row.trades, "trades")?,
            from_time: row.from_time,
            to_time: row.to_time,
            credits: to_u32(row.credits, "credits")?,
            http429: to_u32(row.http429, "http429")?,
            failures: to_u32(row.failures, "failures")?,
            note: row.note,
            parse_stats: row.parse_stats,
        })
    }
}

/// Append one run row.
pub async fn insert(record: &RunRecord) -> Result<()> {
    let row = record.to_new_db()?;
    let conn = connection_pool::get().await?;
    conn.interact(move |conn| {
        diesel::insert_into(ref_flow_runs::table)
            .values(&row)
            .execute(conn)
    })
    .await
    .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    Ok(())
}

/// Most recent run, optionally restricted to one stream.
pub async fn latest(stream: Option<RunStream>) -> Result<Option<RunRecord>> {
    let stream = stream.map(|s| s.as_str().to_owned());
    let conn = connection_pool::get().await?;
    let row = conn
        .interact(move |conn| {
            let mut query = ref_flow_runs::table.into_boxed();
            if let Some(stream) = stream {
                query = query.filter(ref_flow_runs::stream.eq(stream));
            }
            query
                .order((ref_flow_runs::run_at.desc(), ref_flow_runs::id.desc()))
                .select(DbRun::as_select())
                .first::<DbRun>(conn)
                .optional()
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    row.map(RunRecord::from_db).transpose()
}

/// Credits per (UTC day, stream) for runs at or after `since`.
#[derive(Debug, Clone, PartialEq, Eq, QueryableByName)]
pub struct DailyCredits {
    #[diesel(sql_type = Date)]
    pub day: NaiveDate,
    #[diesel(sql_type = Text)]
    stream: String,
    #[diesel(sql_type = BigInt)]
    pub credits: i64,
}

impl DailyCredits {
    pub fn new(day: NaiveDate, stream: RunStream, credits: i64) -> Self {
        Self {
            day,
            stream: stream.as_str().to_owned(),
            credits,
        }
    }

    pub fn stream(&self) -> Result<RunStream> {
        Ok(RunStream::from_str(&self.stream)?)
    }
}

/// Daily credits per stream since `since`, ordered by day then stream.
pub async fn credits_by_day(since: NaiveDateTime) -> Result<Vec<DailyCredits>> {
    use diesel::sql_types::Timestamp;
    let conn = connection_pool::get().await?;
    let rows = conn
        .interact(move |conn| {
            diesel::sql_query(
                "SELECT run_at::date AS day, stream, sum(credits)::bigint AS credits \
                 FROM ref_flow_runs WHERE run_at >= $1 \
                 GROUP BY 1, 2 ORDER BY 1, 2",
            )
            .bind::<Timestamp, _>(since)
            .load::<DailyCredits>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    Ok(rows)
}

/// Delete run rows older than `cutoff`.
pub async fn delete_before(cutoff: NaiveDateTime) -> Result<usize> {
    let conn = connection_pool::get().await?;
    let n = conn
        .interact(move |conn| {
            diesel::delete(ref_flow_runs::table.filter(ref_flow_runs::run_at.lt(cutoff)))
                .execute(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    Ok(n)
}

#[cfg(test)]
mod tests;
