use super::*;
use futures::FutureExt;
use std::num::NonZeroUsize;
use std::panic::AssertUnwindSafe;

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

    let safe_ids_for_assert = safe_pool_ids.clone();
    let result = AssertUnwindSafe(async {
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

        for safe_id in &safe_ids_for_assert {
            let row = get_latest(*safe_id as u32).await.unwrap();
            assert!(
                row.is_none(),
                "chunk 1 pool_id={safe_id} leaked despite chunk 2 UNIQUE collision"
            );
        }
    })
    .catch_unwind()
    .await;

    let mut ids = safe_pool_ids.clone();
    ids.push(collision_pool_id);
    let conn = connection_pool::get_test_only().await.unwrap();
    let _ = conn
        .interact(move |conn| {
            use diesel::RunQueryDsl;
            diesel::delete(pool_info::table.filter(pool_info::pool_id.eq_any(ids))).execute(conn)
        })
        .await;

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

/// Multi-chunk happy path on the slice path: 5 rows with chunk_rows=2
/// spans three chunks (2 + 2 + 1). Exercises `.chunks(...)` past the
/// off-by-one boundary on `pool_info::insert_chunked_with` and verifies
/// **field-level positional correspondence** after the round-trip.
///
/// Each row carries a distinct `amounts` pair, `total_fee`, `amp`,
/// `shares_total_supply`, `pool_kind`, and `token_account_ids` so that a
/// chunk-boundary swap — where pool X's row ends up paired with pool Y's
/// JSONB fields — would surface as a mismatched assertion instead of
/// silently passing on equal-valued rows. This is the slice-path
/// counterpart to the drain-path field-level assert on `trade_transaction`,
/// closing the canary gap for Diesel `Insertable for &[T]` /
/// PG wire-protocol regressions that count-only assertions miss.
#[tokio::test]
#[serial(pool_info, persistence_chunked)]
async fn test_insert_chunked_with_multi_chunk_happy_path() {
    let ts = chrono::Utc::now().naive_utc();
    let pool_ids: Vec<i32> = vec![700, 701, 702, 703, 704];

    let expected: Vec<PoolInfo> = pool_ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let token_a = TokenAccount::from_str(&format!("token_a_{i}.near")).unwrap();
            let token_b = TokenAccount::from_str(&format!("token_b_{i}.near")).unwrap();
            let kind = if i.is_multiple_of(2) {
                "STABLE_SWAP"
            } else {
                "SIMPLE_POOL"
            };
            let bare = PoolInfoBared {
                pool_kind: kind.to_string(),
                token_account_ids: vec![token_a, token_b],
                amounts: vec![
                    U128(1_000_000 + i as u128),
                    U128(2_000_000 + (i as u128) * 7),
                ],
                total_fee: 30 + i as u32,
                shares_total_supply: U128(5_000_000 + (i as u128) * 11),
                amp: 100 + i as u64,
            };
            PoolInfo::new(*id as u32, bare, ts)
        })
        .collect();

    let batch: Vec<NewDbPoolInfo> = expected
        .iter()
        .map(|p| to_new_db(p).expect("to_new_db"))
        .collect();

    let cleanup_ids = pool_ids.clone();
    let result = AssertUnwindSafe(async {
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

        for want in &expected {
            let got = get_latest(want.id).await.unwrap().unwrap_or_else(|| {
                panic!("pool_id={} not visible after multi-chunk insert", want.id)
            });
            assert_eq!(
                got.bare.pool_kind, want.bare.pool_kind,
                "pool_kind mismatch at pool_id={}",
                want.id
            );
            assert_eq!(
                got.bare.token_account_ids, want.bare.token_account_ids,
                "token_account_ids mismatch at pool_id={}",
                want.id
            );
            assert_eq!(
                got.bare.amounts, want.bare.amounts,
                "amounts mismatch at pool_id={}",
                want.id
            );
            assert_eq!(
                got.bare.total_fee, want.bare.total_fee,
                "total_fee mismatch at pool_id={}",
                want.id
            );
            assert_eq!(
                got.bare.shares_total_supply, want.bare.shares_total_supply,
                "shares_total_supply mismatch at pool_id={}",
                want.id
            );
            assert_eq!(
                got.bare.amp, want.bare.amp,
                "amp mismatch at pool_id={}",
                want.id
            );
        }
    })
    .catch_unwind()
    .await;

    let conn = connection_pool::get_test_only().await.unwrap();
    let _ = conn
        .interact(move |conn| {
            use diesel::RunQueryDsl;
            diesel::delete(pool_info::table.filter(pool_info::pool_id.eq_any(cleanup_ids)))
                .execute(conn)
        })
        .await;

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
