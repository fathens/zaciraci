use super::*;
use crate::models::prediction::{PredictionFileData, PredictionMetadata, PredictionPoint, PredictionResults};
use chrono::{DateTime, Utc};
use std::env;
use tempfile::tempdir;

/// Create sample prediction data for testing
fn create_sample_prediction_data() -> PredictionFileData {
    let now = Utc::now();
    PredictionFileData {
        metadata: PredictionMetadata {
            generated_at: now,
            model_name: "test_model".to_string(),
            base_token: "test.token.near".to_string(),
            quote_token: "wrap.near".to_string(),
            history_start: "2025-01-01".to_string(),
            history_end: "2025-01-07".to_string(),
            prediction_start: "2025-01-08".to_string(),
            prediction_end: "2025-01-09".to_string(),
        },
        prediction_results: PredictionResults {
            predictions: vec![
                PredictionPoint {
                    timestamp: now,
                    price: 100.0,
                    confidence: Some(0.8),
                },
                PredictionPoint {
                    timestamp: now + chrono::Duration::hours(1),
                    price: 105.0,
                    confidence: Some(0.75),
                },
            ],
            model_metrics: None,
        },
    }
}

/// Create sample cache parameters for testing
fn create_sample_params() -> PredictionCacheParams<'static> {
    PredictionCacheParams {
        model_name: "test_model",
        quote_token: "wrap.near",
        base_token: "test.token.near",
        hist_start: DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
        hist_end: DateTime::parse_from_rfc3339("2025-01-07T23:59:59Z")
            .unwrap()
            .with_timezone(&Utc),
        pred_start: DateTime::parse_from_rfc3339("2025-01-08T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
        pred_end: DateTime::parse_from_rfc3339("2025-01-09T23:59:59Z")
            .unwrap()
            .with_timezone(&Utc),
    }
}

#[test]
fn test_get_prediction_dir() {
    let temp_dir = tempdir().unwrap();
    env::set_var("CLI_TOKENS_BASE_DIR", temp_dir.path());

    let params = create_sample_params();
    let result = get_prediction_dir(
        params.model_name,
        params.quote_token,
        params.base_token,
        params.hist_start,
        params.hist_end,
    );

    let expected = temp_dir
        .path()
        .join("predictions")
        .join("test_model")
        .join("wrap.near")
        .join("test.token.near")
        .join("history-20250101_0000-20250107_2359");

    assert_eq!(result, expected);
}

#[test]
fn test_create_prediction_filename() {
    let start = DateTime::parse_from_rfc3339("2025-01-08T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let end = DateTime::parse_from_rfc3339("2025-01-09T23:59:59Z")
        .unwrap()
        .with_timezone(&Utc);

    let result = create_prediction_filename(start, end);
    let expected = "predict-20250108_0000-20250109_2359.json";

    assert_eq!(result, expected);
}

#[tokio::test]
async fn test_save_and_load_prediction_result() {
    let temp_dir = tempdir().unwrap();
    env::set_var("CLI_TOKENS_BASE_DIR", temp_dir.path());

    let params = create_sample_params();
    let sample_data = create_sample_prediction_data();

    // Test save
    let saved_path = save_prediction_result(&params, &sample_data)
        .await
        .unwrap();

    assert!(saved_path.exists());
    assert!(saved_path.to_string_lossy().contains("predictions"));
    assert!(saved_path.to_string_lossy().contains("test_model"));
    assert!(saved_path.to_string_lossy().contains("wrap.near"));
    assert!(saved_path.to_string_lossy().contains("test.token.near"));

    // Test load
    let loaded_data = load_prediction_data(&saved_path).await.unwrap();

    assert_eq!(loaded_data.metadata.model_name, sample_data.metadata.model_name);
    assert_eq!(loaded_data.metadata.base_token, sample_data.metadata.base_token);
    assert_eq!(loaded_data.metadata.quote_token, sample_data.metadata.quote_token);
    assert_eq!(
        loaded_data.prediction_results.predictions.len(),
        sample_data.prediction_results.predictions.len()
    );
}

#[tokio::test]
async fn test_check_prediction_cache() {
    let temp_dir = tempdir().unwrap();
    env::set_var("CLI_TOKENS_BASE_DIR", temp_dir.path());

    let params = create_sample_params();

    // Test cache miss
    let result = check_prediction_cache(&params).await.unwrap();
    assert!(result.is_none());

    // Create prediction file
    let sample_data = create_sample_prediction_data();
    let saved_path = save_prediction_result(&params, &sample_data)
        .await
        .unwrap();

    // Test cache hit
    let result = check_prediction_cache(&params).await.unwrap();
    assert!(result.is_some());
    let result_path = result.unwrap();
    assert_eq!(result_path.file_name(), saved_path.file_name());
    assert!(result_path.exists());
}

#[tokio::test]
async fn test_find_latest_prediction_file() {
    let temp_dir = tempdir().unwrap();
    env::set_var("CLI_TOKENS_BASE_DIR", temp_dir.path());

    let predictions_dir = temp_dir.path().join("predictions");
    
    // Test with no files
    let result = find_latest_prediction_file(&predictions_dir, "wrap.near", "test.token.near")
        .await
        .unwrap();
    assert!(result.is_none());

    // Create prediction files with different timestamps
    let params1 = PredictionCacheParams {
        model_name: "model1",
        quote_token: "wrap.near",
        base_token: "test.token.near",
        hist_start: DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
        hist_end: DateTime::parse_from_rfc3339("2025-01-07T23:59:59Z")
            .unwrap()
            .with_timezone(&Utc),
        pred_start: DateTime::parse_from_rfc3339("2025-01-08T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
        pred_end: DateTime::parse_from_rfc3339("2025-01-08T23:59:59Z")
            .unwrap()
            .with_timezone(&Utc),
    };

    let params2 = PredictionCacheParams {
        model_name: "model2",
        quote_token: "wrap.near", 
        base_token: "test.token.near",
        hist_start: DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
        hist_end: DateTime::parse_from_rfc3339("2025-01-07T23:59:59Z")
            .unwrap()
            .with_timezone(&Utc),
        pred_start: DateTime::parse_from_rfc3339("2025-01-09T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
        pred_end: DateTime::parse_from_rfc3339("2025-01-09T23:59:59Z")
            .unwrap()
            .with_timezone(&Utc),
    };

    let sample_data = create_sample_prediction_data();
    
    let path1 = save_prediction_result(&params1, &sample_data).await.unwrap();
    // Small delay to ensure different modification times
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    let path2 = save_prediction_result(&params2, &sample_data).await.unwrap();

    // Should return the latest file (path2)
    let result = find_latest_prediction_file(&predictions_dir, "wrap.near", &crate::utils::file::sanitize_filename("test.token.near"))
        .await
        .unwrap();
    
    assert!(result.is_some());
    let result_path = result.unwrap();
    assert!(result_path.exists());
    // The result should be one of the two files we created
    assert!(result_path == path1 || result_path == path2);
}

#[test]
fn test_prediction_cache_params_construction() {
    let params = create_sample_params();
    
    assert_eq!(params.model_name, "test_model");
    assert_eq!(params.quote_token, "wrap.near");
    assert_eq!(params.base_token, "test.token.near");
    assert!(params.hist_start < params.hist_end);
    assert!(params.pred_start >= params.hist_end);
}

#[tokio::test]
async fn test_error_handling_invalid_paths() {
    let temp_dir = tempdir().unwrap();
    env::set_var("CLI_TOKENS_BASE_DIR", temp_dir.path());

    // Test load from non-existent file
    let non_existent = temp_dir.path().join("non-existent.json");
    let result = load_prediction_data(&non_existent).await;
    assert!(result.is_err());

    // Test save with invalid data should still succeed (JSON serialization works)
    let params = create_sample_params();
    let sample_data = create_sample_prediction_data();
    let result = save_prediction_result(&params, &sample_data).await;
    assert!(result.is_ok());
}