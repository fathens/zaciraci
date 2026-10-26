use super::*;
use crate::ref_flow::receipt::{RefReceipt, RefReceiptParts, ingest_receipts};

pub fn tok(name: &str) -> TokenAccount {
    name.parse().unwrap()
}

pub fn hop(pool_id: u32, token_in: &str, token_out: &str) -> Hop {
    Hop {
        pool_id,
        token_in: tok(token_in).to_in(),
        token_out: tok(token_out).to_out(),
        amount_in: units(1_000_000_000_000_000_000_000_000 + u128::from(pool_id)),
        amount_out: units(2_000_000 + u128::from(pool_id)),
    }
}

pub fn units(v: u128) -> TokenSmallestUnits {
    TokenSmallestUnits::from_u128(v)
}

/// Trade for receipt `n` of transaction `tx` at `ts(secs)`.
pub fn make_trade(n: u32, tx: u32, secs: i64, hops: Vec<Hop>) -> Trade {
    Trade::new(TradeParts {
        receipt_id: receipt_id(n),
        tx_hash: tx_hash(tx),
        block_height: 221_000_000 + u64::from(n),
        block_time: ts(secs),
        sender: format!("trader{n}.near").parse().unwrap(),
        route: if n.is_multiple_of(2) {
            TradeRoute::Ft
        } else {
            TradeRoute::Swap
        },
        hops,
        fees: None,
        amount_in: units(1_000_000_000_000_000_000_000_000 + u128::from(n)),
        amount_out: units(2_000_000 + u128::from(n)),
    })
    .unwrap()
}

pub fn chain_hops() -> Vec<Hop> {
    vec![
        hop(6458, "wrap.near", "token.rhealab.near"),
        hop(5516, "token.rhealab.near", "usdt.tether-token.near"),
    ]
}

/// Create the frame row of transaction `tx` (via a successful receipt).
pub async fn seed_frame(tx: u32, secs: i64) -> Result<()> {
    let receipt = RefReceipt::new(RefReceiptParts {
        receipt_id: receipt_id(500_000 + tx),
        tx_hash: tx_hash(tx),
        block_height: 1,
        block_time: ts(secs),
        predecessor: "seed.near".parse().unwrap(),
        methods: vec!["swap".to_owned()],
        ok: true,
    })
    .unwrap();
    ingest_receipts(&[receipt]).await?;
    Ok(())
}

/// Insert prepared rows through the crate-internal chunked insert.
pub async fn insert_rows(rows: Vec<NewDbRefTrade>) -> Result<usize> {
    let conn = connection_pool::get_test_only().await?;
    let n = conn
        .interact(move |conn| {
            conn.transaction(|conn| insert_chunked_with(&rows, NewDbRefTrade::CHUNK_ROWS, conn))
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    Ok(n)
}

/// Insert trades after seeding their frame rows.
pub async fn insert_trades(trades: &[Trade]) -> Result<usize> {
    let rows = trades.iter().map(to_new_db).collect::<Result<Vec<_>>>()?;
    insert_rows(rows).await
}

/// Build a readable row from an insert row (what the DB would hand back).
pub(super) fn as_db_row(row: &NewDbRefTrade) -> DbRefTrade {
    DbRefTrade {
        receipt_id: row.receipt_id.clone(),
        tx_hash: row.tx_hash.clone(),
        block_height: row.block_height,
        block_time: row.block_time,
        sender: row.sender.clone(),
        route: row.route.clone(),
        pool_ids: row.pool_ids.clone(),
        hop_tokens_in: row.hop_tokens_in.clone(),
        hop_tokens_out: row.hop_tokens_out.clone(),
        hop_amounts_in: row.hop_amounts_in.clone(),
        hop_amounts_out: row.hop_amounts_out.clone(),
        fee_bps: row.fee_bps.clone(),
        token_in: row.token_in.clone(),
        amount_in: row.amount_in.clone(),
        token_out: row.token_out.clone(),
        amount_out: row.amount_out.clone(),
    }
}

/// Mutable access for building invalid rows in CHECK tests.
pub(super) fn tweak(row: &mut NewDbRefTrade) -> RowFields<'_> {
    RowFields {
        pool_ids: &mut row.pool_ids,
        hop_tokens_in: &mut row.hop_tokens_in,
        hop_tokens_out: &mut row.hop_tokens_out,
        hop_amounts_in: &mut row.hop_amounts_in,
        hop_amounts_out: &mut row.hop_amounts_out,
        fee_bps: &mut row.fee_bps,
        token_in: &mut row.token_in,
        token_out: &mut row.token_out,
        amount_in: &mut row.amount_in,
        amount_out: &mut row.amount_out,
        route: &mut row.route,
        tx_hash: &mut row.tx_hash,
    }
}

pub(super) struct RowFields<'a> {
    pub pool_ids: &'a mut Vec<Option<i32>>,
    pub hop_tokens_in: &'a mut Vec<Option<String>>,
    pub hop_tokens_out: &'a mut Vec<Option<String>>,
    pub hop_amounts_in: &'a mut Vec<Option<BigDecimal>>,
    pub hop_amounts_out: &'a mut Vec<Option<BigDecimal>>,
    pub fee_bps: &'a mut Option<Vec<Option<i32>>>,
    pub token_in: &'a mut String,
    pub token_out: &'a mut String,
    pub amount_in: &'a mut BigDecimal,
    pub amount_out: &'a mut BigDecimal,
    pub route: &'a mut String,
    pub tx_hash: &'a mut String,
}
