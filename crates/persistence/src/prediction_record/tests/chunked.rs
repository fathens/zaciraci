use super::*;
use std::num::NonZeroUsize;

fn base_time() -> NaiveDateTime {
    chrono::DateTime::from_timestamp(1_700_000_000, 0)
        .unwrap()
        .naive_utc()
}

/// Multi-chunk happy path: 5 rows with chunk_rows=2 spans three chunks
/// (2 + 2 + 1). Exercises the `.chunks(...)` loop in
/// `NewPredictionRecord::insert_chunked_with` past the off-by-one boundary
/// and verifies the production-side wiring (`PredictionRecord::batch_insert`
/// → `Self::CHUNK_ROWS` → `insert_chunked_with`) so that a regression in any
/// of those layers — not just the slice logic itself — would be caught.
#[tokio::test]
#[serial]
async fn test_insert_chunked_with_multi_chunk_happy_path() -> Result<()> {
    clean_table().await?;

    let base = base_time();
    let quote = "wrap.near".to_string();
    let tokens: Vec<String> = (0..5).map(|i| format!("happy_{i}.near")).collect();

    let rows: Vec<NewPredictionRecord> = tokens
        .iter()
        .enumerate()
        .map(|(i, token)| {
            let data_cutoff = base + chrono::TimeDelta::seconds(i as i64);
            let target = data_cutoff + chrono::TimeDelta::hours(1);
            NewPredictionRecord::try_new(
                token.clone(),
                quote.clone(),
                BigDecimal::from(100 + i as i64),
                data_cutoff,
                target,
                data_cutoff,
            )
            .expect("valid record")
        })
        .collect();

    let conn = connection_pool::get_test_only().await?;
    conn.interact(move |conn| {
        let chunk_rows = crate::batch::chunk_rows_with_budget(
            NonZeroUsize::new(NewPredictionRecord::COLS.get() * 2).expect("non-zero budget"),
            NewPredictionRecord::COLS,
        );
        NewPredictionRecord::insert_chunked_with(rows, chunk_rows, conn)
    })
    .await
    .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

    let inserted_count = {
        let tokens = tokens.clone();
        let conn = connection_pool::get_test_only().await?;
        conn.interact(move |conn| {
            prediction_records::table
                .filter(prediction_records::token.eq_any(&tokens))
                .count()
                .get_result::<i64>(conn)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??
    };
    assert_eq!(
        inserted_count, 5,
        "expected 5 rows visible after multi-chunk insert, got {inserted_count}"
    );

    clean_table().await?;
    Ok(())
}

/// Layer 3 CHECK violation in chunk 2 must roll back chunk 1.
///
/// Layout (chunk_rows=2):
///   chunk 1: rows 0, 1 — valid (created_at == data_cutoff_time)
///   chunk 2: row 2     — `new_unchecked` bypasses Layer 1, INSERT hits
///                        `created_at_geq_data_cutoff` Layer 3 CHECK → fail
///
/// `insert_chunked_with` wraps every chunk in one `conn.transaction`, so the
/// chunk 2 CHECK violation must roll back chunk 1. This is the same contract
/// validated for `pool_info` (UNIQUE collision) and `trade_transaction`
/// (PK collision); reproducing it here exercises the prediction_records
/// table's Layer 3 CHECK as the error trigger.
#[tokio::test]
#[serial]
async fn test_insert_chunked_with_rolls_back_chunk1_on_chunk2_check_violation() -> Result<()> {
    clean_table().await?;

    let base = base_time();
    let quote = "wrap.near".to_string();
    let safe_tokens: Vec<String> = (0..2).map(|i| format!("rb_safe_{i}.near")).collect();
    let violator_token = "rb_violator.near".to_string();

    let mut rows: Vec<NewPredictionRecord> = safe_tokens
        .iter()
        .enumerate()
        .map(|(i, token)| {
            let data_cutoff = base + chrono::TimeDelta::seconds(i as i64);
            let target = data_cutoff + chrono::TimeDelta::hours(1);
            NewPredictionRecord::try_new(
                token.clone(),
                quote.clone(),
                BigDecimal::from(100 + i as i64),
                data_cutoff,
                target,
                data_cutoff,
            )
            .expect("valid record")
        })
        .collect();

    // Layer 3 CHECK violator: created_at < data_cutoff_time.
    rows.push(NewPredictionRecord::new_unchecked(
        violator_token.clone(),
        quote.clone(),
        BigDecimal::from(200),
        base + chrono::TimeDelta::hours(1),
        base + chrono::TimeDelta::hours(2),
        base, // < data_cutoff_time → CHECK violation
    ));

    let conn = connection_pool::get_test_only().await?;
    let outcome = conn
        .interact(move |conn| {
            let chunk_rows = crate::batch::chunk_rows_with_budget(
                NonZeroUsize::new(NewPredictionRecord::COLS.get() * 2).expect("non-zero budget"),
                NewPredictionRecord::COLS,
            );
            NewPredictionRecord::insert_chunked_with(rows, chunk_rows, conn)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))?;
    assert!(
        outcome.is_err(),
        "chunk 2 CHECK violation should propagate as an error"
    );

    let leaked_count = {
        let mut all_tokens = safe_tokens.clone();
        all_tokens.push(violator_token.clone());
        let conn = connection_pool::get_test_only().await?;
        conn.interact(move |conn| {
            prediction_records::table
                .filter(prediction_records::token.eq_any(&all_tokens))
                .count()
                .get_result::<i64>(conn)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??
    };
    assert_eq!(
        leaked_count, 0,
        "chunk 1 rows leaked despite chunk 2 CHECK violation (count={leaked_count})"
    );

    clean_table().await?;
    Ok(())
}
