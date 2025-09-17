//! 外部API（Backend、Chronos）との連携テスト
//! - モックサーバーを使用したAPIレスポンステスト
//! - エラーハンドリング
//! - データ形式の互換性

use anyhow::Result;
use bigdecimal::BigDecimal;
use chrono::Utc;
use common::api::chronos::ChronosApiClient;
use common::types::TokenAccount;
use common::ApiResponse;

use crate::api::backend::BackendClient;

#[tokio::test]
async fn test_backend_api_get_volatility_tokens_success() -> Result<()> {
    let mut server = mockito::Server::new_async().await;
    let mock_tokens = vec![
        TokenAccount("wrap.near".to_string().into()),
        TokenAccount("usdc.near".to_string().into()),
    ];
    let volatility_response = common::pools::VolatilityTokensResponse {
        tokens: mock_tokens.clone(),
    };
    let api_response: ApiResponse<common::pools::VolatilityTokensResponse, String> =
        ApiResponse::Success(volatility_response);

    let _mock = server
        .mock("POST", "/pools/get_volatility_tokens")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(serde_json::to_string(&api_response).unwrap())
        .create_async()
        .await;

    let client = BackendClient::new_with_url(server.url());
    let start_date = Utc::now();
    let end_date = Utc::now();
    let result = client
        .get_volatility_tokens(start_date, end_date, 10, None, None)
        .await;

    assert!(result.is_ok());
    let tokens = result.unwrap();
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].0, "wrap.near".into());
    assert_eq!(tokens[1].0, "usdc.near".into());

    Ok(())
}

#[tokio::test]
async fn test_backend_api_get_volatility_tokens_error() -> Result<()> {
    let mut server = mockito::Server::new_async().await;
    let api_response: ApiResponse<common::pools::VolatilityTokensResponse, String> =
        ApiResponse::Error("Database connection failed".to_string());

    let _mock = server
        .mock("POST", "/pools/get_volatility_tokens")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(serde_json::to_string(&api_response).unwrap())
        .create_async()
        .await;

    let client = BackendClient::new_with_url(server.url());
    let start_date = Utc::now();
    let end_date = Utc::now();
    let result = client
        .get_volatility_tokens(start_date, end_date, 10, None, None)
        .await;

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Database connection failed"));

    Ok(())
}

#[tokio::test]
async fn test_chronos_api_predict_zero_shot_success() -> Result<()> {
    let mut server = mockito::Server::new_async().await;
    let mock_response = common::prediction::AsyncPredictionResponse {
        task_id: "pred_123".to_string(),
        status: "pending".to_string(),
        message: "Task started".to_string(),
    };

    // 実際のAPIは直接レスポンスを返す（ApiResponseラッパーなし）
    let _mock = server
        .mock("POST", "/api/v1/predict_zero_shot_async")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(serde_json::to_string(&mock_response).unwrap())
        .create_async()
        .await;

    let client = ChronosApiClient::new(server.url());
    let request = common::prediction::ZeroShotPredictionRequest {
        timestamp: vec![Utc::now()],
        values: vec![BigDecimal::from(1)],
        forecast_until: Utc::now(),
        model_name: None,
        model_params: None,
    };

    let result = client.predict_zero_shot(request).await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.task_id, "pred_123");
    assert_eq!(response.status, "pending");
    assert_eq!(response.message, "Task started");

    Ok(())
}

#[tokio::test]
async fn test_chronos_api_predict_zero_shot_error() -> Result<()> {
    let mut server = mockito::Server::new_async().await;

    // エラーの場合はHTTPステータスコードでエラーを返す
    let _mock = server
        .mock("POST", "/api/v1/predict_zero_shot_async")
        .with_status(500)
        .with_header("content-type", "application/json")
        .with_body("Internal Server Error")
        .create_async()
        .await;

    let client = ChronosApiClient::new(server.url());
    let request = common::prediction::ZeroShotPredictionRequest {
        timestamp: vec![Utc::now()],
        values: vec![BigDecimal::from(1)],
        forecast_until: Utc::now(),
        model_name: None,
        model_params: None,
    };

    let result = client.predict_zero_shot(request).await;

    assert!(result.is_err());
    let error = result.unwrap_err().to_string();
    assert!(error.contains("500") || error.contains("HTTP Error"));

    Ok(())
}

#[tokio::test]
async fn test_chronos_api_get_prediction_status() -> Result<()> {
    let mut server = mockito::Server::new_async().await;
    let mock_response = common::prediction::PredictionResult {
        task_id: "pred_123".to_string(),
        status: "completed".to_string(),
        progress: Some(BigDecimal::from(100)),
        message: Some("Prediction completed".to_string()),
        result: None,
        error: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    // 実際のAPIは直接レスポンスを返す（ApiResponseラッパーなし）
    let _mock = server
        .mock("GET", "/api/v1/prediction_status/pred_123")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(serde_json::to_string(&mock_response).unwrap())
        .create_async()
        .await;

    let client = ChronosApiClient::new(server.url());
    let result = client.get_prediction_status("pred_123").await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.task_id, "pred_123");
    assert_eq!(response.status, "completed");
    assert!(response.progress.is_some());

    Ok(())
}

#[tokio::test]
async fn test_chronos_api_poll_prediction_until_complete() -> Result<()> {
    let mut server = mockito::Server::new_async().await;
    let completed_response = common::prediction::PredictionResult {
        task_id: "pred_123".to_string(),
        status: "completed".to_string(),
        progress: Some(BigDecimal::from(100)),
        message: Some("Prediction completed".to_string()),
        result: None,
        error: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    // 実際のAPIは直接レスポンスを返す（ApiResponseラッパーなし）
    let _mock = server
        .mock("GET", "/api/v1/prediction_status/pred_123")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(serde_json::to_string(&completed_response).unwrap())
        .create_async()
        .await;

    let client = ChronosApiClient::new(server.url());
    let result = client.poll_prediction_until_complete("pred_123").await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.status, "completed");

    Ok(())
}

#[tokio::test]
async fn test_chronos_api_poll_prediction_failed() -> Result<()> {
    let mut server = mockito::Server::new_async().await;
    let failed_response = common::prediction::PredictionResult {
        task_id: "pred_123".to_string(),
        status: "failed".to_string(),
        progress: Some(BigDecimal::from(0)),
        message: Some("Prediction failed".to_string()),
        result: None,
        error: Some("Model training failed".to_string()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    // 実際のAPIは直接レスポンスを返す（ApiResponseラッパーなし）
    let _mock = server
        .mock("GET", "/api/v1/prediction_status/pred_123")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(serde_json::to_string(&failed_response).unwrap())
        .create_async()
        .await;

    let client = ChronosApiClient::new(server.url());
    let result = client.poll_prediction_until_complete("pred_123").await;

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Prediction failed"));

    Ok(())
}

#[tokio::test]
async fn test_backend_api_get_price_history_success() -> Result<()> {
    let mut server = mockito::Server::new_async().await;
    let mock_values = vec![
        common::stats::ValueAtTime {
            time: chrono::NaiveDate::from_ymd_opt(2025, 7, 6)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            value: "5.23".parse().unwrap(),
        },
        common::stats::ValueAtTime {
            time: chrono::NaiveDate::from_ymd_opt(2025, 7, 6)
                .unwrap()
                .and_hms_opt(1, 0, 0)
                .unwrap(),
            value: "5.25".parse().unwrap(),
        },
    ];
    let price_response = common::stats::GetValuesResponse {
        values: mock_values.clone(),
    };
    let api_response: ApiResponse<common::stats::GetValuesResponse, String> =
        ApiResponse::Success(price_response);

    let _mock = server
        .mock("POST", "/stats/get_values")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(serde_json::to_string(&api_response).unwrap())
        .create_async()
        .await;

    let client = BackendClient::new_with_url(server.url());
    let start_date = chrono::NaiveDate::from_ymd_opt(2025, 7, 6)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let end_date = chrono::NaiveDate::from_ymd_opt(2025, 7, 7)
        .unwrap()
        .and_hms_opt(23, 59, 59)
        .unwrap();

    let result = client
        .get_price_history("wrap.near", "wrap.near", start_date, end_date)
        .await;

    assert!(result.is_ok());
    let values = result.unwrap();
    assert_eq!(values.len(), 2);
    assert_eq!(values[0].value, "5.23".parse::<BigDecimal>().unwrap());
    assert_eq!(values[1].value, "5.25".parse::<BigDecimal>().unwrap());

    Ok(())
}

#[tokio::test]
async fn test_backend_api_get_price_history_error() -> Result<()> {
    let mut server = mockito::Server::new_async().await;
    let api_response: ApiResponse<common::stats::GetValuesResponse, String> =
        ApiResponse::Error("Insufficient data points".to_string());

    let _mock = server
        .mock("POST", "/stats/get_values")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(serde_json::to_string(&api_response).unwrap())
        .create_async()
        .await;

    let client = BackendClient::new_with_url(server.url());
    let start_date = chrono::NaiveDate::from_ymd_opt(2025, 7, 6)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let end_date = chrono::NaiveDate::from_ymd_opt(2025, 7, 7)
        .unwrap()
        .and_hms_opt(23, 59, 59)
        .unwrap();

    let result = client
        .get_price_history("wrap.near", "wrap.near", start_date, end_date)
        .await;

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Insufficient data points"));

    Ok(())
}
