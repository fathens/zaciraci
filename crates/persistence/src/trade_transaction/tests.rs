use super::*;
use bigdecimal::BigDecimal;
use common::types::TokenSmallestUnits;
use futures::FutureExt;
use std::num::NonZeroUsize;
use std::panic::AssertUnwindSafe;

fn make_tx(tx_id: String, period_id: String, batch_id: String) -> TradeTransaction {
    TradeTransaction {
        tx_id,
        trade_batch_id: batch_id,
        from_token: "wrap.near".to_string(),
        from_amount: TokenSmallestUnits::from_u128(1),
        to_token: "akaia.tkn.near".to_string(),
        to_amount: TokenSmallestUnits::from_u128(1),
        timestamp: chrono::Utc::now().naive_utc(),
        evaluation_period_id: period_id,
        actual_to_amount: None,
    }
}

async fn create_test_evaluation_period() -> String {
    use crate::evaluation_period::NewEvaluationPeriod;
    let new_period = NewEvaluationPeriod::new(
        common::types::YoctoAmount::from_u128(100_000_000_000_000_000_000_000_000),
        vec![],
    );
    new_period.insert_async().await.unwrap().period_id
}

async fn delete_test_evaluation_period(period_id: String) {
    let _ = crate::evaluation_period::EvaluationPeriod::delete_by_period_id_async(period_id).await;
}

#[tokio::test]
async fn test_trade_transaction_crud() {
    let period_id = create_test_evaluation_period().await;
    let batch_id = uuid::Uuid::new_v4().to_string();
    let tx_id = format!("test_tx_{}", uuid::Uuid::new_v4());

    let transaction = TradeTransaction {
        tx_id: tx_id.clone(),
        trade_batch_id: batch_id.clone(),
        from_token: "wrap.near".to_string(),
        from_amount: TokenSmallestUnits::from_u128(1_000_000_000_000_000_000_000_000), // 1 NEAR
        to_token: "akaia.tkn.near".to_string(),
        to_amount: TokenSmallestUnits::from_u128(50_000_000_000_000_000_000_000),
        timestamp: chrono::Utc::now().naive_utc(),
        evaluation_period_id: period_id.clone(),
        actual_to_amount: None,
    };

    let result = AssertUnwindSafe(async {
        let inserted = transaction.insert_async().await.unwrap();
        assert_eq!(inserted.tx_id, tx_id);
        assert_eq!(inserted.trade_batch_id, batch_id);

        let found = TradeTransaction::find_by_tx_id_async(tx_id.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.tx_id, tx_id);
        assert_eq!(
            found.from_amount,
            TokenSmallestUnits::from_u128(1_000_000_000_000_000_000_000_000)
        );
        assert_eq!(
            found.to_amount,
            TokenSmallestUnits::from_u128(50_000_000_000_000_000_000_000)
        );

        let batch_transactions = TradeTransaction::find_by_batch_id_async(batch_id)
            .await
            .unwrap();
        assert_eq!(batch_transactions.len(), 1);
    })
    .catch_unwind()
    .await;

    let _ = TradeTransaction::delete_by_tx_id_async(tx_id).await;
    delete_test_evaluation_period(period_id).await;

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[tokio::test]
async fn test_count_by_evaluation_period() {
    use crate::evaluation_period::NewEvaluationPeriod;

    // 評価期間を作成（外部キー制約のため）
    let new_period = NewEvaluationPeriod::new(
        common::types::YoctoAmount::from_u128(100_000_000_000_000_000_000_000_000),
        vec![],
    );
    let created_period = new_period.insert_async().await.unwrap();
    let period_id = created_period.period_id;
    let batch_id = uuid::Uuid::new_v4().to_string();

    // 同じevaluation_period_idで3つのトランザクションを作成
    let mut tx_ids = Vec::new();
    for i in 0..3 {
        let tx_id = format!("test_tx_count_{}_{}", i, uuid::Uuid::new_v4());
        tx_ids.push(tx_id.clone());

        let transaction = TradeTransaction {
            tx_id,
            trade_batch_id: batch_id.clone(),
            from_token: "wrap.near".to_string(),
            from_amount: TokenSmallestUnits::from_u128(1_000_000_000_000_000_000_000_000),
            to_token: "akaia.tkn.near".to_string(),
            to_amount: TokenSmallestUnits::from_u128(50_000_000_000_000_000_000_000),
            timestamp: chrono::Utc::now().naive_utc(),
            evaluation_period_id: period_id.clone(),
            actual_to_amount: None,
        };

        transaction.insert_async().await.unwrap();
    }

    let result = AssertUnwindSafe(async {
        // count_by_evaluation_period_asyncをテスト
        let count = TradeTransaction::count_by_evaluation_period_async(period_id.clone())
            .await
            .unwrap();
        assert_eq!(count, 3);

        // 存在しないperiod_idの場合は0を返す
        let count_non_existent =
            TradeTransaction::count_by_evaluation_period_async("non_existent_period".to_string())
                .await
                .unwrap();
        assert_eq!(count_non_existent, 0);
    })
    .catch_unwind()
    .await;

    // Cleanup（テスト本体がパニックしても常に実行）
    for tx_id in &tx_ids {
        let _ = TradeTransaction::delete_by_tx_id_async(tx_id.clone()).await;
    }
    let _ = crate::evaluation_period::EvaluationPeriod::delete_by_period_id_async(period_id).await;

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[tokio::test]
async fn test_transaction_with_evaluation_period_id() {
    use crate::evaluation_period::NewEvaluationPeriod;

    // 評価期間を作成（外部キー制約のため）
    let new_period = NewEvaluationPeriod::new(
        common::types::YoctoAmount::from_u128(100_000_000_000_000_000_000_000_000),
        vec![],
    );
    let created_period = new_period.insert_async().await.unwrap();
    let period_id = created_period.period_id;
    let batch_id = uuid::Uuid::new_v4().to_string();
    let tx_id = format!("test_tx_period_{}", uuid::Uuid::new_v4());

    // evaluation_period_id付きトランザクションを作成
    let transaction = TradeTransaction {
        tx_id: tx_id.clone(),
        trade_batch_id: batch_id.clone(),
        from_token: "wrap.near".to_string(),
        from_amount: TokenSmallestUnits::from_u128(1_000_000_000_000_000_000_000_000),
        to_token: "akaia.tkn.near".to_string(),
        to_amount: TokenSmallestUnits::from_u128(50_000_000_000_000_000_000_000),
        timestamp: chrono::Utc::now().naive_utc(),
        evaluation_period_id: period_id.clone(),
        actual_to_amount: None,
    };

    let result = AssertUnwindSafe(async {
        let inserted = transaction.insert_async().await.unwrap();
        assert_eq!(inserted.tx_id, tx_id);
        assert_eq!(inserted.evaluation_period_id, period_id);

        // 取得して確認
        let found = TradeTransaction::find_by_tx_id_async(tx_id.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.evaluation_period_id, period_id);
    })
    .catch_unwind()
    .await;

    // Cleanup（テスト本体がパニックしても常に実行）
    let _ = TradeTransaction::delete_by_tx_id_async(tx_id).await;
    let _ = crate::evaluation_period::EvaluationPeriod::delete_by_period_id_async(period_id).await;

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[tokio::test]
async fn test_actual_to_amount_roundtrip() {
    let period_id = create_test_evaluation_period().await;
    let batch_id = uuid::Uuid::new_v4().to_string();
    let tx_id_with = format!("test_tx_actual_{}", uuid::Uuid::new_v4());
    let tx_id_without = format!("test_tx_no_actual_{}", uuid::Uuid::new_v4());

    let actual_value = BigDecimal::from(49_500_000_000_000_000_000_000_u128);

    // actual_to_amount あり
    let tx_with = TradeTransaction {
        tx_id: tx_id_with.clone(),
        trade_batch_id: batch_id.clone(),
        from_token: "wrap.near".to_string(),
        from_amount: TokenSmallestUnits::from_u128(1_000_000_000_000_000_000_000_000),
        to_token: "akaia.tkn.near".to_string(),
        to_amount: TokenSmallestUnits::from_u128(50_000_000_000_000_000_000_000),
        timestamp: chrono::Utc::now().naive_utc(),
        evaluation_period_id: period_id.clone(),
        actual_to_amount: Some(actual_value.clone()),
    };
    tx_with.insert_async().await.unwrap();

    // actual_to_amount なし
    let tx_without = TradeTransaction {
        tx_id: tx_id_without.clone(),
        trade_batch_id: batch_id.clone(),
        from_token: "wrap.near".to_string(),
        from_amount: TokenSmallestUnits::from_u128(1_000_000_000_000_000_000_000_000),
        to_token: "akaia.tkn.near".to_string(),
        to_amount: TokenSmallestUnits::from_u128(50_000_000_000_000_000_000_000),
        timestamp: chrono::Utc::now().naive_utc(),
        evaluation_period_id: period_id.clone(),
        actual_to_amount: None,
    };
    tx_without.insert_async().await.unwrap();

    let result = AssertUnwindSafe(async {
        let found_with = TradeTransaction::find_by_tx_id_async(tx_id_with.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found_with.actual_to_amount, Some(actual_value));

        let found_without = TradeTransaction::find_by_tx_id_async(tx_id_without.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found_without.actual_to_amount, None);
    })
    .catch_unwind()
    .await;

    let _ = TradeTransaction::delete_by_tx_id_async(tx_id_with).await;
    let _ = TradeTransaction::delete_by_tx_id_async(tx_id_without).await;
    delete_test_evaluation_period(period_id).await;

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[tokio::test]
async fn test_find_by_date_range() {
    let period_id = create_test_evaluation_period().await;
    let batch_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().naive_utc();

    // 3つのトランザクション: 2日前、現在、2日後
    let tx_ids: Vec<String> = (0..3)
        .map(|i| format!("test_tx_range_{}_{}", i, uuid::Uuid::new_v4()))
        .collect();

    let timestamps = [
        now - chrono::TimeDelta::days(2),
        now,
        now + chrono::TimeDelta::days(2),
    ];

    for (tx_id, ts) in tx_ids.iter().zip(timestamps.iter()) {
        let tx = TradeTransaction {
            tx_id: tx_id.clone(),
            trade_batch_id: batch_id.clone(),
            from_token: "wrap.near".to_string(),
            from_amount: TokenSmallestUnits::from_u128(1_000_000_000_000_000_000_000_000),
            to_token: "akaia.tkn.near".to_string(),
            to_amount: TokenSmallestUnits::from_u128(50_000_000_000_000_000_000_000),
            timestamp: *ts,
            evaluation_period_id: period_id.clone(),
            actual_to_amount: None,
        };
        tx.insert_async().await.unwrap();
    }

    let result = AssertUnwindSafe(async {
        // 1日前〜1日後で検索 → 現在の1件のみ
        let found = TradeTransaction::find_by_date_range_async(
            now - chrono::TimeDelta::days(1),
            now + chrono::TimeDelta::days(1),
        )
        .await
        .unwrap();
        let found_ids: Vec<&str> = found.iter().map(|t| t.tx_id.as_str()).collect();
        assert!(
            found_ids.contains(&tx_ids[1].as_str()),
            "現在のトランザクションが含まれるべき"
        );
        assert!(
            !found_ids.contains(&tx_ids[0].as_str()),
            "2日前のトランザクションは含まれないべき"
        );
        assert!(
            !found_ids.contains(&tx_ids[2].as_str()),
            "2日後のトランザクションは含まれないべき"
        );

        // 全範囲で検索 → 3件すべて
        let found_all = TradeTransaction::find_by_date_range_async(
            now - chrono::TimeDelta::days(3),
            now + chrono::TimeDelta::days(3),
        )
        .await
        .unwrap();
        let found_all_ids: Vec<&str> = found_all.iter().map(|t| t.tx_id.as_str()).collect();
        for tx_id in &tx_ids {
            assert!(
                found_all_ids.contains(&tx_id.as_str()),
                "全範囲検索で {} が含まれるべき",
                tx_id
            );
        }

        // タイムスタンプ昇順であることを確認
        for [a, b] in found_all.array_windows::<2>() {
            assert!(a.timestamp <= b.timestamp, "結果は昇順であるべき");
        }
    })
    .catch_unwind()
    .await;

    // Cleanup
    for tx_id in &tx_ids {
        let _ = TradeTransaction::delete_by_tx_id_async(tx_id.clone()).await;
    }
    delete_test_evaluation_period(period_id).await;

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

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
        let conn = crate::connection_pool::get().await.unwrap();
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
/// that `RETURNING *` preserves input order across chunks.
#[tokio::test]
async fn test_insert_chunked_with_multi_chunk_happy_path() {
    let period_id = create_test_evaluation_period().await;
    let batch_id = uuid::Uuid::new_v4().to_string();

    let tx_ids: Vec<String> = (0..5)
        .map(|i| format!("happy_{}_{}", i, uuid::Uuid::new_v4()))
        .collect();
    let batch: Vec<TradeTransaction> = tx_ids
        .iter()
        .map(|tx_id| make_tx(tx_id.clone(), period_id.clone(), batch_id.clone()))
        .collect();

    let result = AssertUnwindSafe(async {
        let expected_ids = tx_ids.clone();
        let conn = crate::connection_pool::get().await.unwrap();
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

        assert_eq!(inserted.len(), expected_ids.len());
        for (i, expected_id) in expected_ids.iter().enumerate() {
            assert_eq!(
                inserted[i].tx_id, *expected_id,
                "RETURNING * must preserve input order across chunks"
            );
        }
    })
    .catch_unwind()
    .await;

    for tx_id in &tx_ids {
        let _ = TradeTransaction::delete_by_tx_id_async(tx_id.clone()).await;
    }
    delete_test_evaluation_period(period_id).await;

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

mod structural;
