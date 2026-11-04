use super::*;
use bigdecimal::BigDecimal;
use chrono::SubsecRound;
use common::types::TokenSmallestUnits;
use futures::FutureExt;
use serial_test::serial;
use std::num::NonZeroUsize;
use std::panic::AssertUnwindSafe;

/// PK collision in chunk 2 must roll back chunk 1.
///
/// Layout (chunk_rows=2):
///   chunk 1: indices 0, 1  (unique tx_ids)
///   chunk 2: index 2       (tx_id duplicates a pre-existing row → INSERT fails)
///
/// If `insert_chunked_with` wrapped each chunk in its own implicit
/// transaction, the chunk 1 rows would persist after the chunk 2 INSERT
/// blew up. Wrapping the whole loop in `conn.transaction(...)` makes the
/// chunk 2 error propagate up and roll back chunk 1 too. Placing the
/// collision in chunk 2 (not chunk 1) is what exercises this contract:
/// a chunk 1 collision would abort before chunk 1 ever commits.
#[tokio::test]
#[serial(persistence_chunked)]
async fn test_insert_chunked_with_rolls_back_chunk1_on_chunk2_collision() {
    let period_id = create_test_evaluation_period().await;
    let batch_id = uuid::Uuid::new_v4().to_string();
    let collision_tx_id = format!("collision_{}", uuid::Uuid::new_v4());

    let marker = make_tx(collision_tx_id.clone(), period_id.clone(), batch_id.clone());
    marker.insert_async().await.unwrap();

    let safe_tx_ids: Vec<String> = (0..2)
        .map(|i| format!("rb_{}_{}", i, uuid::Uuid::new_v4()))
        .collect();

    let mut batch: Vec<TradeTransaction> = safe_tx_ids
        .iter()
        .map(|tx_id| make_tx(tx_id.clone(), period_id.clone(), batch_id.clone()))
        .collect();
    batch.push(make_tx(
        collision_tx_id.clone(),
        period_id.clone(),
        batch_id.clone(),
    ));

    let result = AssertUnwindSafe(async {
        let conn = crate::connection_pool::get_test_only().await.unwrap();
        let outcome = conn
            .interact(move |conn| {
                let chunk_rows = crate::batch::chunk_rows_with_budget(
                    NonZeroUsize::new(TradeTransaction::COLS.get() * 2).expect("non-zero budget"),
                    TradeTransaction::COLS,
                );
                TradeTransaction::insert_chunked_with(batch, chunk_rows, conn)
            })
            .await
            .unwrap();
        assert!(
            outcome.is_err(),
            "chunk 2 collision should propagate as an error"
        );

        for tx_id in &safe_tx_ids {
            let found = TradeTransaction::find_by_tx_id_async(tx_id.clone())
                .await
                .unwrap();
            assert!(
                found.is_none(),
                "chunk 1 row {tx_id} leaked despite chunk 2 collision"
            );
        }
    })
    .catch_unwind()
    .await;

    for tx_id in &safe_tx_ids {
        let _ = TradeTransaction::delete_by_tx_id_async(tx_id.clone()).await;
    }
    let _ = TradeTransaction::delete_by_tx_id_async(collision_tx_id).await;
    delete_test_evaluation_period(period_id).await;

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

/// Multi-chunk happy path: 5 rows with chunk_rows=2 spans three chunks
/// (2 + 2 + 1). Exercises the `while !empty { take = len.min(...) }` loop
/// in `insert_chunked_with` past the off-by-one boundary and verifies
/// that `RETURNING *` preserves input order **and** each field's value
/// across chunks. Every row carries distinct values for **every**
/// `Insertable` field — owned `String`s (`from_token`/`to_token`/
/// `trade_batch_id`), `TokenSmallestUnits` amounts, `NaiveDateTime`
/// timestamp, and a `Some`/`None`-mixed `actual_to_amount` — so the
/// drain-based move (which transfers heap pointers per element) is
/// verified field-by-field. The owned-`String` columns are the primary
/// canary for the drain pattern: any positional swap where row X's
/// `from_token` ends up in row Y's slot would surface as a mismatched
/// assertion rather than silently passing on equal-valued rows.
#[tokio::test]
#[serial(persistence_chunked)]
async fn test_insert_chunked_with_multi_chunk_happy_path() {
    let period_id = create_test_evaluation_period().await;
    let batch_id_prefix = uuid::Uuid::new_v4().to_string();
    // PostgreSQL TIMESTAMP truncates to microseconds, so match that on input.
    let base_ts = chrono::Utc::now().naive_utc().trunc_subsecs(6);

    let expected: Vec<TradeTransaction> = (0..5)
        .map(|i| {
            let i = i as u128;
            TradeTransaction {
                tx_id: format!("happy_{}_{}", i, uuid::Uuid::new_v4()),
                trade_batch_id: format!("{batch_id_prefix}_{i}"),
                from_token: format!("wrap_{i}.near"),
                from_amount: TokenSmallestUnits::from_u128(1_000_000 + i),
                to_token: format!("akaia_{i}.tkn.near"),
                to_amount: TokenSmallestUnits::from_u128(2_000_000 + i * 7),
                timestamp: base_ts + chrono::TimeDelta::seconds(i as i64),
                evaluation_period_id: period_id.clone(),
                actual_to_amount: if i.is_multiple_of(2) {
                    None
                } else {
                    Some(BigDecimal::from(3_000_000_u128 + i * 11))
                },
            }
        })
        .collect();
    let batch = expected.clone();

    let result = AssertUnwindSafe(async {
        let conn = crate::connection_pool::get_test_only().await.unwrap();
        let inserted = conn
            .interact(move |conn| {
                let chunk_rows = crate::batch::chunk_rows_with_budget(
                    NonZeroUsize::new(TradeTransaction::COLS.get() * 2).expect("non-zero budget"),
                    TradeTransaction::COLS,
                );
                TradeTransaction::insert_chunked_with(batch, chunk_rows, conn)
            })
            .await
            .unwrap()
            .unwrap();

        assert_eq!(inserted.len(), expected.len());
        for (i, want) in expected.iter().enumerate() {
            assert_eq!(
                inserted[i].tx_id, want.tx_id,
                "RETURNING * must preserve input order across chunks at index {i}"
            );
            assert_eq!(
                inserted[i].trade_batch_id, want.trade_batch_id,
                "trade_batch_id mismatch at index {i}"
            );
            assert_eq!(
                inserted[i].from_token, want.from_token,
                "from_token mismatch at index {i}"
            );
            assert_eq!(
                inserted[i].from_amount, want.from_amount,
                "from_amount mismatch at index {i}"
            );
            assert_eq!(
                inserted[i].to_token, want.to_token,
                "to_token mismatch at index {i}"
            );
            assert_eq!(
                inserted[i].to_amount, want.to_amount,
                "to_amount mismatch at index {i}"
            );
            assert_eq!(
                inserted[i].timestamp, want.timestamp,
                "timestamp mismatch at index {i}"
            );
            assert_eq!(
                inserted[i].evaluation_period_id, want.evaluation_period_id,
                "evaluation_period_id mismatch at index {i}"
            );
            assert_eq!(
                inserted[i].actual_to_amount, want.actual_to_amount,
                "actual_to_amount mismatch at index {i}"
            );
        }
    })
    .catch_unwind()
    .await;

    for tx in &expected {
        let _ = TradeTransaction::delete_by_tx_id_async(tx.tx_id.clone()).await;
    }
    delete_test_evaluation_period(period_id).await;

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
