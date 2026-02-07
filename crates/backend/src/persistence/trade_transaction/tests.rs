use super::*;

#[tokio::test]
async fn test_trade_transaction_crud() {
    let batch_id = uuid::Uuid::new_v4().to_string();
    let tx_id = format!("test_tx_{}", uuid::Uuid::new_v4());

    let transaction = TradeTransaction {
        tx_id: tx_id.clone(),
        trade_batch_id: batch_id.clone(),
        from_token: "wrap.near".to_string(),
        from_amount: BigDecimal::from(1000000000000000000000000i128), // 1 NEAR
        to_token: "akaia.tkn.near".to_string(),
        to_amount: BigDecimal::from(50000000000000000000000i128),
        timestamp: chrono::Utc::now().naive_utc(),
        evaluation_period_id: None,
    };

    let result = transaction.insert_async().await.unwrap();
    assert_eq!(result.tx_id, tx_id);
    assert_eq!(result.trade_batch_id, batch_id);

    let found = TradeTransaction::find_by_tx_id_async(tx_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.tx_id, tx_id);

    let batch_transactions = TradeTransaction::find_by_batch_id_async(batch_id)
        .await
        .unwrap();
    assert_eq!(batch_transactions.len(), 1);

    TradeTransaction::delete_by_tx_id_async(tx_id)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_count_by_evaluation_period() {
    use crate::persistence::evaluation_period::NewEvaluationPeriod;

    // 評価期間を作成（外部キー制約のため）
    let new_period =
        NewEvaluationPeriod::new(BigDecimal::from(100000000000000000000000000i128), vec![]);
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
            from_amount: BigDecimal::from(1000000000000000000000000i128),
            to_token: "akaia.tkn.near".to_string(),
            to_amount: BigDecimal::from(50000000000000000000000i128),
            timestamp: chrono::Utc::now().naive_utc(),
            evaluation_period_id: Some(period_id.clone()),
        };

        transaction.insert_async().await.unwrap();
    }

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

    // Cleanup
    for tx_id in tx_ids {
        TradeTransaction::delete_by_tx_id_async(tx_id)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn test_transaction_with_evaluation_period_id() {
    use crate::persistence::evaluation_period::NewEvaluationPeriod;

    // 評価期間を作成（外部キー制約のため）
    let new_period =
        NewEvaluationPeriod::new(BigDecimal::from(100000000000000000000000000i128), vec![]);
    let created_period = new_period.insert_async().await.unwrap();
    let period_id = created_period.period_id;
    let batch_id = uuid::Uuid::new_v4().to_string();
    let tx_id = format!("test_tx_period_{}", uuid::Uuid::new_v4());

    // evaluation_period_id付きトランザクションを作成
    let transaction = TradeTransaction {
        tx_id: tx_id.clone(),
        trade_batch_id: batch_id.clone(),
        from_token: "wrap.near".to_string(),
        from_amount: BigDecimal::from(1000000000000000000000000i128),
        to_token: "akaia.tkn.near".to_string(),
        to_amount: BigDecimal::from(50000000000000000000000i128),
        timestamp: chrono::Utc::now().naive_utc(),
        evaluation_period_id: Some(period_id.clone()),
    };

    let result = transaction.insert_async().await.unwrap();
    assert_eq!(result.tx_id, tx_id);
    assert_eq!(result.evaluation_period_id, Some(period_id.clone()));

    // 取得して確認
    let found = TradeTransaction::find_by_tx_id_async(tx_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.evaluation_period_id, Some(period_id));

    // Cleanup
    TradeTransaction::delete_by_tx_id_async(tx_id)
        .await
        .unwrap();
}
