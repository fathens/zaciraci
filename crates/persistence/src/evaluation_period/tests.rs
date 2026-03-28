use super::*;
use futures::FutureExt;
use serial_test::serial;
use std::panic::AssertUnwindSafe;

#[tokio::test]
async fn test_create_and_get_evaluation_period() {
    let initial_value = YoctoAmount::from_u128(100_000_000_000_000_000_000_000_000);
    let tokens = vec!["token1.near".to_string(), "token2.near".to_string()];

    let new_period = NewEvaluationPeriod::new(initial_value, tokens);

    assert!(new_period.period_id.starts_with("eval_"));
    assert_eq!(new_period.selected_tokens.as_ref().unwrap().len(), 2);

    // 実際のDBへの挿入テストは統合テストで行う
}

#[test]
fn test_new_evaluation_period_with_empty_tokens() {
    let initial_value = YoctoAmount::from_u128(100_000_000_000_000_000_000_000_000);
    let tokens = vec![];

    let new_period = NewEvaluationPeriod::new(initial_value, tokens);

    assert_eq!(new_period.selected_tokens, None);
}

#[tokio::test]
async fn test_initial_value_db_roundtrip() {
    // 大きな値で DB ラウンドトリップが正しく YoctoAmount に戻ることを確認
    let initial_value = YoctoAmount::from_u128(100_000_000_000_000_000_000_000_000);
    let new_period = NewEvaluationPeriod::new(initial_value.clone(), vec![]);

    let created = new_period.insert_async().await.unwrap();
    let period_id = created.period_id.clone();

    let result = AssertUnwindSafe(async {
        assert_eq!(created.initial_value, initial_value);

        // DB から再取得しても一致
        let fetched = EvaluationPeriod::get_by_period_id_async(period_id.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched.initial_value, initial_value);
    })
    .catch_unwind()
    .await;

    // Cleanup（テスト本体がパニックしても常に実行）
    let _ = EvaluationPeriod::delete_by_period_id_async(period_id).await;

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

/// 古い created_at を持つ evaluation_period を直接 INSERT するヘルパー
async fn insert_with_created_at(period_id: &str, created_at: NaiveDateTime) -> Result<()> {
    use diesel::sql_types::{Numeric, Timestamp, Varchar};

    let conn = connection_pool::get().await?;
    let period_id = period_id.to_string();
    conn.interact(move |conn| {
        diesel::sql_query(
            "INSERT INTO evaluation_periods (period_id, start_time, initial_value, created_at) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind::<Varchar, _>(&period_id)
        .bind::<Timestamp, _>(created_at)
        .bind::<Numeric, _>(BigDecimal::from(0))
        .bind::<Timestamp, _>(created_at)
        .execute(conn)
    })
    .await
    .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;
    Ok(())
}

#[tokio::test]
#[serial(evaluation_period)]
async fn test_cleanup_old_records() {
    let now = chrono::Utc::now().naive_utc();
    let old_id = format!("eval_test_old_{}", uuid::Uuid::new_v4());
    let recent_id = format!("eval_test_recent_{}", uuid::Uuid::new_v4());

    // 400日前のレコードと10日前のレコードを作成
    insert_with_created_at(&old_id, now - chrono::TimeDelta::days(400))
        .await
        .unwrap();
    insert_with_created_at(&recent_id, now - chrono::TimeDelta::days(10))
        .await
        .unwrap();

    // 365日より古いレコードを削除
    cleanup_old_records(365).await.unwrap();

    // 古いレコードは削除、新しいレコードは残る
    let old = EvaluationPeriod::get_by_period_id_async(old_id)
        .await
        .unwrap();
    assert!(old.is_none(), "400-day-old record should be deleted");

    let recent = EvaluationPeriod::get_by_period_id_async(recent_id.clone())
        .await
        .unwrap();
    assert!(recent.is_some(), "10-day-old record should remain");

    let _ = EvaluationPeriod::delete_by_period_id_async(recent_id).await;
}

#[tokio::test]
#[serial(evaluation_period)]
async fn test_cleanup_old_records_zero_days_skips() {
    let now = chrono::Utc::now().naive_utc();
    let id = format!("eval_test_zero_{}", uuid::Uuid::new_v4());

    // 古いレコードを作成
    insert_with_created_at(&id, now - chrono::TimeDelta::days(400))
        .await
        .unwrap();

    // retention_days=0 は何も削除しない
    let result = cleanup_old_records(0).await;
    assert!(result.is_ok());

    // レコードが残っていることを確認
    let record = EvaluationPeriod::get_by_period_id_async(id.clone())
        .await
        .unwrap();
    assert!(
        record.is_some(),
        "record should remain when retention_days is 0"
    );

    let _ = EvaluationPeriod::delete_by_period_id_async(id).await;
}

#[tokio::test]
#[serial(evaluation_period)]
async fn test_cleanup_old_records_minimum_retention() {
    let now = chrono::Utc::now().naive_utc();
    let id = format!("eval_test_min_{}", uuid::Uuid::new_v4());

    // 40日前のレコード
    insert_with_created_at(&id, now - chrono::TimeDelta::days(40))
        .await
        .unwrap();

    // retention_days=1 → effective=30 (MIN_RETENTION_DAYS)
    // 40日前 > 30日前 なので削除される
    cleanup_old_records(1).await.unwrap();

    let record = EvaluationPeriod::get_by_period_id_async(id.clone())
        .await
        .unwrap();
    assert!(
        record.is_none(),
        "40-day-old record should be deleted with effective retention of 30 days"
    );
}

#[tokio::test]
#[serial(evaluation_period)]
async fn test_cleanup_cascades_to_child_tables() {
    use crate::portfolio_holding::{NewPortfolioHolding, PortfolioHolding};
    use crate::trade_transaction::TradeTransaction;

    let now = chrono::Utc::now().naive_utc();
    let period_id = format!("eval_test_cascade_{}", uuid::Uuid::new_v4());
    let old_time = now - chrono::TimeDelta::days(400);

    // 古い evaluation_period を作成
    insert_with_created_at(&period_id, old_time).await.unwrap();

    // 子テーブルにレコードを追加
    let holding = NewPortfolioHolding {
        evaluation_period_id: period_id.clone(),
        timestamp: old_time,
        token_holdings: serde_json::json!([]),
    };
    PortfolioHolding::insert_async(holding).await.unwrap();

    let tx_id = format!("test_tx_{}", uuid::Uuid::new_v4());
    let tx = TradeTransaction {
        tx_id: tx_id.clone(),
        trade_batch_id: "test_batch".to_string(),
        from_token: "wrap.near".to_string(),
        from_amount: "1000".parse().unwrap(),
        to_token: "usdt.tether-token.near".to_string(),
        to_amount: "100".parse().unwrap(),
        timestamp: old_time,
        evaluation_period_id: Some(period_id.clone()),
        actual_to_amount: None,
    };
    tx.insert_async().await.unwrap();

    // cleanup で親を削除 → CASCADE で子も消える
    cleanup_old_records(365).await.unwrap();

    // 親が消えている
    let parent = EvaluationPeriod::get_by_period_id_async(period_id.clone())
        .await
        .unwrap();
    assert!(parent.is_none(), "parent should be deleted");

    // portfolio_holdings も消えている
    let holdings = PortfolioHolding::get_by_period_async(period_id)
        .await
        .unwrap();
    assert!(
        holdings.is_empty(),
        "child portfolio_holdings should be cascade-deleted"
    );

    // trade_transaction も消えている
    let tx = TradeTransaction::find_by_tx_id_async(tx_id).await.unwrap();
    assert!(
        tx.is_none(),
        "child trade_transaction should be cascade-deleted"
    );
}
