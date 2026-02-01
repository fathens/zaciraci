//! 外部API（Backend）との連携テスト
//! - モックサーバーを使用したAPIレスポンステスト
//! - エラーハンドリング
//! - データ形式の互換性

use anyhow::Result;
use common::ApiResponse;
use common::types::{TokenAccount, TokenPrice};

use crate::api::backend::BackendClient;

#[tokio::test]
async fn test_backend_api_get_volatility_tokens_success() -> Result<()> {
    let mut server = mockito::Server::new_async().await;
    let mock_tokens = vec![
        "wrap.near".parse::<TokenAccount>().unwrap(),
        "usdc.near".parse::<TokenAccount>().unwrap(),
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
    let start_date = chrono::Utc::now();
    let end_date = chrono::Utc::now();
    let result = client
        .get_volatility_tokens(start_date, end_date, 10, None, None)
        .await;

    assert!(result.is_ok());
    let tokens = result.unwrap();
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].as_str(), "wrap.near");
    assert_eq!(tokens[1].as_str(), "usdc.near");

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
    let start_date = chrono::Utc::now();
    let end_date = chrono::Utc::now();
    let result = client
        .get_volatility_tokens(start_date, end_date, 10, None, None)
        .await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Database connection failed")
    );

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
            value: TokenPrice::from_near_per_token("5.23".parse().unwrap()),
        },
        common::stats::ValueAtTime {
            time: chrono::NaiveDate::from_ymd_opt(2025, 7, 6)
                .unwrap()
                .and_hms_opt(1, 0, 0)
                .unwrap(),
            value: TokenPrice::from_near_per_token("5.25".parse().unwrap()),
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
    assert_eq!(
        values[0].value,
        TokenPrice::from_near_per_token("5.23".parse().unwrap())
    );
    assert_eq!(
        values[1].value,
        TokenPrice::from_near_per_token("5.25".parse().unwrap())
    );

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
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Insufficient data points")
    );

    Ok(())
}
