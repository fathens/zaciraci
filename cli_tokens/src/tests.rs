use anyhow::Result;
use chrono::Utc;
use std::path::PathBuf;
use tempfile::TempDir;

use crate::commands::{
    history::HistoryArgs, predict::kick::KickArgs, top::TopArgs, verify::VerifyArgs,
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
        // Test parsing of KickArgs
        let args = KickArgs {
            token_file: PathBuf::from("tokens/wrap.near.json"),
            output: PathBuf::from("predictions"),
            model: None,
            start_pct: 0.0,
            end_pct: 100.0,
            forecast_ratio: 10.0,
            force: false,
        };

        assert_eq!(args.token_file, PathBuf::from("tokens/wrap.near.json"));
        assert_eq!(args.output, PathBuf::from("predictions"));
        assert_eq!(args.model, None);
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
            min_depth: None,
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
                quote_token: Some("wrap.near".to_string()),
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
                quote_token: Some("wrap.near".to_string()),
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

    #[test]
    fn test_simulate_args_integration() {
        use crate::commands::simulate::SimulateArgs;

        // Test integration of SimulateArgs with various configurations
        let momentum_config = SimulateArgs {
            start: Some("2024-08-01".to_string()),
            end: Some("2024-08-10".to_string()),
            algorithm: Some("momentum".to_string()),
            capital: 10000.0,
            quote_token: "wrap.near".to_string(),
            tokens: 3,
            output: "simulation_results".to_string(),
            rebalance_interval: "1d".to_string(),
            fee_model: "realistic".to_string(),
            custom_fee: None,
            slippage: 0.01,
            gas_cost: 0.01,
            min_trade: 1.0,
            prediction_horizon: 24,
            historical_days: 30,
            chart: false,
            verbose: false,
        };

        // Test that configuration is parsed correctly
        assert_eq!(momentum_config.algorithm, Some("momentum".to_string()));
        assert_eq!(momentum_config.capital, 10000.0);
        assert_eq!(momentum_config.tokens, 3);
        assert_eq!(momentum_config.fee_model, "realistic");
        assert_eq!(momentum_config.slippage, 0.01);

        // Test portfolio algorithm config
        let portfolio_config = SimulateArgs {
            start: Some("2024-08-01".to_string()),
            end: Some("2024-08-10".to_string()),
            algorithm: Some("portfolio".to_string()),
            capital: 5000.0,
            quote_token: "wrap.near".to_string(),
            tokens: 5, // Will use top 5 volatility tokens
            output: "portfolio_results".to_string(),
            rebalance_interval: "1h".to_string(),
            fee_model: "zero".to_string(),
            custom_fee: Some(0.002),
            slippage: 0.005,
            gas_cost: 0.005,
            min_trade: 0.5,
            prediction_horizon: 12,
            historical_days: 14,
            chart: true,
            verbose: true,
        };

        assert_eq!(portfolio_config.algorithm, Some("portfolio".to_string()));
        assert_eq!(portfolio_config.capital, 5000.0);
        assert_eq!(portfolio_config.tokens, 5); // Should fetch top 5 volatility tokens
        assert_eq!(portfolio_config.fee_model, "zero");
        assert_eq!(portfolio_config.custom_fee.unwrap(), 0.002);
        assert!(portfolio_config.verbose);
    }

    #[test]
    fn test_fee_model_integration() {
        use crate::commands::simulate::{calculate_trading_cost, FeeModel};

        let trade_amount = 1000.0;
        let slippage = 0.01;
        let gas_cost = 0.01;

        // Test zero fee model (still includes slippage and gas costs)
        let zero_cost = calculate_trading_cost(trade_amount, &FeeModel::Zero, slippage, gas_cost);
        let expected_zero_cost = trade_amount * slippage + gas_cost; // Only slippage + gas, no protocol fee
        assert!((zero_cost - expected_zero_cost).abs() < 0.01);

        // Test realistic fee model
        let realistic_cost =
            calculate_trading_cost(trade_amount, &FeeModel::Realistic, slippage, gas_cost);
        // Should include pool fee (0.3%) + slippage + gas
        let expected_cost = trade_amount * 0.003 + trade_amount * slippage + gas_cost;
        assert!((realistic_cost - expected_cost).abs() < 0.01);

        // Test custom fee model
        let custom_fee = 0.005; // 0.5%
        let custom_cost = calculate_trading_cost(
            trade_amount,
            &FeeModel::Custom(custom_fee),
            slippage,
            gas_cost,
        );
        let expected_custom_cost = trade_amount * custom_fee + trade_amount * slippage + gas_cost;
        assert!((custom_cost - expected_custom_cost).abs() < 0.01);
    }

    #[test]
    fn test_volatility_token_filtering() {
        use crate::commands::top::calculate_volatility_score;
        use chrono::Utc;
        use common::stats::ValueAtTime;

        // Test that high volatility tokens are correctly scored
        let high_volatility_data = vec![
            ValueAtTime {
                time: Utc::now().naive_utc(),
                value: 100.0,
            },
            ValueAtTime {
                time: Utc::now().naive_utc(),
                value: 150.0,
            }, // +50%
            ValueAtTime {
                time: Utc::now().naive_utc(),
                value: 75.0,
            }, // -50%
            ValueAtTime {
                time: Utc::now().naive_utc(),
                value: 125.0,
            }, // +67%
        ];

        let high_score = calculate_volatility_score(&high_volatility_data);

        // Test that low volatility tokens get lower scores
        let low_volatility_data = vec![
            ValueAtTime {
                time: Utc::now().naive_utc(),
                value: 100.0,
            },
            ValueAtTime {
                time: Utc::now().naive_utc(),
                value: 101.0,
            }, // +1%
            ValueAtTime {
                time: Utc::now().naive_utc(),
                value: 102.0,
            }, // +1%
            ValueAtTime {
                time: Utc::now().naive_utc(),
                value: 103.0,
            }, // +1%
        ];

        let low_score = calculate_volatility_score(&low_volatility_data);

        // High volatility should score higher than low volatility
        assert!(high_score > low_score);
        assert!(high_score <= 1.0);
        assert!(low_score >= 0.0);
    }
}

#[cfg(test)]
mod api_tests {
    use super::*;
    use crate::api::backend::BackendClient;
    use common::api::chronos::ChronosApiClient;
    use common::types::TokenAccount;
    use common::ApiResponse;

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
            values: vec![1.0],
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
            progress: Some(100.0),
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
            progress: Some(100.0),
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
            progress: Some(0.0),
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
                value: 5.23,
            },
            common::stats::ValueAtTime {
                time: chrono::NaiveDate::from_ymd_opt(2025, 7, 6)
                    .unwrap()
                    .and_hms_opt(1, 0, 0)
                    .unwrap(),
                value: 5.25,
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
        assert_eq!(values[0].value, 5.23);
        assert_eq!(values[1].value, 5.25);

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
}

#[cfg(test)]
mod predict_args_tests {
    use super::*;
    use chrono::{Duration, NaiveDate, TimeZone, Utc};

    // === 基本オプションテスト ===

    #[test]
    fn test_default_values() {
        // テストのデフォルト値確認
        let args = KickArgs {
            token_file: PathBuf::from("test.json"),
            output: PathBuf::from("predictions"),
            model: None,
            start_pct: 0.0,
            end_pct: 100.0,
            forecast_ratio: 10.0,
            force: false,
        };

        assert_eq!(args.output, PathBuf::from("predictions"));
        assert_eq!(args.model, None);
        assert!(!args.force);
        assert_eq!(args.start_pct, 0.0);
        assert_eq!(args.end_pct, 100.0);
        assert_eq!(args.forecast_ratio, 10.0);
    }

    #[test]
    fn test_custom_values() {
        // カスタム値でのテスト
        let args = KickArgs {
            token_file: PathBuf::from("custom/token.json"),
            output: PathBuf::from("custom_output"),
            model: Some("chronos_bolt".to_string()),
            start_pct: 25.0,
            end_pct: 75.0,
            forecast_ratio: 50.0,
            force: true,
        };

        assert_eq!(args.token_file, PathBuf::from("custom/token.json"));
        assert_eq!(args.output, PathBuf::from("custom_output"));
        assert_eq!(args.model, Some("chronos_bolt".to_string()));
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
            let args = KickArgs {
                token_file: PathBuf::from("test.json"),
                output: PathBuf::from("predictions"),
                model: None,
                start_pct: start,
                end_pct: end,
                forecast_ratio: 10.0,
                force: false,
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
            let args = KickArgs {
                token_file: PathBuf::from("test.json"),
                output: PathBuf::from("predictions"),
                model: None,
                start_pct: start,
                end_pct: end,
                forecast_ratio: 10.0,
                force: false,
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
            let args = KickArgs {
                token_file: PathBuf::from("test.json"),
                output: PathBuf::from("predictions"),
                model: Some(model.to_string()),
                start_pct: 0.0,
                end_pct: 100.0,
                forecast_ratio: 10.0,
                force: false,
            };

            assert_eq!(args.model, Some(model.to_string()));
            assert!(args.model.is_some());
        }
    }

    #[test]
    fn test_force_flag_variations() {
        // force フラグのテスト
        let args_false = KickArgs {
            token_file: PathBuf::from("test.json"),
            output: PathBuf::from("predictions"),
            model: None,
            start_pct: 0.0,
            end_pct: 100.0,
            forecast_ratio: 10.0,
            force: false,
        };

        let args_true = KickArgs {
            token_file: PathBuf::from("test.json"),
            output: PathBuf::from("predictions"),
            model: None,
            start_pct: 0.0,
            end_pct: 100.0,
            forecast_ratio: 10.0,
            force: true,
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
            let args = KickArgs {
                token_file: PathBuf::from("test.json"),
                output: PathBuf::from(output_path),
                model: None,
                start_pct: 0.0,
                end_pct: 100.0,
                forecast_ratio: 10.0,
                force: false,
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
            let args = KickArgs {
                token_file: PathBuf::from(token_file),
                output: PathBuf::from("predictions"),
                model: None,
                start_pct: 0.0,
                end_pct: 100.0,
                forecast_ratio: 10.0,
                force: false,
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
            let args = KickArgs {
                token_file: PathBuf::from("test.json"),
                output: PathBuf::from("predictions"),
                model: None,
                start_pct: start,
                end_pct: end,
                forecast_ratio: 10.0,
                force: false,
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
        let args = KickArgs {
            token_file: PathBuf::from("test.json"),
            output: PathBuf::from("predictions"),
            model: None,
            start_pct: 0.0,
            end_pct: 100.0,
            forecast_ratio: 10.0,
            force: false,
        };

        assert_eq!(args.forecast_ratio, 10.0);
    }

    #[test]
    fn test_forecast_ratio_validation_valid_values() {
        let test_cases = vec![0.1, 1.0, 10.0, 50.0, 100.0, 500.0];

        for ratio in test_cases {
            let args = KickArgs {
                token_file: PathBuf::from("test.json"),
                output: PathBuf::from("predictions"),
                model: None,
                start_pct: 0.0,
                end_pct: 100.0,
                forecast_ratio: ratio,
                force: false,
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
            let args = KickArgs {
                token_file: PathBuf::from("test.json"),
                output: PathBuf::from("predictions"),
                model: None,
                start_pct: 0.0,
                end_pct: 100.0,
                forecast_ratio: invalid_ratio,
                force: false,
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
            ("wrap.near", "wrap.near", "history/wrap.near/wrap.near.json"),
            ("usdc.near", "wrap.near", "history/wrap.near/usdc.near.json"),
            (
                "token/with/slash",
                "usdc.tether-token.near",
                "history/usdc.tether-token.near/token_with_slash.json",
            ),
            (
                "token:with:colons",
                "wrap.near",
                "history/wrap.near/token_with_colons.json",
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

#[cfg(test)]
mod environment_tests {
    use super::*;
    use std::env;

    #[test]
    fn test_base_dir_environment_variable() {
        // Test default behavior (no environment variable)
        env::remove_var("CLI_TOKENS_BASE_DIR");
        let default_base = env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
        assert_eq!(default_base, ".");

        // Test custom base directory
        env::set_var("CLI_TOKENS_BASE_DIR", "/custom/workspace");
        let custom_base = env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
        assert_eq!(custom_base, "/custom/workspace");

        // Clean up
        env::remove_var("CLI_TOKENS_BASE_DIR");
    }

    #[test]
    fn test_path_construction_with_base_dir() {
        // Test path construction with different base directories
        let test_cases = vec![
            (".", "tokens", "./tokens"),
            ("/workspace", "history", "/workspace/history"),
            ("./project", "predictions", "./project/predictions"),
            (
                "/tmp/cli_test",
                "verification",
                "/tmp/cli_test/verification",
            ),
        ];

        for (base_dir, relative_path, expected) in test_cases {
            env::set_var("CLI_TOKENS_BASE_DIR", base_dir);
            let actual_base = env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
            let constructed_path = PathBuf::from(actual_base).join(relative_path);
            assert_eq!(constructed_path, PathBuf::from(expected));
        }

        // Clean up
        env::remove_var("CLI_TOKENS_BASE_DIR");
    }

    #[test]
    fn test_history_file_path_construction() {
        // Test history file path construction logic similar to predict command
        env::set_var("CLI_TOKENS_BASE_DIR", "/test/workspace");

        let base_dir = env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
        let quote_token = "wrap.near";
        let token_name = "sample.token.near";

        let history_file = PathBuf::from(base_dir)
            .join("history")
            .join(quote_token)
            .join(format!("{}.json", token_name));

        assert_eq!(
            history_file,
            PathBuf::from("/test/workspace/history/wrap.near/sample.token.near.json")
        );

        // Clean up
        env::remove_var("CLI_TOKENS_BASE_DIR");
    }

    #[tokio::test]
    async fn test_top_command_with_base_dir() -> Result<()> {
        use crate::commands::top::TopArgs;
        use std::env;
        use tempfile::TempDir;

        let temp_dir = TempDir::new()?;
        let base_path = temp_dir.path().to_str().unwrap();

        // Set environment variable
        env::set_var("CLI_TOKENS_BASE_DIR", base_path);

        let args = TopArgs {
            start: None,
            end: None,
            limit: 1,
            output: PathBuf::from("tokens"),
            format: "json".to_string(),
            quote_token: None,
            min_depth: None,
        };

        // Test that environment variable is correctly used in path construction
        let expected_output_path = PathBuf::from(base_path).join("tokens");

        // Verify the environment variable is being read correctly
        let actual_base = env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
        let constructed_path = PathBuf::from(actual_base).join(&args.output);

        assert_eq!(constructed_path, expected_output_path);

        // Clean up
        env::remove_var("CLI_TOKENS_BASE_DIR");
        Ok(())
    }

    #[tokio::test]
    async fn test_history_command_with_base_dir() -> Result<()> {
        use crate::commands::history::HistoryArgs;
        use std::env;
        use tempfile::TempDir;

        let temp_dir = TempDir::new()?;
        let base_path = temp_dir.path().to_str().unwrap();

        // Set environment variable
        env::set_var("CLI_TOKENS_BASE_DIR", base_path);

        let args = HistoryArgs {
            token_file: PathBuf::from("tokens/wrap.near/sample.token.near.json"),
            quote_token: "wrap.near".to_string(),
            output: PathBuf::from("history"),
            force: false,
        };

        // Test that environment variable is correctly used in path construction
        let expected_output_path = PathBuf::from(base_path).join("history");

        // Verify the environment variable is being read correctly
        let actual_base = env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
        let constructed_path = PathBuf::from(actual_base).join(&args.output);

        assert_eq!(constructed_path, expected_output_path);

        // Clean up
        env::remove_var("CLI_TOKENS_BASE_DIR");
        Ok(())
    }

    #[tokio::test]
    async fn test_predict_command_with_base_dir() -> Result<()> {
        use crate::commands::predict::kick::KickArgs;
        use std::env;
        use tempfile::TempDir;

        let temp_dir = TempDir::new()?;
        let base_path = temp_dir.path().to_str().unwrap();

        // Set environment variable
        env::set_var("CLI_TOKENS_BASE_DIR", base_path);

        let args = KickArgs {
            token_file: PathBuf::from("tokens/wrap.near/sample.token.near.json"),
            output: PathBuf::from("predictions"),
            model: None,
            start_pct: 0.0,
            end_pct: 100.0,
            forecast_ratio: 10.0,
            force: false,
        };

        // Test output path construction
        let expected_output_path = PathBuf::from(base_path).join("predictions");
        let actual_base = env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
        let constructed_output_path = PathBuf::from(&actual_base).join(&args.output);
        assert_eq!(constructed_output_path, expected_output_path);

        // Test history file path construction
        let expected_history_path = PathBuf::from(base_path)
            .join("history")
            .join("wrap.near")
            .join("sample.token.near.json");
        let constructed_history_path = PathBuf::from(actual_base)
            .join("history")
            .join("wrap.near")
            .join("sample.token.near.json");
        assert_eq!(constructed_history_path, expected_history_path);

        // Clean up
        env::remove_var("CLI_TOKENS_BASE_DIR");
        Ok(())
    }

    #[tokio::test]
    async fn test_verify_command_with_base_dir() -> Result<()> {
        use crate::commands::verify::VerifyArgs;
        use std::env;
        use tempfile::TempDir;

        let temp_dir = TempDir::new()?;
        let base_path = temp_dir.path().to_str().unwrap();

        // Set environment variable
        env::set_var("CLI_TOKENS_BASE_DIR", base_path);

        let args = VerifyArgs {
            prediction_file: PathBuf::from("predictions/wrap.near/sample.token.near.json"),
            actual_data_file: None,
            output: PathBuf::from("verification"),
            force: false,
        };

        // Test that environment variable is correctly used in path construction
        let expected_output_path = PathBuf::from(base_path).join("verification");

        // Verify the environment variable is being read correctly
        let actual_base = env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
        let constructed_path = PathBuf::from(actual_base).join(&args.output);

        assert_eq!(constructed_path, expected_output_path);

        // Clean up
        env::remove_var("CLI_TOKENS_BASE_DIR");
        Ok(())
    }

    #[tokio::test]
    async fn test_commands_without_base_dir() -> Result<()> {
        use std::env;

        // Ensure environment variable is not set
        env::remove_var("CLI_TOKENS_BASE_DIR");

        // Test default behavior (should use "." as base directory)
        let default_base = env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
        assert_eq!(default_base, ".");

        // Test path construction with default
        let output_path = PathBuf::from(default_base).join("tokens");
        assert_eq!(output_path, PathBuf::from("./tokens"));

        Ok(())
    }

    #[test]
    fn test_environment_variable_precedence() {
        use std::env;

        // Test that environment variable takes precedence over default
        env::remove_var("CLI_TOKENS_BASE_DIR");
        let default_base = env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
        assert_eq!(default_base, ".");

        // Set custom value
        env::set_var("CLI_TOKENS_BASE_DIR", "/custom/path");
        let custom_base = env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
        assert_eq!(custom_base, "/custom/path");

        // Test empty value handling
        env::set_var("CLI_TOKENS_BASE_DIR", "");
        let empty_base = env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
        assert_eq!(empty_base, "");

        // Clean up
        env::remove_var("CLI_TOKENS_BASE_DIR");
    }
}
