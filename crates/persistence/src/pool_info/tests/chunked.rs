use super::*;
use std::num::NonZeroUsize;

/// UNIQUE(pool_id, timestamp) collision in chunk 2 must roll back chunk 1.
///
/// Layout (chunk_rows=2):
///   chunk 1: pool_id=600/601 at the chunk timestamp  (would succeed alone)
///   chunk 2: pool_id=600 at the same timestamp        (UNIQUE violation)
///
/// `insert_chunked_with` wraps every chunk in one `conn.transaction`, so
/// the chunk 2 error rolls back chunk 1. This is the same contract as the
/// drain-path test on `trade_transactions`; reproducing it on the slice
/// path (`.chunks(...)`) verifies both code paths preserve atomicity.
#[tokio::test]
#[serial(pool_info, persistence_chunked)]
async fn test_insert_chunked_with_rolls_back_chunk1_on_chunk2_collision() {
    let ts = chrono::Utc::now().naive_utc();
    let collision_pool_id: i32 = 600;
    let safe_pool_ids: Vec<i32> = vec![601, 602];

    fn build_new_db(pool_id: i32, ts: chrono::NaiveDateTime) -> NewDbPoolInfo {
        let mut p = create_test_pool_info();
        p.id = pool_id as u32;
        p.timestamp = ts;
        to_new_db(&p).expect("to_new_db")
    }

    {
        let marker = build_new_db(collision_pool_id, ts);
        let conn = connection_pool::get_test_only().await.unwrap();
        conn.interact(move |conn| {
            use diesel::RunQueryDsl;
            diesel::insert_into(pool_info::table)
                .values(&marker)
                .execute(conn)
        })
        .await
        .unwrap()
        .unwrap();
    }

    let mut batch: Vec<NewDbPoolInfo> = safe_pool_ids
        .iter()
        .map(|id| build_new_db(*id, ts))
        .collect();
    batch.push(build_new_db(collision_pool_id, ts));

    let conn = connection_pool::get_test_only().await.unwrap();
    let outcome = conn
        .interact(move |conn| {
            let chunk_rows = crate::batch::chunk_rows_with_budget(
                NonZeroUsize::new(NewDbPoolInfo::COLS.get() * 2).expect("non-zero budget"),
                NewDbPoolInfo::COLS,
            );
            NewDbPoolInfo::insert_chunked_with(batch, chunk_rows, conn)
        })
        .await
        .unwrap();
    assert!(
        outcome.is_err(),
        "chunk 2 UNIQUE collision should propagate as an error"
    );

    for safe_id in &safe_pool_ids {
        let row = get_latest(*safe_id as u32).await.unwrap();
        assert!(
            row.is_none(),
            "chunk 1 pool_id={safe_id} leaked despite chunk 2 UNIQUE collision"
        );
    }

    let mut ids = safe_pool_ids.clone();
    ids.push(collision_pool_id);
    let conn = connection_pool::get_test_only().await.unwrap();
    conn.interact(move |conn| {
        use diesel::RunQueryDsl;
        diesel::delete(pool_info::table.filter(pool_info::pool_id.eq_any(ids))).execute(conn)
    })
    .await
    .unwrap()
    .unwrap();
}

/// Multi-chunk happy path on the slice path: 5 rows with chunk_rows=2
/// spans three chunks (2 + 2 + 1). Exercises `.chunks(...)` past the
/// off-by-one boundary on `pool_info::insert_chunked_with`.
#[tokio::test]
#[serial(pool_info, persistence_chunked)]
async fn test_insert_chunked_with_multi_chunk_happy_path() {
    let ts = chrono::Utc::now().naive_utc();
    let pool_ids: Vec<i32> = vec![700, 701, 702, 703, 704];

    let batch: Vec<NewDbPoolInfo> = pool_ids
        .iter()
        .map(|id| {
            let mut p = create_test_pool_info();
            p.id = *id as u32;
            p.timestamp = ts;
            to_new_db(&p).expect("to_new_db")
        })
        .collect();

    let conn = connection_pool::get_test_only().await.unwrap();
    conn.interact(move |conn| {
        let chunk_rows = crate::batch::chunk_rows_with_budget(
            NonZeroUsize::new(NewDbPoolInfo::COLS.get() * 2).expect("non-zero budget"),
            NewDbPoolInfo::COLS,
        );
        NewDbPoolInfo::insert_chunked_with(batch, chunk_rows, conn)
    })
    .await
    .unwrap()
    .unwrap();

    for pool_id in &pool_ids {
        let row = get_latest(*pool_id as u32).await.unwrap();
        assert!(
            row.is_some(),
            "expected pool_id={pool_id} to be visible after multi-chunk insert"
        );
    }

    let conn = connection_pool::get_test_only().await.unwrap();
    conn.interact(move |conn| {
        use diesel::RunQueryDsl;
        diesel::delete(pool_info::table.filter(pool_info::pool_id.eq_any(pool_ids))).execute(conn)
    })
    .await
    .unwrap()
    .unwrap();
}
