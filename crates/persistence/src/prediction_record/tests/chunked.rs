use super::*;
use bigdecimal::num_bigint::BigInt;
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
/// → `Self::CHUNK_ROWS` → `insert_chunked_with`) with **field-level
/// positional assertions** after the round-trip.
///
/// Each row carries a distinct `predicted_price`, `data_cutoff_time`,
/// `target_time`, and `created_at` per `token`, so that a chunk-boundary
/// swap — where token X ends up paired with token Y's `predicted_price`
/// — would surface as a mismatched assertion rather than silently passing.
/// `predicted_price ↔ token` corruption is the primary data-leakage
/// path here: the optimizer that consumes these rows would learn an
/// incorrect target for the wrong token, so this canary closes the
/// regression gap that count-only assertions leave open.
///
/// `predicted_price` is built via `BigDecimal::new(BigInt::from(mantissa),
/// scale)` with `scale=8` to model the fractional values production uses.
/// PG `NUMERIC` carries no typmod here, so the scale survives the
/// round-trip; bigdecimal's `PartialEq` is representation-based
/// (int_val × scale), so any chunk-boundary regression that shifts scale
/// without changing the numeric magnitude also surfaces as a
/// mismatched assertion. The prior canary used `BigDecimal::from(int)`
/// which always produced `scale=0`, missing that class of bug.
#[tokio::test]
#[serial(persistence_chunked)]
async fn test_insert_chunked_with_multi_chunk_happy_path() -> Result<()> {
    clean_table().await?;

    let base = base_time();
    let quote = "wrap.near".to_string();
    let tokens: Vec<String> = (0..5).map(|i| format!("happy_{i}.near")).collect();

    struct Expected {
        token: String,
        quote_token: String,
        predicted_price: BigDecimal,
        data_cutoff_time: NaiveDateTime,
        target_time: NaiveDateTime,
        created_at: NaiveDateTime,
    }

    let expected: Vec<Expected> = tokens
        .iter()
        .enumerate()
        .map(|(i, token)| {
            let data_cutoff = base + chrono::TimeDelta::seconds(i as i64);
            let target = data_cutoff + chrono::TimeDelta::hours(1 + i as i64);
            let created = data_cutoff + chrono::TimeDelta::milliseconds(i as i64);
            Expected {
                token: token.clone(),
                quote_token: quote.clone(),
                predicted_price: BigDecimal::new(BigInt::from(12345 + (i as i64) * 37), 8),
                data_cutoff_time: data_cutoff,
                target_time: target,
                created_at: created,
            }
        })
        .collect();

    let rows: Vec<NewPredictionRecord> = expected
        .iter()
        .map(|e| {
            NewPredictionRecord::try_new(
                e.token.clone(),
                e.quote_token.clone(),
                e.predicted_price.clone(),
                e.data_cutoff_time,
                e.target_time,
                e.created_at,
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

    let inserted: Vec<DbPredictionRecord> = {
        let tokens = tokens.clone();
        let conn = connection_pool::get_test_only().await?;
        conn.interact(move |conn| {
            prediction_records::table
                .filter(prediction_records::token.eq_any(&tokens))
                .select(DbPredictionRecord::as_select())
                .load::<DbPredictionRecord>(conn)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??
    };
    assert_eq!(
        inserted.len(),
        expected.len(),
        "expected {} rows visible after multi-chunk insert, got {}",
        expected.len(),
        inserted.len()
    );

    let by_token: std::collections::HashMap<String, DbPredictionRecord> =
        inserted.into_iter().map(|r| (r.token.clone(), r)).collect();
    for want in &expected {
        let got = by_token
            .get(&want.token)
            .unwrap_or_else(|| panic!("token={} not visible after multi-chunk insert", want.token));
        assert_eq!(
            got.quote_token, want.quote_token,
            "quote_token mismatch at token={}",
            want.token
        );
        assert_eq!(
            got.predicted_price, want.predicted_price,
            "predicted_price mismatch at token={}",
            want.token
        );
        assert_eq!(
            got.data_cutoff_time, want.data_cutoff_time,
            "data_cutoff_time mismatch at token={}",
            want.token
        );
        assert_eq!(
            got.target_time, want.target_time,
            "target_time mismatch at token={}",
            want.token
        );
        assert_eq!(
            got.created_at, want.created_at,
            "created_at mismatch at token={}",
            want.token
        );
    }

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
#[serial(persistence_chunked)]
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
