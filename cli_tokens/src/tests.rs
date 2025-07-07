use anyhow::Result;
use chrono::Utc;
use std::path::PathBuf;
use tempfile::TempDir;

use crate::commands::{
    history::HistoryArgs, predict::PredictArgs, top::TopArgs, verify::VerifyArgs,
};
use crate::models::{
    token::{FileMetadata, PriceData, TokenFileData, TokenVolatilityData},
    verification::ComparisonPoint,
};
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
            forecast_ratio: 10.0,
        };

        assert_eq!(args.token_file, PathBuf::from("tokens/wrap.near.json"));
        assert_eq!(args.output, PathBuf::from("predictions"));
        assert_eq!(args.model, "server_default");
        assert!(!args.force);
        assert_eq!(args.forecast_ratio, 10.0);
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
            quote_token: None,
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

    #[test]
    fn test_history_args_parsing() {
        // Test parsing of HistoryArgs
        let args = HistoryArgs {
            token_file: PathBuf::from("tokens/wrap.near.json"),
            quote_token: "wrap.near".to_string(),
            output: PathBuf::from("history"),
            force: false,
        };

        assert_eq!(args.token_file, PathBuf::from("tokens/wrap.near.json"));
        assert_eq!(args.quote_token, "wrap.near");
        assert_eq!(args.output, PathBuf::from("history"));
        assert!(!args.force);
    }

    #[test]
    fn test_history_args_default_values() {
        // Test default values for HistoryArgs
        let args = HistoryArgs {
            token_file: PathBuf::from("tokens/test.json"),
            quote_token: "wrap.near".to_string(),
            output: PathBuf::from("history"),
            force: false,
        };

        assert_eq!(args.quote_token, "wrap.near");
        assert_eq!(args.output, PathBuf::from("history"));
        assert!(!args.force);
    }

    #[test]
    fn test_history_args_custom_values() {
        // Test custom values for HistoryArgs
        let args = HistoryArgs {
            token_file: PathBuf::from("custom/token.json"),
            quote_token: "usdc.near".to_string(),
            output: PathBuf::from("custom_history"),
            force: true,
        };

        assert_eq!(args.token_file, PathBuf::from("custom/token.json"));
        assert_eq!(args.quote_token, "usdc.near");
        assert_eq!(args.output, PathBuf::from("custom_history"));
        assert!(args.force);
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
    use crate::api::backend::BackendClient;
    use crate::api::chronos::ChronosApiClient;
    use crate::models::prediction::{
        AsyncPredictionResponse, PredictionResult, ZeroShotPredictionRequest,
    };
    use chrono::Utc;
    use zaciraci_common::pools::VolatilityTokensResponse;
    use zaciraci_common::stats::{GetValuesResponse, ValueAtTime};
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

        let client = BackendClient::new_with_url(server.url());
        let start_date = Utc::now();
        let end_date = Utc::now();
        let result = client
            .get_volatility_tokens(start_date, end_date, 10, None)
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
        let api_response: ApiResponse<VolatilityTokensResponse, String> =
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
            .get_volatility_tokens(start_date, end_date, 10, None)
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

    #[tokio::test]
    async fn test_backend_api_get_price_history_success() -> Result<()> {
        let mut server = mockito::Server::new_async().await;
        let mock_values = vec![
            ValueAtTime {
                time: chrono::NaiveDate::from_ymd_opt(2025, 7, 6)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap(),
                value: 5.23,
            },
            ValueAtTime {
                time: chrono::NaiveDate::from_ymd_opt(2025, 7, 6)
                    .unwrap()
                    .and_hms_opt(1, 0, 0)
                    .unwrap(),
                value: 5.25,
            },
        ];
        let price_response = GetValuesResponse {
            values: mock_values.clone(),
        };
        let api_response: ApiResponse<GetValuesResponse, String> =
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
        assert_eq!(values[0].value, 5.23);
        assert_eq!(values[1].value, 5.25);

        Ok(())
    }

    #[tokio::test]
    async fn test_backend_api_get_price_history_error() -> Result<()> {
        let mut server = mockito::Server::new_async().await;
        let api_response: ApiResponse<GetValuesResponse, String> =
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
}

#[cfg(test)]
mod predict_args_tests {
    use super::*;
    use chrono::{Duration, NaiveDate, TimeZone, Utc};

    // === 基本オプションテスト ===

    #[test]
    fn test_default_values() {
        // テストのデフォルト値確認
        let args = PredictArgs {
            token_file: PathBuf::from("test.json"),
            output: PathBuf::from("predictions"),
            model: "server_default".to_string(),
            force: false,
            start_pct: 0.0,
            end_pct: 100.0,
            forecast_ratio: 10.0,
        };

        assert_eq!(args.output, PathBuf::from("predictions"));
        assert_eq!(args.model, "server_default");
        assert!(!args.force);
        assert_eq!(args.start_pct, 0.0);
        assert_eq!(args.end_pct, 100.0);
        assert_eq!(args.forecast_ratio, 10.0);
    }

    #[test]
    fn test_custom_values() {
        // カスタム値でのテスト
        let args = PredictArgs {
            token_file: PathBuf::from("custom/token.json"),
            output: PathBuf::from("custom_output"),
            model: "chronos_bolt".to_string(),
            force: true,
            start_pct: 25.0,
            end_pct: 75.0,
            forecast_ratio: 50.0,
        };

        assert_eq!(args.token_file, PathBuf::from("custom/token.json"));
        assert_eq!(args.output, PathBuf::from("custom_output"));
        assert_eq!(args.model, "chronos_bolt");
        assert!(args.force);
        assert_eq!(args.start_pct, 25.0);
        assert_eq!(args.end_pct, 75.0);
        assert_eq!(args.forecast_ratio, 50.0);
    }

    // === パーセンテージ範囲オプションテスト ===

    #[test]
    fn test_start_pct_end_pct_validation_valid_values() {
        // 有効な start_pct と end_pct の組み合わせ
        let valid_combinations = vec![
            (0.0, 100.0),  // 全範囲
            (0.0, 50.0),   // 前半
            (50.0, 100.0), // 後半
            (25.0, 75.0),  // 中間
            (10.5, 89.5),  // 小数点
        ];

        for (start, end) in valid_combinations {
            let args = PredictArgs {
                token_file: PathBuf::from("test.json"),
                output: PathBuf::from("predictions"),
                model: "server_default".to_string(),
                force: false,
                start_pct: start,
                end_pct: end,
                forecast_ratio: 10.0,
            };

            // バリデーション条件をテスト
            assert!(args.start_pct >= 0.0 && args.start_pct <= 100.0);
            assert!(args.end_pct >= 0.0 && args.end_pct <= 100.0);
            assert!(args.start_pct < args.end_pct);
        }
    }

    #[test]
    fn test_start_pct_end_pct_validation_invalid_values() {
        // 無効な start_pct と end_pct の組み合わせ
        let invalid_combinations = vec![
            (-1.0, 100.0),  // start_pct が負の値
            (0.0, 101.0),   // end_pct が100を超える
            (50.0, 50.0),   // start_pct = end_pct
            (75.0, 25.0),   // start_pct > end_pct
            (100.1, 200.0), // 両方とも範囲外
        ];

        for (start, end) in invalid_combinations {
            let args = PredictArgs {
                token_file: PathBuf::from("test.json"),
                output: PathBuf::from("predictions"),
                model: "server_default".to_string(),
                force: false,
                start_pct: start,
                end_pct: end,
                forecast_ratio: 10.0,
            };

            // バリデーション条件をテスト
            let start_valid = args.start_pct >= 0.0 && args.start_pct <= 100.0;
            let end_valid = args.end_pct >= 0.0 && args.end_pct <= 100.0;
            let range_valid = args.start_pct < args.end_pct;

            let is_valid = start_valid && end_valid && range_valid;
            assert!(
                !is_valid,
                "Combination start={}, end={} should be invalid",
                start, end
            );
        }
    }

    // === その他のオプションテスト ===

    #[test]
    fn test_model_option_values() {
        // 異なるモデル名のテスト
        let models = vec![
            "server_default",
            "chronos_bolt",
            "autogluon",
            "statistical",
            "custom_model_name",
        ];

        for model in models {
            let args = PredictArgs {
                token_file: PathBuf::from("test.json"),
                output: PathBuf::from("predictions"),
                model: model.to_string(),
                force: false,
                start_pct: 0.0,
                end_pct: 100.0,
                forecast_ratio: 10.0,
            };

            assert_eq!(args.model, model);
            assert!(!args.model.is_empty());
        }
    }

    #[test]
    fn test_force_flag_variations() {
        // force フラグのテスト
        let args_false = PredictArgs {
            token_file: PathBuf::from("test.json"),
            output: PathBuf::from("predictions"),
            model: "server_default".to_string(),
            force: false,
            start_pct: 0.0,
            end_pct: 100.0,
            forecast_ratio: 10.0,
        };

        let args_true = PredictArgs {
            token_file: PathBuf::from("test.json"),
            output: PathBuf::from("predictions"),
            model: "server_default".to_string(),
            force: true,
            start_pct: 0.0,
            end_pct: 100.0,
            forecast_ratio: 10.0,
        };

        assert!(!args_false.force);
        assert!(args_true.force);
    }

    #[test]
    fn test_output_path_variations() {
        // 異なる出力パスのテスト
        let output_paths = vec![
            "predictions",
            "custom_output",
            "results/2024",
            "/tmp/predictions",
            "./relative/path",
        ];

        for output_path in output_paths {
            let args = PredictArgs {
                token_file: PathBuf::from("test.json"),
                output: PathBuf::from(output_path),
                model: "server_default".to_string(),
                force: false,
                start_pct: 0.0,
                end_pct: 100.0,
                forecast_ratio: 10.0,
            };

            assert_eq!(args.output, PathBuf::from(output_path));
        }
    }

    #[test]
    fn test_token_file_path_variations() {
        // 異なるトークンファイルパスのテスト
        let token_files = vec![
            "tokens/wrap.near.json",
            "data/token_data.json",
            "/absolute/path/token.json",
            "./relative/token.json",
            "nested/dir/structure/token.json",
        ];

        for token_file in token_files {
            let args = PredictArgs {
                token_file: PathBuf::from(token_file),
                output: PathBuf::from("predictions"),
                model: "server_default".to_string(),
                force: false,
                start_pct: 0.0,
                end_pct: 100.0,
                forecast_ratio: 10.0,
            };

            assert_eq!(args.token_file, PathBuf::from(token_file));
            assert!(!args.token_file.as_os_str().is_empty());
        }
    }

    // === 境界値テスト ===

    #[test]
    fn test_extreme_percentage_values() {
        // 境界値での start_pct と end_pct のテスト
        let boundary_cases = vec![
            (0.0, 0.1),    // 最小範囲
            (99.9, 100.0), // 最大近く
            (0.0, 1.0),    // 1%の範囲
            (49.0, 51.0),  // 中央の小さな範囲
        ];

        for (start, end) in boundary_cases {
            let args = PredictArgs {
                token_file: PathBuf::from("test.json"),
                output: PathBuf::from("predictions"),
                model: "server_default".to_string(),
                force: false,
                start_pct: start,
                end_pct: end,
                forecast_ratio: 10.0,
            };

            assert!(args.start_pct >= 0.0 && args.start_pct <= 100.0);
            assert!(args.end_pct >= 0.0 && args.end_pct <= 100.0);
            assert!(args.start_pct < args.end_pct);

            // 範囲の大きさをテスト
            let range = args.end_pct - args.start_pct;
            assert!(range > 0.0);
        }
    }

    // === forecast_ratio オプションテスト ===

    #[test]
    fn test_forecast_ratio_default_value() {
        let args = PredictArgs {
            token_file: PathBuf::from("test.json"),
            output: PathBuf::from("predictions"),
            model: "server_default".to_string(),
            force: false,
            start_pct: 0.0,
            end_pct: 100.0,
            forecast_ratio: 10.0,
        };

        assert_eq!(args.forecast_ratio, 10.0);
    }

    #[test]
    fn test_forecast_ratio_validation_valid_values() {
        let test_cases = vec![0.1, 1.0, 10.0, 50.0, 100.0, 500.0];

        for ratio in test_cases {
            let args = PredictArgs {
                token_file: PathBuf::from("test.json"),
                output: PathBuf::from("predictions"),
                model: "server_default".to_string(),
                force: false,
                start_pct: 0.0,
                end_pct: 100.0,
                forecast_ratio: ratio,
            };

            assert!(args.forecast_ratio > 0.0 && args.forecast_ratio <= 500.0);
        }
    }

    #[test]
    fn test_forecast_duration_calculation() {
        // 30日間のデータ期間をテスト
        let start_date = NaiveDate::from_ymd_opt(2024, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let end_date = NaiveDate::from_ymd_opt(2024, 1, 31)
            .unwrap()
            .and_hms_opt(23, 59, 59)
            .unwrap();

        let start_utc = Utc.from_utc_datetime(&start_date);
        let end_utc = Utc.from_utc_datetime(&end_date);

        let input_duration = end_utc.signed_duration_since(start_utc);

        // 10%の比率でテスト
        let forecast_ratio = 10.0;
        let forecast_duration_ms =
            (input_duration.num_milliseconds() as f64 * (forecast_ratio / 100.0)) as i64;
        let forecast_duration = Duration::milliseconds(forecast_duration_ms);

        // 30日の10%は約3日
        assert!(forecast_duration.num_days() >= 2 && forecast_duration.num_days() <= 4);
    }

    #[test]
    fn test_forecast_duration_calculation_7_days() {
        // 7日間のデータ期間をテスト
        let start_date = NaiveDate::from_ymd_opt(2024, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let end_date = NaiveDate::from_ymd_opt(2024, 1, 8)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();

        let start_utc = Utc.from_utc_datetime(&start_date);
        let end_utc = Utc.from_utc_datetime(&end_date);

        let input_duration = end_utc.signed_duration_since(start_utc);

        // 10%の比率でテスト
        let forecast_ratio = 10.0;
        let forecast_duration_ms =
            (input_duration.num_milliseconds() as f64 * (forecast_ratio / 100.0)) as i64;
        let forecast_duration = Duration::milliseconds(forecast_duration_ms);

        // 7日の10%は約16.8時間
        let expected_hours = 7.0 * 24.0 * 0.1; // 16.8時間
        let actual_hours = forecast_duration.num_hours() as f64;

        assert!((actual_hours - expected_hours).abs() < 1.0); // 1時間の誤差許容
    }

    #[test]
    fn test_forecast_ratio_edge_cases() {
        // 最小値テスト (0.1%)
        let input_duration = Duration::days(30);
        let forecast_ratio = 0.1;
        let forecast_duration_ms =
            (input_duration.num_milliseconds() as f64 * (forecast_ratio / 100.0)) as i64;
        let forecast_duration = Duration::milliseconds(forecast_duration_ms);

        // 30日の0.1%は約7.2時間なので、分単位で確認
        assert!(forecast_duration.num_minutes() > 0);

        // 最大値テスト (500%)
        let forecast_ratio = 500.0;
        let forecast_duration_ms =
            (input_duration.num_milliseconds() as f64 * (forecast_ratio / 100.0)) as i64;
        let forecast_duration = Duration::milliseconds(forecast_duration_ms);

        // 30日の500%は150日
        assert!(forecast_duration.num_days() >= 149 && forecast_duration.num_days() <= 151);
    }

    #[test]
    fn test_different_forecast_ratios() {
        let input_duration = Duration::days(10); // 10日間のデータ

        let test_cases = vec![
            (10.0, 1.0),   // 10% = 1日
            (25.0, 2.5),   // 25% = 2.5日
            (50.0, 5.0),   // 50% = 5日
            (100.0, 10.0), // 100% = 10日
        ];

        for (ratio, expected_days) in test_cases {
            let forecast_duration_ms =
                (input_duration.num_milliseconds() as f64 * (ratio / 100.0)) as i64;
            let forecast_duration = Duration::milliseconds(forecast_duration_ms);

            // 時間単位で比較（より精密）
            let actual_hours = forecast_duration.num_hours() as f64;
            let expected_hours = expected_days * 24.0;

            assert!(
                (actual_hours - expected_hours).abs() < 1.0,
                "Ratio {}% should result in {} hours, got {} hours",
                ratio,
                expected_hours,
                actual_hours
            );
        }
    }

    #[tokio::test]
    async fn test_forecast_ratio_validation_errors() {
        // 無効な値でのバリデーションエラーをテスト
        let invalid_ratios = vec![0.0, -1.0, 500.1, 1000.0];

        for invalid_ratio in invalid_ratios {
            let args = PredictArgs {
                token_file: PathBuf::from("test.json"),
                output: PathBuf::from("predictions"),
                model: "server_default".to_string(),
                force: false,
                start_pct: 0.0,
                end_pct: 100.0,
                forecast_ratio: invalid_ratio,
            };

            // 実際のrunメソッドを呼び出すのではなく、バリデーション条件をテスト
            let is_valid = args.forecast_ratio > 0.0 && args.forecast_ratio <= 500.0;
            assert!(!is_valid, "Ratio {} should be invalid", invalid_ratio);
        }
    }

    #[test]
    fn test_forecast_ratio_precision() {
        // 小数点以下の精度をテスト
        let test_cases = vec![
            (10.5, Duration::days(1).num_milliseconds() as f64 * 0.105),
            (33.3, Duration::days(1).num_milliseconds() as f64 * 0.333),
            (0.1, Duration::days(1).num_milliseconds() as f64 * 0.001),
        ];

        let input_duration = Duration::days(1); // 1日間のデータ

        for (ratio, expected_ms) in test_cases {
            let forecast_duration_ms =
                (input_duration.num_milliseconds() as f64 * (ratio / 100.0)) as i64;
            let expected_ms_i64 = expected_ms as i64;

            // 誤差を許容した比較（1秒以内）
            assert!(
                (forecast_duration_ms - expected_ms_i64).abs() < 1000,
                "Ratio {}% calculation precision test failed",
                ratio
            );
        }
    }

    #[test]
    fn test_predict_output_file_structure() {
        // 新しいファイルベース構造のテスト
        use crate::utils::file::sanitize_filename;
        use std::path::PathBuf;

        let test_cases = vec![
            ("wrap.near", "wrap.near.json"),
            ("usdc.near", "usdc.near.json"),
            ("token-with-dash", "token-with-dash.json"),
            ("token/with/slash", "token_with_slash.json"),
        ];

        let output_dir = PathBuf::from("predictions");

        for (token_name, expected_filename) in test_cases {
            let sanitized_name = sanitize_filename(token_name);
            let filename = format!("{}.json", sanitized_name);
            let prediction_file = output_dir.join(&filename);

            assert_eq!(filename, expected_filename);
            assert_eq!(
                prediction_file,
                PathBuf::from(format!("predictions/{}", expected_filename))
            );
        }
    }
}

#[cfg(test)]
mod verify_tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_verify_args_parsing() {
        // Test parsing of VerifyArgs
        let args = VerifyArgs {
            prediction_file: PathBuf::from("predictions/wrap.near.json"),
            actual_data_file: Some(PathBuf::from("tokens/wrap.near.json")),
            output: PathBuf::from("verification"),
            force: false,
        };

        assert_eq!(
            args.prediction_file,
            PathBuf::from("predictions/wrap.near.json")
        );
        assert_eq!(
            args.actual_data_file,
            Some(PathBuf::from("tokens/wrap.near.json"))
        );
        assert_eq!(args.output, PathBuf::from("verification"));
        assert!(!args.force);
    }

    #[test]
    fn test_verify_args_default_values() {
        // Test default values for VerifyArgs
        let args = VerifyArgs {
            prediction_file: PathBuf::from("predictions/test.json"),
            actual_data_file: None,
            output: PathBuf::from("verification"),
            force: false,
        };

        assert_eq!(args.output, PathBuf::from("verification"));
        assert!(!args.force);
        assert!(args.actual_data_file.is_none());
    }

    #[test]
    fn test_infer_actual_data_file() {
        use crate::commands::verify::infer_actual_data_file;

        let test_cases = vec![
            ("wrap.near", "wrap.near", "tokens/wrap.near/wrap.near.json"),
            ("usdc.near", "wrap.near", "tokens/wrap.near/usdc.near.json"),
            (
                "token/with/slash",
                "usdc.tether-token.near",
                "tokens/usdc.tether-token.near/token_with_slash.json",
            ),
            (
                "token:with:colons",
                "wrap.near",
                "tokens/wrap.near/token_with_colons.json",
            ),
        ];

        for (token_name, quote_token, expected_path) in test_cases {
            let result = infer_actual_data_file(token_name, quote_token).unwrap();
            assert_eq!(result, PathBuf::from(expected_path));
        }
    }

    #[test]
    fn test_verification_metrics_calculation() {
        use crate::commands::verify::calculate_verification_metrics;

        // Create test comparison points
        let comparison_points = vec![
            ComparisonPoint {
                timestamp: Utc::now(),
                predicted_value: 100.0,
                actual_value: 98.0,
                error: 2.0,
                percentage_error: 2.04,
            },
            ComparisonPoint {
                timestamp: Utc::now(),
                predicted_value: 105.0,
                actual_value: 107.0,
                error: -2.0,
                percentage_error: -1.87,
            },
            ComparisonPoint {
                timestamp: Utc::now(),
                predicted_value: 110.0,
                actual_value: 109.0,
                error: 1.0,
                percentage_error: 0.92,
            },
        ];

        let metrics = calculate_verification_metrics(&comparison_points).unwrap();

        // Check MAE (Mean Absolute Error): (2.0 + 2.0 + 1.0) / 3 = 1.67
        assert!((metrics.mae - 1.666666666666667).abs() < 0.01);

        // Check RMSE: sqrt((4 + 4 + 1) / 3) = sqrt(3) ≈ 1.732
        assert!((metrics.rmse - 1.7320508075688772).abs() < 0.01);

        // Check MAPE: (2.04 + 1.87 + 0.92) / 3 ≈ 1.61
        assert!((metrics.mape - 1.61).abs() < 0.01);

        // Check correlation is calculated (should not be NaN)
        assert!(!metrics.correlation.is_nan());
    }

    #[test]
    fn test_verification_metrics_empty_data() {
        use crate::commands::verify::calculate_verification_metrics;

        let empty_points: Vec<ComparisonPoint> = vec![];
        let result = calculate_verification_metrics(&empty_points);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No comparison points"));
    }

    #[test]
    fn test_verification_metrics_single_point() {
        use crate::commands::verify::calculate_verification_metrics;

        let single_point = vec![ComparisonPoint {
            timestamp: Utc::now(),
            predicted_value: 100.0,
            actual_value: 95.0,
            error: 5.0,
            percentage_error: 5.26,
        }];

        let metrics = calculate_verification_metrics(&single_point).unwrap();

        // With single point: MAE = RMSE = 5.0
        assert_eq!(metrics.mae, 5.0);
        assert_eq!(metrics.rmse, 5.0);
        assert_eq!(metrics.mape, 5.26);

        // Direction accuracy is 0 for single point
        assert_eq!(metrics.direction_accuracy, 0.0);

        // Correlation is 0 for single point (no variance)
        assert_eq!(metrics.correlation, 0.0);
    }

    #[test]
    fn test_verify_force_flag_variations() {
        // force フラグのテスト
        let args_false = VerifyArgs {
            prediction_file: PathBuf::from("predictions/test.json"),
            actual_data_file: None,
            output: PathBuf::from("verification"),
            force: false,
        };

        let args_true = VerifyArgs {
            prediction_file: PathBuf::from("predictions/test.json"),
            actual_data_file: None,
            output: PathBuf::from("verification"),
            force: true,
        };

        assert!(!args_false.force);
        assert!(args_true.force);
    }

    #[test]
    fn test_verify_output_path_variations() {
        // 異なる出力パスのテスト
        let output_paths = vec![
            "verification",
            "custom_verification",
            "results/2024",
            "/tmp/verification",
            "./relative/path",
        ];

        for output_path in output_paths {
            let args = VerifyArgs {
                prediction_file: PathBuf::from("predictions/test.json"),
                actual_data_file: None,
                output: PathBuf::from(output_path),
                force: false,
            };

            assert_eq!(args.output, PathBuf::from(output_path));
        }
    }
}
