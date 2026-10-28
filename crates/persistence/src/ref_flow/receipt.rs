//! `ref_flow_receipts`: the append-only index of receipts listed for the
//! Ref contract, plus creation of the matching `ref_flow_txs` rows.

use crate::Result;
use crate::batch;
use crate::connection_pool;
use crate::schema::{ref_flow_receipts, ref_flow_txs};
use anyhow::{Context, anyhow};
use chrono::NaiveDateTime;
use common::types::{AccountId, ReceiptId, TimeRange, TxHash};
use diesel::prelude::*;
use diesel::upsert::excluded;
use logging::*;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::str::FromStr;
use thiserror::Error;

/// Upper bound on action methods per receipt. Mirrors the
/// `cardinality(methods) BETWEEN 1 AND 16` CHECK.
pub const MAX_METHODS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RefReceiptError {
    #[error("receipt has no action methods")]
    NoMethods,
    #[error("receipt has {0} action methods, more than the {MAX_METHODS} allowed")]
    TooManyMethods(usize),
    #[error("receipt has an empty action method name")]
    EmptyMethod,
}

/// One receipt as listed by `/v3/accounts/v2.ref-finance.near/receipts`.
/// Construct through [`RefReceipt::new`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefReceipt {
    pub receipt_id: ReceiptId,
    pub tx_hash: TxHash,
    pub block_height: u64,
    pub block_time: NaiveDateTime,
    pub predecessor: AccountId,
    methods: Vec<String>,
    /// Outcome status of the receipt.
    pub ok: bool,
}

/// Constructor input for [`RefReceipt::new`].
#[derive(Debug, Clone)]
pub struct RefReceiptParts {
    pub receipt_id: ReceiptId,
    pub tx_hash: TxHash,
    pub block_height: u64,
    pub block_time: NaiveDateTime,
    pub predecessor: AccountId,
    /// Action method names in order, e.g. `["swap"]` or
    /// `["swap", "withdraw"]`.
    pub methods: Vec<String>,
    pub ok: bool,
}

impl RefReceipt {
    pub fn new(parts: RefReceiptParts) -> std::result::Result<Self, RefReceiptError> {
        if parts.methods.is_empty() {
            return Err(RefReceiptError::NoMethods);
        }
        if parts.methods.len() > MAX_METHODS {
            return Err(RefReceiptError::TooManyMethods(parts.methods.len()));
        }
        if parts.methods.iter().any(String::is_empty) {
            return Err(RefReceiptError::EmptyMethod);
        }
        Ok(Self {
            receipt_id: parts.receipt_id,
            tx_hash: parts.tx_hash,
            block_height: parts.block_height,
            block_time: parts.block_time,
            predecessor: parts.predecessor,
            methods: parts.methods,
            ok: parts.ok,
        })
    }

    /// Action method names in order (never empty).
    pub fn methods(&self) -> &[String] {
        &self.methods
    }

    fn from_db(row: DbRefReceipt) -> Result<Self> {
        let receipt_id = ReceiptId::new(&row.receipt_id)
            .with_context(|| format!("ref_flow_receipts.receipt_id {:?}", row.receipt_id))?;
        let context = format!("ref_flow_receipts row {}", receipt_id);
        let tx_hash = TxHash::new(&row.tx_hash).with_context(|| context.clone())?;
        let block_height = u64::try_from(row.block_height).with_context(|| context.clone())?;
        let predecessor = AccountId::from_str(&row.predecessor).with_context(|| context.clone())?;
        let methods = row
            .methods
            .into_iter()
            .map(|m| m.ok_or_else(|| anyhow!("NULL element in methods")))
            .collect::<Result<Vec<String>>>()
            .with_context(|| context.clone())?;
        Self::new(RefReceiptParts {
            receipt_id,
            tx_hash,
            block_height,
            block_time: row.block_time,
            predecessor,
            methods,
            ok: row.ok,
        })
        .with_context(|| context)
    }

    fn to_new_db(&self) -> Result<NewDbRefReceipt> {
        Ok(NewDbRefReceipt {
            receipt_id: self.receipt_id.as_str().to_owned(),
            tx_hash: self.tx_hash.as_str().to_owned(),
            block_height: i64::try_from(self.block_height)
                .with_context(|| format!("block_height {} exceeds i64", self.block_height))?,
            block_time: self.block_time,
            predecessor: self.predecessor.to_string(),
            methods: self.methods.iter().cloned().map(Some).collect(),
            ok: self.ok,
        })
    }
}

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = ref_flow_receipts)]
#[diesel(check_for_backend(diesel::pg::Pg))]
struct DbRefReceipt {
    receipt_id: String,
    tx_hash: String,
    block_height: i64,
    block_time: NaiveDateTime,
    predecessor: String,
    methods: Vec<Option<String>>,
    ok: bool,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = ref_flow_receipts)]
struct NewDbRefReceipt {
    receipt_id: String,
    tx_hash: String,
    block_height: i64,
    block_time: NaiveDateTime,
    predecessor: String,
    methods: Vec<Option<String>>,
    ok: bool,
}

impl NewDbRefReceipt {
    /// Bind parameters per row; see `crate::batch`.
    const COLS: NonZeroUsize = NonZeroUsize::new(7).expect("COLS must be non-zero");
    const CHUNK_ROWS: NonZeroUsize = batch::chunk_rows(Self::COLS);
}

/// Frame row for a successful transaction. `enriched_at`, `attempts` and
/// `trade_count` take their defaults.
#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = ref_flow_txs)]
struct NewDbRefTx {
    tx_hash: String,
    block_time: NaiveDateTime,
}

impl NewDbRefTx {
    const COLS: NonZeroUsize = NonZeroUsize::new(2).expect("COLS must be non-zero");
    const CHUNK_ROWS: NonZeroUsize = batch::chunk_rows(Self::COLS);
}

diesel::define_sql_function! {
    fn least(a: diesel::sql_types::Timestamp, b: diesel::sql_types::Timestamp) -> diesel::sql_types::Timestamp;
}

/// Outcome of [`ingest_receipts`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct IngestStats {
    /// Receipts newly inserted (duplicates are skipped).
    pub receipts_inserted: usize,
    /// Frame rows inserted or whose `block_time` was moved earlier.
    pub txs_upserted: usize,
}

/// Chunked `INSERT ... ON CONFLICT` of receipts and their frame rows in
/// one transaction. Test DI point for chunk sizes (see
/// `batch::chunk_rows_with_budget`).
fn ingest_chunked_with(
    receipts: Vec<NewDbRefReceipt>,
    txs: Vec<NewDbRefTx>,
    receipt_chunk_rows: NonZeroUsize,
    tx_chunk_rows: NonZeroUsize,
    conn: &mut PgConnection,
) -> QueryResult<IngestStats> {
    conn.transaction::<_, diesel::result::Error, _>(|conn| {
        let mut stats = IngestStats::default();
        for chunk in receipts.chunks(receipt_chunk_rows.get()) {
            stats.receipts_inserted += diesel::insert_into(ref_flow_receipts::table)
                .values(chunk)
                .on_conflict(ref_flow_receipts::receipt_id)
                .do_nothing()
                .execute(conn)?;
        }
        for chunk in txs.chunks(tx_chunk_rows.get()) {
            // Keep the earliest receipt time seen for the transaction, so a
            // gap fill that lists an older receipt moves the frame row's
            // bucket backwards instead of being ignored.
            stats.txs_upserted += diesel::insert_into(ref_flow_txs::table)
                .values(chunk)
                .on_conflict(ref_flow_txs::tx_hash)
                .do_update()
                .set(ref_flow_txs::block_time.eq(least(
                    ref_flow_txs::block_time,
                    excluded(ref_flow_txs::block_time),
                )))
                .execute(conn)?;
        }
        Ok(stats)
    })
}

/// Build the deduplicated DB rows for a batch: receipts keyed by
/// `receipt_id`, and one frame row per successful `tx_hash` carrying the
/// earliest `block_time` of its receipts in the batch.
fn prepare_rows(receipts: &[RefReceipt]) -> Result<(Vec<NewDbRefReceipt>, Vec<NewDbRefTx>)> {
    let mut seen = HashSet::with_capacity(receipts.len());
    let mut rows = Vec::with_capacity(receipts.len());
    let mut first_seen: HashMap<&TxHash, NaiveDateTime> = HashMap::new();
    for receipt in receipts {
        if !seen.insert(&receipt.receipt_id) {
            continue;
        }
        rows.push(receipt.to_new_db()?);
        if receipt.ok {
            first_seen
                .entry(&receipt.tx_hash)
                .and_modify(|t| *t = (*t).min(receipt.block_time))
                .or_insert(receipt.block_time);
        }
    }
    let mut txs: Vec<NewDbRefTx> = first_seen
        .into_iter()
        .map(|(tx_hash, block_time)| NewDbRefTx {
            tx_hash: tx_hash.as_str().to_owned(),
            block_time,
        })
        .collect();
    txs.sort_by(|a, b| a.tx_hash.cmp(&b.tx_hash));
    Ok((rows, txs))
}

/// Insert listed receipts and create the frame row of every successful
/// transaction, atomically. Re-ingesting the same receipts is a no-op.
pub async fn ingest_receipts(receipts: &[RefReceipt]) -> Result<IngestStats> {
    let log = DEFAULT.new(o!(
        "function" => "ref_flow::ingest_receipts",
        "receipts" => receipts.len(),
    ));
    if receipts.is_empty() {
        return Ok(IngestStats::default());
    }
    let (rows, txs) = prepare_rows(receipts)?;
    let receipt_chunk_rows = NewDbRefReceipt::CHUNK_ROWS;
    let tx_chunk_rows = NewDbRefTx::CHUNK_ROWS;
    if rows.len() > receipt_chunk_rows.get() {
        debug!(log, "batch chunked";
            "rows" => rows.len(),
            "chunk_rows" => receipt_chunk_rows.get(),
            "chunks" => rows.len().div_ceil(receipt_chunk_rows.get()),
        );
    }
    let conn = connection_pool::get().await?;
    let stats = conn
        .interact(move |conn| {
            ingest_chunked_with(rows, txs, receipt_chunk_rows, tx_chunk_rows, conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    debug!(log, "finish";
        "receipts_inserted" => stats.receipts_inserted,
        "txs_upserted" => stats.txs_upserted,
    );
    Ok(stats)
}

/// All receipts of one transaction, ordered by block height then id.
pub async fn get_by_tx_hash(tx_hash: &TxHash) -> Result<Vec<RefReceipt>> {
    let key = tx_hash.as_str().to_owned();
    let conn = connection_pool::get().await?;
    let rows = conn
        .interact(move |conn| {
            ref_flow_receipts::table
                .filter(ref_flow_receipts::tx_hash.eq(key))
                .order((
                    ref_flow_receipts::block_height.asc(),
                    ref_flow_receipts::receipt_id.asc(),
                ))
                .select(DbRefReceipt::as_select())
                .load::<DbRefReceipt>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    rows.into_iter().map(RefReceipt::from_db).collect()
}

/// Receipts with `start <= block_time < end`, ordered by time then id.
pub async fn get_in_time_range(range: &TimeRange) -> Result<Vec<RefReceipt>> {
    let (start, end) = (range.start, range.end);
    let conn = connection_pool::get().await?;
    let rows = conn
        .interact(move |conn| {
            ref_flow_receipts::table
                .filter(ref_flow_receipts::block_time.ge(start))
                .filter(ref_flow_receipts::block_time.lt(end))
                .order((
                    ref_flow_receipts::block_time.asc(),
                    ref_flow_receipts::receipt_id.asc(),
                ))
                .select(DbRefReceipt::as_select())
                .load::<DbRefReceipt>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    rows.into_iter().map(RefReceipt::from_db).collect()
}

#[cfg(test)]
mod tests;
