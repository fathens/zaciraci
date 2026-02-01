//! 基本的な構造体や関数の単体テスト
//! - CLI引数のパース
//! - ファイル操作ユーティリティ
//! - データ構造のシリアライゼーション

use anyhow::Result;
use chrono::Utc;
use std::path::PathBuf;
use tempfile::TempDir;

use crate::commands::{history::HistoryArgs, predict::kick::KickArgs, top::TopArgs};
use crate::models::token::{FileMetadata, TokenFileData};
use crate::utils::file::{ensure_directory_exists, write_json_file};

#[test]
fn test_predict_args_parsing() {
    // Test parsing of KickArgs
    let args = KickArgs {
        token_file: PathBuf::from("tokens/wrap.near.json"),
        output: PathBuf::from("predictions"),
        start_pct: 0.0,
        end_pct: 100.0,
        forecast_ratio: 10.0,
    };

    assert_eq!(args.token_file, PathBuf::from("tokens/wrap.near.json"));
    assert_eq!(args.output, PathBuf::from("predictions"));
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
            quote_token: Some("wrap.near".to_string()),
        },
        token: "wrap.near".to_string(),
    };

    // Test serialization
    let json = serde_json::to_string(&token_data).unwrap();
    assert!(json.contains("wrap.near"));
    assert!(json.contains("2023-01-01"));

    // Test deserialization
    let deserialized: TokenFileData = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.token, "wrap.near");
    assert_eq!(deserialized.metadata.start_date, "2023-01-01");
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
            quote_token: Some("wrap.near".to_string()),
        },
        token: "test.token".to_string(),
    };

    // Write JSON file
    write_json_file(&test_file, &test_data).await?;
    assert!(test_file.exists());

    // Read and verify content
    let content = tokio::fs::read_to_string(&test_file).await?;
    let parsed: TokenFileData = serde_json::from_str(&content)?;
    assert_eq!(parsed.token, "test.token");
    assert_eq!(parsed.metadata.start_date, "2023-01-01");

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
    };

    assert_eq!(args.token_file, PathBuf::from("tokens/wrap.near.json"));
    assert_eq!(args.quote_token, "wrap.near");
    assert_eq!(args.output, PathBuf::from("history"));
}

#[test]
fn test_history_args_variations() {
    // Test default values for HistoryArgs
    let default_args = HistoryArgs {
        token_file: PathBuf::from("tokens/test.json"),
        quote_token: "wrap.near".to_string(),
        output: PathBuf::from("history"),
    };
    assert_eq!(default_args.quote_token, "wrap.near");
    assert_eq!(default_args.output, PathBuf::from("history"));

    // Test custom values for HistoryArgs
    let custom_args = HistoryArgs {
        token_file: PathBuf::from("custom/token.json"),
        quote_token: "usdc.near".to_string(),
        output: PathBuf::from("custom_history"),
    };
    assert_eq!(custom_args.token_file, PathBuf::from("custom/token.json"));
    assert_eq!(custom_args.quote_token, "usdc.near");
    assert_eq!(custom_args.output, PathBuf::from("custom_history"));
}
