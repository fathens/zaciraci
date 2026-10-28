//! `ref_flow_txs`: the per-transaction frame and enrichment marker.
//!
//! Rows are created by `ref_flow::receipt::ingest_receipts` (one per
//! successful transaction). This module drives the enrichment lifecycle:
//! pick pending transactions, record the trades fetched for one of them
//! (replacing any previous result in the same transaction), count failed
//! attempts and isolate poison transactions, reset a time range for
//! re-enrichment, and report per-bucket coverage.

use crate::Result;
use crate::connection_pool;
use crate::ref_flow::trade::{self, NewDbRefTrade};
use crate::schema::{ref_flow_trades, ref_flow_txs};
use anyhow::{Context, anyhow};
use chrono::NaiveDateTime;
use common::types::{TimeRange, TxHash};
use dex::Trade;
use diesel::prelude::*;
use diesel::sql_types::{BigInt, Int4, Timestamp};
use logging::*;

/// One frame row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxRecord {
    pub tx_hash: TxHash,
    pub block_time: NaiveDateTime,
    pub enriched_at: Option<NaiveDateTime>,
    pub attempts: u16,
    pub trade_count: Option<u16>,
}

impl TxRecord {
    pub fn is_enriched(&self) -> bool {
        self.enriched_at.is_some()
    }
}

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = ref_flow_txs)]
#[diesel(check_for_backend(diesel::pg::Pg))]
struct DbRefTx {
    tx_hash: String,
    block_time: NaiveDateTime,
    enriched_at: Option<NaiveDateTime>,
    attempts: i16,
    trade_count: Option<i16>,
}

impl TxRecord {
    fn from_db(row: DbRefTx) -> Result<Self> {
        let tx_hash = TxHash::new(&row.tx_hash)
            .with_context(|| format!("ref_flow_txs.tx_hash {:?}", row.tx_hash))?;
        let context = format!("ref_flow_txs row {tx_hash}");
        let attempts = u16::try_from(row.attempts).with_context(|| context.clone())?;
        let trade_count = row
            .trade_count
            .map(u16::try_from)
            .transpose()
            .with_context(|| context)?;
        Ok(Self {
            tx_hash,
            block_time: row.block_time,
            enriched_at: row.enriched_at,
            attempts,
            trade_count,
        })
    }
}

/// Look up one frame row.
pub async fn get(tx_hash: &TxHash) -> Result<Option<TxRecord>> {
    let key = tx_hash.as_str().to_owned();
    let conn = connection_pool::get().await?;
    let row = conn
        .interact(move |conn| {
            ref_flow_txs::table
                .filter(ref_flow_txs::tx_hash.eq(key))
                .select(DbRefTx::as_select())
                .first::<DbRefTx>(conn)
                .optional()
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    row.map(TxRecord::from_db).transpose()
}

/// A transaction waiting for enrichment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingTx {
    pub tx_hash: TxHash,
    pub block_time: NaiveDateTime,
}

/// Up to `limit` un-enriched transactions with `block_time >= not_before`,
/// newest first. `not_before` should sit a little after the retention
/// cutoff so the daily DELETE and the enrichment never race.
pub async fn pending_candidates(not_before: NaiveDateTime, limit: i64) -> Result<Vec<PendingTx>> {
    let conn = connection_pool::get().await?;
    let rows = conn
        .interact(move |conn| {
            ref_flow_txs::table
                .filter(ref_flow_txs::enriched_at.is_null())
                .filter(ref_flow_txs::block_time.ge(not_before))
                .order(ref_flow_txs::block_time.desc())
                .limit(limit)
                .select((ref_flow_txs::tx_hash, ref_flow_txs::block_time))
                .load::<(String, NaiveDateTime)>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    rows.into_iter()
        .map(|(hash, block_time)| {
            Ok(PendingTx {
                tx_hash: TxHash::new(&hash)
                    .with_context(|| format!("ref_flow_txs.tx_hash {hash:?}"))?,
                block_time,
            })
        })
        .collect()
}

/// Whether [`record_enrichment`] may overwrite a transaction that is
/// already enriched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplacePolicy {
    /// Replace unconditionally (collector re-fetch, parser upgrade).
    Always,
    /// Write only if the transaction is still pending (importer: rows
    /// written by the live collector win).
    OnlyIfPending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrichmentOutcome {
    /// Trades written and the frame row marked enriched.
    Recorded { trades: usize },
    /// The transaction was already enriched and the policy forbids
    /// replacing it. Nothing was written.
    Skipped,
    /// No frame row exists for the transaction. Nothing was written.
    NotInFrame,
}

diesel::define_sql_function! {
    fn coalesce(a: diesel::sql_types::Nullable<Timestamp>, b: Timestamp) -> Timestamp;
}

/// Store the trades fetched for one transaction and mark it enriched, in a
/// single DB transaction: previous trades of the transaction are deleted
/// first, `enriched_at` is set only the first time, and `trade_count`
/// always reflects this write. An empty `trades` slice is valid and marks
/// the transaction enriched with zero trades.
pub async fn record_enrichment(
    tx_hash: &TxHash,
    trades: &[Trade],
    policy: ReplacePolicy,
    now: NaiveDateTime,
) -> Result<EnrichmentOutcome> {
    if let Some(stray) = trades.iter().find(|t| &t.tx_hash != tx_hash) {
        return Err(anyhow!(
            "trade {} belongs to transaction {}, not {}",
            stray.receipt_id,
            stray.tx_hash,
            tx_hash
        ));
    }
    let rows = trades
        .iter()
        .map(trade::to_new_db)
        .collect::<Result<Vec<NewDbRefTrade>>>()?;
    let trade_count = i16::try_from(rows.len())
        .with_context(|| format!("{} trades exceed the i16 trade_count", rows.len()))?;
    let key = tx_hash.as_str().to_owned();
    let conn = connection_pool::get().await?;
    let outcome = conn
        .interact(move |conn| {
            conn.transaction::<_, diesel::result::Error, _>(|conn| {
                let target = ref_flow_txs::table.filter(ref_flow_txs::tx_hash.eq(&key));
                let update = diesel::update(target).set((
                    ref_flow_txs::enriched_at
                        .eq(coalesce(ref_flow_txs::enriched_at, now).nullable()),
                    ref_flow_txs::trade_count.eq(Some(trade_count)),
                ));
                let updated = match policy {
                    ReplacePolicy::Always => update.execute(conn)?,
                    ReplacePolicy::OnlyIfPending => update
                        .filter(ref_flow_txs::enriched_at.is_null())
                        .execute(conn)?,
                };
                if updated == 0 {
                    let exists =
                        diesel::select(diesel::dsl::exists(target)).get_result::<bool>(conn)?;
                    return Ok(if exists {
                        EnrichmentOutcome::Skipped
                    } else {
                        EnrichmentOutcome::NotInFrame
                    });
                }
                trade::delete_by_tx_hash(&key, conn)?;
                let inserted = trade::insert_chunked_with(&rows, NewDbRefTrade::CHUNK_ROWS, conn)?;
                Ok(EnrichmentOutcome::Recorded { trades: inserted })
            })
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    Ok(outcome)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FailureOutcome {
    /// Attempts recorded so far, including this one.
    pub attempts: u16,
    /// The transaction reached `max_attempts` and was marked enriched with
    /// zero trades so it stops consuming API budget.
    pub isolated: bool,
}

/// Count one failed enrichment attempt. Once `attempts` reaches
/// `max_attempts` the transaction is isolated (see [`FailureOutcome`]).
/// Returns `None` when there is no frame row.
pub async fn mark_failed_attempt(
    tx_hash: &TxHash,
    max_attempts: u16,
    now: NaiveDateTime,
) -> Result<Option<FailureOutcome>> {
    let key = tx_hash.as_str().to_owned();
    let conn = connection_pool::get().await?;
    let outcome = conn
        .interact(move |conn| {
            conn.transaction::<_, diesel::result::Error, _>(|conn| {
                let target = ref_flow_txs::table.filter(ref_flow_txs::tx_hash.eq(&key));
                let attempts = diesel::update(target)
                    .set(ref_flow_txs::attempts.eq(ref_flow_txs::attempts + 1))
                    .returning(ref_flow_txs::attempts)
                    .get_result::<i16>(conn)
                    .optional()?;
                let Some(attempts) = attempts else {
                    return Ok(None);
                };
                let isolated = attempts >= i16::try_from(max_attempts).unwrap_or(i16::MAX);
                if isolated {
                    diesel::update(target.filter(ref_flow_txs::enriched_at.is_null()))
                        .set((
                            ref_flow_txs::enriched_at.eq(Some(now)),
                            ref_flow_txs::trade_count.eq(Some(0i16)),
                        ))
                        .execute(conn)?;
                }
                Ok(Some((attempts, isolated)))
            })
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    outcome
        .map(|(attempts, isolated)| {
            Ok(FailureOutcome {
                attempts: u16::try_from(attempts).context("negative attempts")?,
                isolated,
            })
        })
        .transpose()
}

/// Forget the enrichment of every transaction with `start <= block_time <
/// end`: their trades are deleted and the frame rows return to pending
/// with zero attempts. Returns the number of frame rows reset.
pub async fn reset_enrichment(range: &TimeRange) -> Result<usize> {
    let log = DEFAULT.new(o!("function" => "ref_flow::reset_enrichment"));
    let (start, end) = (range.start, range.end);
    let conn = connection_pool::get().await?;
    let (trades_deleted, reset) = conn
        .interact(move |conn| {
            conn.transaction::<_, diesel::result::Error, _>(|conn| {
                let in_range = ref_flow_txs::table
                    .filter(ref_flow_txs::block_time.ge(start))
                    .filter(ref_flow_txs::block_time.lt(end));
                let trades_deleted = diesel::delete(ref_flow_trades::table.filter(
                    ref_flow_trades::tx_hash.eq_any(in_range.select(ref_flow_txs::tx_hash)),
                ))
                .execute(conn)?;
                let reset = diesel::update(in_range)
                    .set((
                        ref_flow_txs::enriched_at.eq(None::<NaiveDateTime>),
                        ref_flow_txs::trade_count.eq(None::<i16>),
                        ref_flow_txs::attempts.eq(0i16),
                    ))
                    .execute(conn)?;
                Ok((trades_deleted, reset))
            })
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    info!(log, "finish"; "txs_reset" => reset, "trades_deleted" => trades_deleted);
    Ok(reset)
}

/// Enrichment coverage of one time bucket.
#[derive(Debug, Clone, PartialEq, Eq, QueryableByName)]
pub struct BucketCoverage {
    #[diesel(sql_type = Timestamp)]
    pub bucket_start: NaiveDateTime,
    /// Successful transactions whose block_time falls in the bucket.
    #[diesel(sql_type = BigInt)]
    pub total: i64,
    /// Of those, how many are enriched.
    #[diesel(sql_type = BigInt)]
    pub enriched: i64,
}

/// Coverage per `bucket_minutes` bucket over `start <= block_time < end`,
/// ordered by bucket. Buckets without transactions are omitted.
pub async fn coverage_by_bucket(
    range: &TimeRange,
    bucket_minutes: u32,
) -> Result<Vec<BucketCoverage>> {
    let minutes = i32::try_from(bucket_minutes).context("bucket_minutes exceeds i32")?;
    if minutes <= 0 {
        return Err(anyhow!("bucket_minutes must be positive"));
    }
    let (start, end) = (range.start, range.end);
    let conn = connection_pool::get().await?;
    let rows = conn
        .interact(move |conn| {
            diesel::sql_query(
                "SELECT date_bin(make_interval(mins => $1), block_time, TIMESTAMP '2000-01-01') \
                        AS bucket_start, \
                        count(*) AS total, \
                        count(enriched_at) AS enriched \
                 FROM ref_flow_txs \
                 WHERE block_time >= $2 AND block_time < $3 \
                 GROUP BY 1 ORDER BY 1",
            )
            .bind::<Int4, _>(minutes)
            .bind::<Timestamp, _>(start)
            .bind::<Timestamp, _>(end)
            .load::<BucketCoverage>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    Ok(rows)
}

#[cfg(test)]
mod tests;
