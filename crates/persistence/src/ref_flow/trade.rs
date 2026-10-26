//! `ref_flow_trades`: executed trades, one row per Ref receipt.
//!
//! Rows are written only through `ref_flow::tx::record_enrichment`, which
//! owns the transaction and keeps the frame row in sync; this module holds
//! the Diesel models, the domain conversions, the read queries and the
//! pool-fee lookup.

use crate::Result;
use crate::batch;
use crate::connection_pool;
use crate::schema::{pool_info, ref_flow_trades};
use anyhow::{Context, anyhow};
use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use common::types::{AccountId, ReceiptId, TimeRange, TokenAccount, TokenSmallestUnits, TxHash};
use dex::ref_trade::TradeParts;
use dex::{Hop, Trade, TradeRoute};
use diesel::prelude::*;
use std::num::NonZeroUsize;
use std::str::FromStr;

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = ref_flow_trades)]
#[diesel(check_for_backend(diesel::pg::Pg))]
struct DbRefTrade {
    receipt_id: String,
    tx_hash: String,
    block_height: i64,
    block_time: NaiveDateTime,
    sender: String,
    route: String,
    pool_ids: Vec<Option<i32>>,
    hop_tokens_in: Vec<Option<String>>,
    hop_tokens_out: Vec<Option<String>>,
    hop_amounts_in: Vec<Option<BigDecimal>>,
    hop_amounts_out: Vec<Option<BigDecimal>>,
    fee_bps: Option<Vec<Option<i32>>>,
    token_in: String,
    amount_in: BigDecimal,
    token_out: String,
    amount_out: BigDecimal,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = ref_flow_trades)]
pub(crate) struct NewDbRefTrade {
    receipt_id: String,
    tx_hash: String,
    block_height: i64,
    block_time: NaiveDateTime,
    sender: String,
    route: String,
    pool_ids: Vec<Option<i32>>,
    hop_tokens_in: Vec<Option<String>>,
    hop_tokens_out: Vec<Option<String>>,
    hop_amounts_in: Vec<Option<BigDecimal>>,
    hop_amounts_out: Vec<Option<BigDecimal>>,
    fee_bps: Option<Vec<Option<i32>>>,
    token_in: String,
    amount_in: BigDecimal,
    token_out: String,
    amount_out: BigDecimal,
}

impl NewDbRefTrade {
    /// Bind parameters per row; see `crate::batch`.
    pub(crate) const COLS: NonZeroUsize = NonZeroUsize::new(16).expect("COLS must be non-zero");
    pub(crate) const CHUNK_ROWS: NonZeroUsize = batch::chunk_rows(Self::COLS);
}

/// Domain → insert row. Infallible except for `pool_id > i32::MAX`,
/// which the Ref contract never produces.
pub(crate) fn to_new_db(trade: &Trade) -> Result<NewDbRefTrade> {
    let pool_ids = trade
        .hops()
        .iter()
        .map(|h| {
            i32::try_from(h.pool_id)
                .map(Some)
                .with_context(|| format!("pool_id {} exceeds i32", h.pool_id))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(NewDbRefTrade {
        receipt_id: trade.receipt_id.as_str().to_owned(),
        tx_hash: trade.tx_hash.as_str().to_owned(),
        block_height: i64::try_from(trade.block_height)
            .with_context(|| format!("block_height {} exceeds i64", trade.block_height))?,
        block_time: trade.block_time,
        sender: trade.sender.to_string(),
        route: trade.route.as_str().to_owned(),
        pool_ids,
        hop_tokens_in: trade
            .hops()
            .iter()
            .map(|h| Some(h.token_in.to_string()))
            .collect(),
        hop_tokens_out: trade
            .hops()
            .iter()
            .map(|h| Some(h.token_out.to_string()))
            .collect(),
        hop_amounts_in: trade
            .hops()
            .iter()
            .map(|h| Some(h.amount_in.as_bigdecimal().clone()))
            .collect(),
        hop_amounts_out: trade
            .hops()
            .iter()
            .map(|h| Some(h.amount_out.as_bigdecimal().clone()))
            .collect(),
        fee_bps: trade
            .fees()
            .map(|fees| {
                fees.iter()
                    .map(|&f| {
                        i32::try_from(f)
                            .map(Some)
                            .with_context(|| format!("fee {f} exceeds i32"))
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?,
        token_in: trade.token_in().to_string(),
        amount_in: trade.amount_in().as_bigdecimal().clone(),
        token_out: trade.token_out().to_string(),
        amount_out: trade.amount_out().as_bigdecimal().clone(),
    })
}

fn non_null<T>(values: Vec<Option<T>>, column: &str) -> Result<Vec<T>> {
    values
        .into_iter()
        .map(|v| v.ok_or_else(|| anyhow!("NULL element in {column}")))
        .collect()
}

fn from_db(row: DbRefTrade) -> Result<Trade> {
    let receipt_id = ReceiptId::new(&row.receipt_id)
        .with_context(|| format!("ref_flow_trades.receipt_id {:?}", row.receipt_id))?;
    let context = format!("ref_flow_trades row {receipt_id}");
    let parsed = (|| -> Result<Trade> {
        let pool_ids = non_null(row.pool_ids, "pool_ids")?;
        let tokens_in = non_null(row.hop_tokens_in, "hop_tokens_in")?;
        let tokens_out = non_null(row.hop_tokens_out, "hop_tokens_out")?;
        let amounts_in = non_null(row.hop_amounts_in, "hop_amounts_in")?;
        let amounts_out = non_null(row.hop_amounts_out, "hop_amounts_out")?;
        let n = pool_ids.len();
        if [
            tokens_in.len(),
            tokens_out.len(),
            amounts_in.len(),
            amounts_out.len(),
        ]
        .iter()
        .any(|&len| len != n)
        {
            return Err(anyhow!(
                "hop array lengths differ: pool_ids={n} hop_tokens_in={} hop_tokens_out={} \
                 hop_amounts_in={} hop_amounts_out={}",
                tokens_in.len(),
                tokens_out.len(),
                amounts_in.len(),
                amounts_out.len()
            ));
        }
        let hops = pool_ids
            .into_iter()
            .zip(tokens_in)
            .zip(tokens_out)
            .zip(amounts_in)
            .zip(amounts_out)
            .map(
                |((((pool_id, token_in), token_out), amount_in), amount_out)| {
                    Ok(Hop {
                        pool_id: u32::try_from(pool_id)
                            .with_context(|| format!("negative pool_id {pool_id}"))?,
                        token_in: TokenAccount::from_str(&token_in)?.to_in(),
                        token_out: TokenAccount::from_str(&token_out)?.to_out(),
                        amount_in: TokenSmallestUnits::from_bigdecimal(amount_in),
                        amount_out: TokenSmallestUnits::from_bigdecimal(amount_out),
                    })
                },
            )
            .collect::<Result<Vec<Hop>>>()?;
        let fees = row
            .fee_bps
            .map(|fees| {
                non_null(fees, "fee_bps")?
                    .into_iter()
                    .map(|f| u32::try_from(f).with_context(|| format!("negative fee {f}")))
                    .collect::<Result<Vec<u32>>>()
            })
            .transpose()?;
        let trade = Trade::new(TradeParts {
            receipt_id: receipt_id.clone(),
            tx_hash: TxHash::new(&row.tx_hash)?,
            block_height: u64::try_from(row.block_height)?,
            block_time: row.block_time,
            sender: AccountId::from_str(&row.sender)?,
            route: TradeRoute::from_str(&row.route)?,
            hops,
            fees: None,
            amount_in: TokenSmallestUnits::from_bigdecimal(row.amount_in),
            amount_out: TokenSmallestUnits::from_bigdecimal(row.amount_out),
        })?;
        let trade = match fees {
            Some(fees) => trade.with_fees(fees)?,
            None => trade,
        };
        // The redundant endpoint columns are pinned by CHECKs; verify the
        // row we read still agrees with the hops.
        if trade.token_in().to_string() != row.token_in
            || trade.token_out().to_string() != row.token_out
        {
            return Err(anyhow!("token_in/token_out columns disagree with hops"));
        }
        Ok(trade)
    })();
    parsed.with_context(|| context)
}

/// Chunked insert inside a transaction the caller already opened.
pub(crate) fn insert_chunked_with(
    rows: &[NewDbRefTrade],
    chunk_rows: NonZeroUsize,
    conn: &mut PgConnection,
) -> QueryResult<usize> {
    let mut inserted = 0;
    for chunk in rows.chunks(chunk_rows.get()) {
        inserted += diesel::insert_into(ref_flow_trades::table)
            .values(chunk)
            .execute(conn)?;
    }
    Ok(inserted)
}

/// Delete every trade of one transaction. Caller owns the transaction.
pub(crate) fn delete_by_tx_hash(tx_hash: &str, conn: &mut PgConnection) -> QueryResult<usize> {
    diesel::delete(ref_flow_trades::table.filter(ref_flow_trades::tx_hash.eq(tx_hash)))
        .execute(conn)
}

/// Trades of one transaction, ordered by block time then receipt id.
pub async fn get_by_tx_hash(tx_hash: &TxHash) -> Result<Vec<Trade>> {
    let key = tx_hash.as_str().to_owned();
    let conn = connection_pool::get().await?;
    let rows = conn
        .interact(move |conn| {
            ref_flow_trades::table
                .filter(ref_flow_trades::tx_hash.eq(key))
                .order((
                    ref_flow_trades::block_time.asc(),
                    ref_flow_trades::receipt_id.asc(),
                ))
                .select(DbRefTrade::as_select())
                .load::<DbRefTrade>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    rows.into_iter().map(from_db).collect()
}

/// Trades with `start <= block_time < end`, ordered by time then receipt id.
pub async fn get_in_time_range(range: &TimeRange) -> Result<Vec<Trade>> {
    let (start, end) = (range.start, range.end);
    let conn = connection_pool::get().await?;
    let rows = conn
        .interact(move |conn| {
            ref_flow_trades::table
                .filter(ref_flow_trades::block_time.ge(start))
                .filter(ref_flow_trades::block_time.lt(end))
                .order((
                    ref_flow_trades::block_time.asc(),
                    ref_flow_trades::receipt_id.asc(),
                ))
                .select(DbRefTrade::as_select())
                .load::<DbRefTrade>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    rows.into_iter().map(from_db).collect()
}

/// Pool fee (`PoolInfo.total_fee`) of every hop, taken from the latest
/// `pool_info` snapshot at or before `block_time`.
///
/// Returns `None` when any hop's pool has no snapshot at or before that
/// time: fees are stored all-or-nothing because the multi-hop fee
/// correction needs every hop. The caller attaches the result with
/// [`Trade::with_fees`].
pub async fn lookup_hop_fees(hops: &[Hop], block_time: NaiveDateTime) -> Result<Option<Vec<u32>>> {
    let pool_ids = hops
        .iter()
        .map(|h| {
            i32::try_from(h.pool_id).with_context(|| format!("pool_id {} exceeds i32", h.pool_id))
        })
        .collect::<Result<Vec<i32>>>()?;
    let conn = connection_pool::get().await?;
    let fees = conn
        .interact(move |conn| -> QueryResult<Option<Vec<i32>>> {
            let mut fees = Vec::with_capacity(pool_ids.len());
            for pool_id in pool_ids {
                let fee = pool_info::table
                    .filter(pool_info::pool_id.eq(pool_id))
                    .filter(pool_info::timestamp.le(block_time))
                    .order(pool_info::timestamp.desc())
                    .select(pool_info::total_fee)
                    .first::<i32>(conn)
                    .optional()?;
                match fee {
                    Some(fee) => fees.push(fee),
                    None => return Ok(None),
                }
            }
            Ok(Some(fees))
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    fees.map(|fees| {
        fees.into_iter()
            .map(|f| {
                u32::try_from(f).with_context(|| format!("negative total_fee {f} in pool_info"))
            })
            .collect::<Result<Vec<u32>>>()
    })
    .transpose()
}

#[cfg(test)]
pub(crate) mod tests;
