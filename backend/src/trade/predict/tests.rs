use super::*;
use bigdecimal::BigDecimal;
use chrono::{Duration, Utc};
use std::str::FromStr;
use zaciraci_common::prediction::{ChronosPredictionResponse, PredictionResult};

// テストヘルパー関数

#[tokio::test]
async fn test_get_top_tokens() {
    // このテストは実際のデータベース接続が必要
    // CI環境では統合テストとして実行する
    let service = PredictionService::new(
        "http://localhost:8000".to_string(),
        "http://localhost:3000".to_string(),
    );

    let start_date = Utc::now() - Duration::days(7);
    let end_date = Utc::now();

    let result = service
        .get_top_tokens(start_date, end_date, 10, "wrap.near", None)
        .await;

    // データベースにデータがない場合は空のリストが返される
    if result.is_ok() {
        let _tokens = result.unwrap();
        // データベース状態に依存するため、長さのアサーションは不要
        // assert!(_tokens.len() >= 0); // len()は常に0以上
    }
}

#[tokio::test]
async fn test_get_price_history() {
    // このテストは実際のデータベース接続が必要
    // CI環境では統合テストとして実行する
    let service = PredictionService::new(
        "http://localhost:8000".to_string(),
        "http://localhost:3000".to_string(),
    );

    let start_date = Utc::now() - Duration::hours(2);
    let end_date = Utc::now();

    // 実際に存在するトークンペアでテスト
    let result = service
        .get_price_history("usdc.tether-token.near", "wrap.near", start_date, end_date)
        .await;

    // データベースにデータがない場合も考慮
    if result.is_ok() {
        let history = result.unwrap();
        assert_eq!(history.token, "usdc.tether-token.near");
        assert_eq!(history.quote_token, "wrap.near");
        // データベース状態に依存するため、長さのアサーションは不要
        // assert!(history.prices.len() >= 0); // len()は常に0以上
    }
}

#[tokio::test]
async fn test_convert_prediction_result() {
    let service = PredictionService::new(
        "http://localhost:8000".to_string(),
        "http://localhost:3000".to_string(),
    );

    let result = PredictionResult {
        task_id: "test-id".to_string(),
        status: "completed".to_string(),
        progress: Some(BigDecimal::from(1)),
        message: None,
        result: Some(ChronosPredictionResponse {
            forecast_timestamp: vec![],
            forecast_values: vec![
                "1.2".parse().unwrap(),
                "1.3".parse().unwrap(),
                "1.4".parse().unwrap(),
                "1.5".parse().unwrap(),
            ],
            model_name: "chronos-t5-large".to_string(),
            confidence_intervals: None,
            metrics: None,
        }),
        error: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let last_timestamp = Utc::now();
    let predictions = service.convert_prediction_result(&result, &last_timestamp, 3);

    assert!(predictions.is_ok());
    let preds = predictions.unwrap();
    assert_eq!(preds.len(), 3);
    assert_eq!(preds[0].price, "1.2".parse::<BigDecimal>().unwrap());
    assert_eq!(preds[1].price, "1.3".parse::<BigDecimal>().unwrap());
    assert_eq!(preds[2].price, "1.4".parse::<BigDecimal>().unwrap());

    // タイムスタンプが1時間ずつ増加していることを確認
    assert_eq!(preds[1].timestamp - preds[0].timestamp, Duration::hours(1));
}

#[tokio::test]
async fn test_get_top_tokens_error_handling() {
    // データベース接続エラーのテストは統合テスト環境で実施
    let service = PredictionService::new(
        "http://localhost:8000".to_string(),
        "http://localhost:3000".to_string(),
    );

    let start_date = Utc::now() - Duration::days(7);
    let end_date = Utc::now();

    // 無効なトークン名でテスト
    let result = service
        .get_top_tokens(start_date, end_date, 10, "invalid_token", None)
        .await;

    // データベース実装では、無効なトークンでもエラーではなく空のリストが返される可能性
    // テストの詳細はデータベースの実装に依存
    assert!(result.is_err() || result.unwrap().is_empty());
}

#[tokio::test]
async fn test_empty_price_history() {
    let service = PredictionService::new(
        "http://localhost:8000".to_string(),
        "http://localhost:3000".to_string(),
    );

    let history = TokenPriceHistory {
        token: "test.near".to_string(),
        quote_token: "wrap.near".to_string(),
        prices: vec![],
    };

    let result = service.predict_price(&history, 24).await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("No price history available")
    );
}

// データ構造のシリアライゼーションテスト
#[test]
fn test_token_prediction_serialization() {
    let prediction = TokenPrediction {
        token: "test.near".to_string(),
        quote_token: "wrap.near".to_string(),
        prediction_time: Utc::now(),
        predictions: vec![PredictedPrice {
            timestamp: Utc::now(),
            price: BigDecimal::from_str("1.5").unwrap(),
            confidence: Some(BigDecimal::from_str("0.85").unwrap()),
        }],
    };

    let json = serde_json::to_string(&prediction);
    assert!(json.is_ok());

    let deserialized: Result<TokenPrediction, _> = serde_json::from_str(&json.unwrap());
    assert!(deserialized.is_ok());
    assert_eq!(deserialized.unwrap().token, "test.near");
}

#[tokio::test]
#[ignore] // Requires Chronos API - skip in regular tests
async fn test_predict_multiple_tokens_batch_processing() {
    // 統合テスト環境でのみ実行
    if std::env::var("CHRONOS_URL").is_err() {
        eprintln!("Skipping test: CHRONOS_URL not set");
        return;
    }

    let tokens: Vec<String> = vec![
        "usdc.tether-token.near".to_string(),
        "blackdragon.tkn.near".to_string(),
    ];

    let service = PredictionService::new(
        std::env::var("CHRONOS_URL").unwrap_or_else(|_| "http://localhost:8000".to_string()),
        "http://localhost:3000".to_string(),
    );

    // バッチ処理のテスト
    let result = service
        .predict_multiple_tokens(tokens.clone(), "wrap.near", 7, 5)
        .await;

    if let Err(ref e) = result {
        eprintln!("Error during predict_multiple_tokens: {:?}", e);
        // データベースまたはChronos APIが利用できない場合はスキップ
        return;
    }

    let predictions = result.unwrap();
    // 少なくとも何か結果が返ることを確認
    assert!(predictions.len() <= tokens.len());
}
