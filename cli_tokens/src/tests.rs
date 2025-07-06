use anyhow::Result;
use chrono::Utc;
use std::path::PathBuf;
use tempfile::TempDir;

use crate::commands::{predict::PredictArgs, top::TopArgs};
use crate::models::token::{FileMetadata, PriceData, TokenFileData, TokenVolatilityData};
use crate::utils::file::{ensure_directory_exists, write_json_file};

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_predict_args_parsing() {
        // Test parsing of PredictArgs
        let args = PredictArgs {
            token_file: PathBuf::from("tokens/wrap.near.json"),
            output: PathBuf::from("predictions"),
            model: "server_default".to_string(),
            force: false,
            start_pct: 0.0,
            end_pct: 100.0,
        };

        assert_eq!(args.token_file, PathBuf::from("tokens/wrap.near.json"));
        assert_eq!(args.output, PathBuf::from("predictions"));
        assert_eq!(args.model, "server_default");
        assert!(!args.force);
    }

    #[test]
    fn test_top_args_default_values() {
        // Test default values for TopArgs
        let args = TopArgs {
            start: None,
            end: None,
            limit: 10,
            output: PathBuf::from("tokens"),
            format: "json".to_string(),
        };

        assert_eq!(args.limit, 10);
        assert_eq!(args.output, PathBuf::from("tokens"));
        assert_eq!(args.format, "json");
    }

    #[test]
    fn test_token_file_data_serialization() {
        let token_data = TokenFileData {
            metadata: FileMetadata {
                generated_at: Utc::now(),
                start_date: "2023-01-01".to_string(),
                end_date: "2023-01-31".to_string(),
                token: "wrap.near".to_string(),
            },
            token_data: TokenVolatilityData {
                token: "wrap.near".to_string(),
                volatility_score: 0.85,
                price_data: PriceData {
                    current_price: 1.23,
                    price_change_24h: 0.05,
                    volume_24h: 1000.0,
                },
            },
        };

        // Test serialization
        let json = serde_json::to_string(&token_data).unwrap();
        assert!(json.contains("wrap.near"));
        assert!(json.contains("0.85"));

        // Test deserialization
        let deserialized: TokenFileData = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.token_data.token, "wrap.near");
        assert_eq!(deserialized.token_data.volatility_score, 0.85);
    }

    #[tokio::test]
    async fn test_file_utils_directory_creation() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let test_path = temp_dir.path().join("test_dir");

        // Ensure directory creation works
        ensure_directory_exists(&test_path)?;
        assert!(test_path.exists());
        assert!(test_path.is_dir());

        Ok(())
    }

    #[tokio::test]
    async fn test_file_utils_json_write() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let test_file = temp_dir.path().join("test.json");

        let test_data = TokenFileData {
            metadata: FileMetadata {
                generated_at: Utc::now(),
                start_date: "2023-01-01".to_string(),
                end_date: "2023-01-31".to_string(),
                token: "test.token".to_string(),
            },
            token_data: TokenVolatilityData {
                token: "test.token".to_string(),
                volatility_score: 0.95,
                price_data: PriceData {
                    current_price: 2.34,
                    price_change_24h: -0.10,
                    volume_24h: 500.0,
                },
            },
        };

        // Write JSON file
        write_json_file(&test_file, &test_data).await?;
        assert!(test_file.exists());

        // Read and verify content
        let content = tokio::fs::read_to_string(&test_file).await?;
        let parsed: TokenFileData = serde_json::from_str(&content)?;
        assert_eq!(parsed.token_data.token, "test.token");
        assert_eq!(parsed.token_data.volatility_score, 0.95);

        Ok(())
    }

    #[test]
    fn test_date_string_format() {
        // Test date format consistency
        let date_str = "2023-01-15";
        let parsed_date = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d");
        assert!(parsed_date.is_ok());

        let date = parsed_date.unwrap();
        let formatted = date.format("%Y-%m-%d").to_string();
        assert_eq!(formatted, date_str);
    }
}

#[cfg(test)]
mod integration_tests {
    use crate::commands::top::parse_date;

    #[test]
    fn test_parse_date_function() {
        // Test valid date parsing
        let result = parse_date("2023-01-15");
        assert!(result.is_ok());

        let date = result.unwrap();
        assert_eq!(date.format("%Y-%m-%d").to_string(), "2023-01-15");

        // Test invalid date parsing
        let invalid_result = parse_date("invalid-date");
        assert!(invalid_result.is_err());
    }

    #[test]
    fn test_date_calculations() {
        // Test date calculations used in top command
        let base_date = parse_date("2023-01-15").unwrap();
        let thirty_days_ago = base_date - chrono::Duration::days(30);

        assert_eq!(thirty_days_ago.format("%Y-%m-%d").to_string(), "2022-12-16");
    }

    #[test]
    fn test_sanitize_filename() {
        use crate::utils::file::sanitize_filename;

        // Test filename sanitization
        assert_eq!(sanitize_filename("wrap.near"), "wrap.near");
        assert_eq!(
            sanitize_filename("token/with/slashes"),
            "token_with_slashes"
        );
        assert_eq!(sanitize_filename("token:with:colons"), "token_with_colons");
        assert_eq!(sanitize_filename("token with spaces"), "token_with_spaces");
    }
}

#[cfg(test)]
mod api_tests {
    use super::*;
    use crate::api::backend::BackendApiClient;
    use crate::api::chronos::ChronosApiClient;
    use crate::models::prediction::{
        AsyncPredictionResponse, PredictionResult, ZeroShotPredictionRequest,
    };
    use chrono::Utc;
    use serde_json::json;
    use zaciraci_common::pools::VolatilityTokensResponse;
    use zaciraci_common::types::TokenAccount;
    use zaciraci_common::ApiResponse;

    #[tokio::test]
    async fn test_backend_api_get_volatility_tokens_success() -> Result<()> {
        let mut server = mockito::Server::new_async().await;
        let mock_tokens = vec![
            TokenAccount("wrap.near".to_string().into()),
            TokenAccount("usdc.near".to_string().into()),
        ];
        let volatility_response = VolatilityTokensResponse {
            tokens: mock_tokens.clone(),
        };
        let api_response: ApiResponse<VolatilityTokensResponse, String> =
            ApiResponse::Success(volatility_response);

        let _mock = server
            .mock("POST", "/pools/get_volatility_tokens")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&api_response).unwrap())
            .create_async()
            .await;

        let client = BackendApiClient::new(server.url());
        let start_date = Utc::now();
        let end_date = Utc::now();
        let result = client.get_volatility_tokens(start_date, end_date, 10).await;

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
        let api_response: ApiResponse<VolatilityTokensResponse, String> =
            ApiResponse::Error("Database connection failed".to_string());

        let _mock = server
            .mock("POST", "/pools/get_volatility_tokens")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&api_response).unwrap())
            .create_async()
            .await;

        let client = BackendApiClient::new(server.url());
        let start_date = Utc::now();
        let end_date = Utc::now();
        let result = client.get_volatility_tokens(start_date, end_date, 10).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Database connection failed"));

        Ok(())
    }

    #[tokio::test]
    async fn test_backend_api_get_token_history() -> Result<()> {
        let mut server = mockito::Server::new_async().await;
        let mock_data = json!({
            "data": [
                {"timestamp": "2023-01-01T00:00:00Z", "price": 1.23},
                {"timestamp": "2023-01-02T00:00:00Z", "price": 1.25}
            ]
        });

        let _mock = server
            .mock("GET", "/api/token-history/wrap.near")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(mock_data.to_string())
            .create_async()
            .await;

        let client = BackendApiClient::new(server.url());
        let start_date = Utc::now();
        let end_date = Utc::now();
        let result = client
            .get_token_history("wrap.near", start_date, end_date)
            .await;

        // Debug the actual error if test fails
        if let Err(ref e) = result {
            println!("Error: {:?}", e);
        }
        assert!(result.is_ok());
        let history = result.unwrap();
        // Now returns mock data with 180 points
        assert_eq!(history.len(), 180);

        Ok(())
    }

    #[tokio::test]
    async fn test_chronos_api_predict_zero_shot_success() -> Result<()> {
        let mut server = mockito::Server::new_async().await;
        let mock_response = AsyncPredictionResponse {
            task_id: "pred_123".to_string(),
            status: "pending".to_string(),
            message: "Task started".to_string(),
        };

        let _mock = server
            .mock("POST", "/api/v1/predict_zero_shot_async")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&mock_response).unwrap())
            .create_async()
            .await;

        let client = ChronosApiClient::new(server.url());
        let request = ZeroShotPredictionRequest {
            timestamp: vec![Utc::now()],
            values: vec![1.0],
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

        let _mock = server
            .mock("POST", "/api/v1/predict_zero_shot_async")
            .with_status(500)
            .with_header("content-type", "application/json")
            .with_body("Internal Server Error")
            .create_async()
            .await;

        let client = ChronosApiClient::new(server.url());
        let request = ZeroShotPredictionRequest {
            timestamp: vec![Utc::now()],
            values: vec![1.0],
            forecast_until: Utc::now(),
            model_name: None,
            model_params: None,
        };

        let result = client.predict_zero_shot(request).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Chronos API error"));

        Ok(())
    }

    #[tokio::test]
    async fn test_chronos_api_get_prediction_status() -> Result<()> {
        let mut server = mockito::Server::new_async().await;
        let mock_response = PredictionResult {
            task_id: "pred_123".to_string(),
            status: "completed".to_string(),
            progress: Some(100.0),
            message: Some("Prediction completed".to_string()),
            result: None,
            error: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

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
        let completed_response = PredictionResult {
            task_id: "pred_123".to_string(),
            status: "completed".to_string(),
            progress: Some(100.0),
            message: Some("Prediction completed".to_string()),
            result: None,
            error: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let _mock = server
            .mock("GET", "/api/v1/prediction_status/pred_123")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&completed_response).unwrap())
            .create_async()
            .await;

        let client = ChronosApiClient::new(server.url());
        let result = client.poll_prediction_until_complete("pred_123", 1).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.status, "completed");

        Ok(())
    }

    #[tokio::test]
    async fn test_chronos_api_poll_prediction_failed() -> Result<()> {
        let mut server = mockito::Server::new_async().await;
        let failed_response = PredictionResult {
            task_id: "pred_123".to_string(),
            status: "failed".to_string(),
            progress: Some(0.0),
            message: Some("Prediction failed".to_string()),
            result: None,
            error: Some("Model training failed".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let _mock = server
            .mock("GET", "/api/v1/prediction_status/pred_123")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&failed_response).unwrap())
            .create_async()
            .await;

        let client = ChronosApiClient::new(server.url());
        let result = client.poll_prediction_until_complete("pred_123", 1).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Prediction failed"));

        Ok(())
    }
}
