use super::*;
use bigdecimal::BigDecimal;
use chrono::{DateTime, Duration, Utc};
use mockito::{self, Matcher};
use std::str::FromStr;
use zaciraci_common::prediction::{ChronosPredictionResponse, PredictionResult};

// モックレスポンスのヘルパー関数
fn create_mock_top_tokens() -> Vec<TopToken> {
    vec![
        TopToken {
            token: "token1.near".to_string(),
            volatility: BigDecimal::from_str("0.25").unwrap(),
            volume_24h: BigDecimal::from(1000000),
            current_price: BigDecimal::from_str("1.5").unwrap(),
        },
        TopToken {
            token: "token2.near".to_string(),
            volatility: BigDecimal::from_str("0.20").unwrap(),
            volume_24h: BigDecimal::from(500000),
            current_price: BigDecimal::from(2),
        },
    ]
}

fn create_mock_price_history() -> Vec<(i64, f64)> {
    let now = Utc::now().timestamp();
    vec![
        (now - 7200, 1.0),
        (now - 5400, 1.1),
        (now - 3600, 1.05),
        (now - 1800, 1.15),
        (now - 900, 1.12),
        (now, 1.18),
    ]
}

#[tokio::test]
async fn test_get_top_tokens() {
    let mut server = mockito::Server::new_async().await;
    let url = server.url();

    let expected_tokens = create_mock_top_tokens();
    let _m = server
        .mock("GET", "/api/volatility_tokens")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("limit".into(), "10".into()),
            Matcher::UrlEncoded("quote_token".into(), "wrap.near".into()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(serde_json::to_string(&expected_tokens).unwrap())
        .create_async()
        .await;

    let service = PredictionService::new(url.clone(), url.clone());

    let start_date = Utc::now() - Duration::days(7);
    let end_date = Utc::now();

    let result = service
        .get_top_tokens(start_date, end_date, 10, "wrap.near", None)
        .await;

    assert!(result.is_ok());
    let tokens = result.unwrap();
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].token, "token1.near");
    assert_eq!(tokens[1].token, "token2.near");
}

#[tokio::test]
async fn test_get_price_history() {
    let mut server = mockito::Server::new_async().await;
    let url = server.url();

    let mock_prices = create_mock_price_history();
    let _m = server
        .mock("GET", "/api/price_history/wrap.near/test.near")
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(serde_json::to_string(&mock_prices).unwrap())
        .create_async()
        .await;

    let service = PredictionService::new(url.clone(), url.clone());

    let start_date = Utc::now() - Duration::hours(2);
    let end_date = Utc::now();

    let result = service
        .get_price_history("test.near", "wrap.near", start_date, end_date)
        .await;

    // デバッグ用にエラーを出力
    if let Err(ref e) = result {
        eprintln!("Error in test_get_price_history: {:?}", e);
    }

    assert!(result.is_ok());
    let history = result.unwrap();
    assert_eq!(history.token, "test.near");
    assert_eq!(history.quote_token, "wrap.near");
    assert_eq!(history.prices.len(), 6);
    assert_eq!(history.prices[0].price, BigDecimal::from(1));
    assert_eq!(history.prices[3].price, BigDecimal::from(1));
    assert_eq!(history.prices[5].price, BigDecimal::from(1));
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
    let mut server = mockito::Server::new_async().await;
    let url = server.url();

    let _m = server
        .mock("GET", "/api/volatility_tokens")
        .with_status(500)
        .with_body("Internal Server Error")
        .create_async()
        .await;

    let service = PredictionService::new(url.clone(), url.clone());

    let start_date = Utc::now() - Duration::days(7);
    let end_date = Utc::now();

    let result = service
        .get_top_tokens(start_date, end_date, 10, "wrap.near", None)
        .await;

    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.to_string().contains("Failed to fetch top tokens"));
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
async fn test_predict_multiple_tokens_batch_processing() {
    let mut server = mockito::Server::new_async().await;
    let url = server.url();

    // 15個のトークンを作成してバッチ処理をテスト
    let tokens: Vec<String> = (1..=15).map(|i| format!("token{}.near", i)).collect();

    // 各トークンの価格履歴モックを作成
    for token in &tokens {
        let mock_prices = create_mock_price_history();
        let _m = server
            .mock(
                "GET",
                format!("/api/price_history/wrap.near/{}", token).as_str(),
            )
            .match_query(Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&mock_prices).unwrap())
            .create_async()
            .await;
    }

    // Chronosの予測レスポンスモックを作成
    let mock_task_id = "test-task-id";

    // 各トークンに対して予測モックを作成
    for i in 0..tokens.len() {
        let task_id = format!("{}-{}", mock_task_id, i);
        let async_resp = zaciraci_common::prediction::AsyncPredictionResponse {
            task_id: task_id.clone(),
            status: "pending".to_string(),
            message: "Processing".to_string(),
        };

        let _m_predict = server
            .mock("POST", "/api/v1/predict_zero_shot_async")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&async_resp).unwrap())
            .create_async()
            .await;

        // 予測結果のモック
        let now = Utc::now();
        let forecast_timestamps: Vec<DateTime<Utc>> =
            (1..=5).map(|h| now + Duration::hours(h)).collect();

        let mock_result = PredictionResult {
            task_id: task_id.clone(),
            status: "completed".to_string(),
            progress: Some(BigDecimal::from(100)),
            message: Some("Completed".to_string()),
            result: Some(ChronosPredictionResponse {
                forecast_timestamp: forecast_timestamps,
                forecast_values: vec![
                    "1.2".parse().unwrap(),
                    "1.3".parse().unwrap(),
                    "1.4".parse().unwrap(),
                    "1.5".parse().unwrap(),
                    "1.6".parse().unwrap(),
                ],
                model_name: "chronos-small".to_string(),
                confidence_intervals: None,
                metrics: None,
            }),
            error: None,
            created_at: now,
            updated_at: now,
        };

        let _m_result = server
            .mock(
                "GET",
                format!("/api/v1/prediction_status/{}", task_id).as_str(),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&mock_result).unwrap())
            .create_async()
            .await;
    }

    let service = PredictionService::new(url.clone(), url.clone());

    // 15個のトークンを10個ずつのバッチで処理
    let result = service
        .predict_multiple_tokens(tokens.clone(), "wrap.near", 7, 5)
        .await;

    if let Err(ref e) = result {
        eprintln!("Error during predict_multiple_tokens: {:?}", e);
    }
    assert!(result.is_ok());
    let predictions = result.unwrap();

    // 全15個のトークンが処理されたことを確認
    assert_eq!(predictions.len(), 15);

    // 各トークンの予測結果を確認
    for token in &tokens {
        assert!(predictions.contains_key(token));
        let prediction = predictions.get(token).unwrap();
        assert_eq!(prediction.token, *token);
        assert_eq!(prediction.quote_token, "wrap.near");
        assert_eq!(prediction.predictions.len(), 5);
    }
}
