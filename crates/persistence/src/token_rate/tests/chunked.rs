use super::*;
use std::num::NonZeroUsize;

/// Multi-chunk happy path: 5 rows with chunk_rows=2 spans three chunks
/// (2 + 2 + 1). Exercises the `.chunks(...)` loop in
/// `NewDbTokenRate::insert_chunked_with` past the off-by-one boundary and
/// verifies the production-side wiring (`TokenRate::batch_insert` →
/// `Self::CHUNK_ROWS` → `insert_chunked_with`).
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

    let base = chrono::Utc::now().naive_utc();
    let quote: TokenInAccount = TokenAccount::from_str("wrap.near")?.into();
    let bases: Vec<TokenOutAccount> = (0..5)
        .map(|i| {
            TokenAccount::from_str(&format!("happy_{i}.near"))
                .unwrap()
                .into()
        })
        .collect();

    let new_rates: Vec<NewDbTokenRate> = bases
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let ts = base + chrono::TimeDelta::seconds(i as i64);
            make_token_rate(b.clone(), quote.clone(), 1000 + i as i64, ts).to_new_db()
        })
        .collect();

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

    let token_strs: Vec<String> = bases.iter().map(|b| b.to_string()).collect();
    let inserted_count = {
        let conn = connection_pool::get_test_only().await?;
        let token_strs = token_strs.clone();
        conn.interact(move |conn| {
            token_rates::table
                .filter(token_rates::base_token.eq_any(&token_strs))
                .count()
                .get_result::<i64>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??
    };
    assert_eq!(
        inserted_count, 5,
        "expected 5 rows visible after multi-chunk insert, got {inserted_count}"
    );

    clean_table().await?;
    Ok(())
}
