use super::*;
use std::num::NonZeroUsize;

/// Multi-chunk happy path: 5 rows with chunk_rows=2 spans three chunks
/// (2 + 2 + 1). Exercises the `.chunks(...)` loop in
/// `NewDbTokenRate::insert_chunked_with` past the off-by-one boundary and
/// verifies the production-side wiring (`TokenRate::batch_insert` →
/// `Self::CHUNK_ROWS` → `insert_chunked_with`) with **field-level
/// positional assertions** after the round-trip.
///
/// Each row carries a distinct `rate`, `decimals`, `rate_calc_near`, and
/// a `Some`/`None`-mixed `swap_path` so that a chunk-boundary swap —
/// where `base_token` X ends up paired with token Y's `rate` JSONB,
/// `decimals`, or `swap_path` JSONB — would surface as a mismatched
/// assertion rather than silently passing on equal-valued rows. Asserting
/// `decimals` positionally is load-bearing: `ExchangeRate::from_raw_rate`
/// pairs `rate` and `decimals` to reconstruct the spot rate, so a silent
/// swap on the `decimals` column would propagate through
/// `to_spot_rate_with_fallback` as an incorrect magnitude with no error
/// path to detect it. This mirrors the drain-path canary on
/// `trade_transaction` and closes the regression gap that count-only
/// assertions leave open against Diesel `Insertable for &[T]` /
/// PG wire-protocol changes.
///
/// `token_rates` has no UNIQUE or CHECK constraints that the in-process
/// build can violate cheaply, so a chunk-boundary rollback test is omitted
/// here — the slice-path atomicity contract is independently validated on
/// the structurally identical `pool_info::insert_chunked_with` (UNIQUE
/// violation in chunk 2) and the Layer 3 CHECK variant on
/// `prediction_record::insert_chunked_with`. The omission therefore
/// depends on `pool_info` carrying the slice-path rollback proof for the
/// whole family: if a UNIQUE / CHECK constraint is ever added to
/// `token_rates`, add a dedicated rollback test here rather than continuing
/// to rely on the proxy.
#[tokio::test]
#[serial(persistence_chunked)]
async fn test_insert_chunked_with_multi_chunk_happy_path() -> Result<()> {
    clean_table().await?;

    let base_ts = chrono::Utc::now().naive_utc();
    let quote: TokenInAccount = TokenAccount::from_str("wrap.near")?.into();
    let bases: Vec<TokenOutAccount> = (0..5)
        .map(|i| {
            TokenAccount::from_str(&format!("happy_{i}.near"))
                .unwrap()
                .into()
        })
        .collect();

    let expected: Vec<TokenRate> = bases
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let ts = base_ts + chrono::TimeDelta::seconds(i as i64);
            let swap_path = if i.is_multiple_of(2) {
                None
            } else {
                Some(SwapPath {
                    pools: vec![SwapPoolInfo {
                        pool_id: 100 + i as u32,
                        token_in_idx: 0,
                        token_out_idx: 1,
                        amount_in: TokenSmallestUnits::from_u128(1_000_000 + i as u128),
                        amount_out: TokenSmallestUnits::from_u128(2_000_000 + (i as u128) * 7),
                    }],
                })
            };
            TokenRate {
                base: b.clone(),
                quote: quote.clone(),
                exchange_rate: ExchangeRate::from_raw_rate(
                    BigDecimal::from(1000 + i as i64),
                    6 + i as u8,
                ),
                timestamp: ts,
                rate_calc_near: 10 + (i as i64) * 3,
                swap_path,
            }
        })
        .collect();

    let new_rates: Vec<NewDbTokenRate> = expected.iter().map(|r| r.to_new_db()).collect();

    let conn = connection_pool::get_test_only().await?;
    conn.interact(move |conn| {
        let chunk_rows = crate::batch::chunk_rows_with_budget(
            NonZeroUsize::new(NewDbTokenRate::COLS.get() * 2).expect("non-zero budget"),
            NewDbTokenRate::COLS,
        );
        NewDbTokenRate::insert_chunked_with(new_rates, chunk_rows, conn)
    })
    .await
    .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    let base_strs: Vec<String> = expected.iter().map(|r| r.base.to_string()).collect();
    let rows: Vec<DbTokenRate> = {
        let conn = connection_pool::get_test_only().await?;
        let base_strs = base_strs.clone();
        conn.interact(move |conn| {
            token_rates::table
                .filter(token_rates::base_token.eq_any(&base_strs))
                .select(DbTokenRate::as_select())
                .load::<DbTokenRate>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??
    };
    assert_eq!(
        rows.len(),
        expected.len(),
        "expected {} rows visible after multi-chunk insert, got {}",
        expected.len(),
        rows.len()
    );

    let by_base: std::collections::HashMap<String, DbTokenRate> = rows
        .into_iter()
        .map(|r| (r.base_token.clone(), r))
        .collect();
    for want in &expected {
        let got = by_base
            .get(&want.base.to_string())
            .unwrap_or_else(|| panic!("base={} not visible after multi-chunk insert", want.base));
        assert_eq!(
            got.rate,
            *want.exchange_rate.raw_rate(),
            "rate mismatch at base={}",
            want.base
        );
        assert_eq!(
            got.decimals,
            want.exchange_rate.decimals() as i16,
            "decimals mismatch at base={}",
            want.base
        );
        assert_eq!(
            got.rate_calc_near, want.rate_calc_near,
            "rate_calc_near mismatch at base={}",
            want.base
        );
        let got_swap_path: Option<SwapPath> = got
            .swap_path
            .as_ref()
            .map(|v| serde_json::from_value(v.clone()).expect("decode swap_path"));
        assert_eq!(
            got_swap_path, want.swap_path,
            "swap_path mismatch at base={}",
            want.base
        );
    }

    clean_table().await?;
    Ok(())
}
