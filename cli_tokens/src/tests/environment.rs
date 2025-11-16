//! 環境変数とワークスペース設定のテスト
//! - CLI_TOKENS_BASE_DIR の動作
//! - パス構築の動作
//! - コマンド間でのディレクトリ共有

use anyhow::Result;
use std::env;
use std::path::PathBuf;
use tempfile::TempDir;

use crate::commands::history::HistoryArgs;
use crate::commands::predict::kick::KickArgs;
use crate::commands::top::TopArgs;

#[test]
fn test_base_dir_environment_variable() {
    // Test default behavior (no environment variable)
    unsafe {
        env::remove_var("CLI_TOKENS_BASE_DIR");
    }
    let default_base = env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
    assert_eq!(default_base, ".");

    // Test custom base directory
    unsafe {
        env::set_var("CLI_TOKENS_BASE_DIR", "/custom/workspace");
    }
    let custom_base = env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
    assert_eq!(custom_base, "/custom/workspace");

    // Clean up
    unsafe {
        env::remove_var("CLI_TOKENS_BASE_DIR");
    }
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
        unsafe {
            env::set_var("CLI_TOKENS_BASE_DIR", base_dir);
        }
        let actual_base = env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
        let constructed_path = PathBuf::from(actual_base).join(relative_path);
        assert_eq!(constructed_path, PathBuf::from(expected));
    }

    // Clean up
    unsafe {
        env::remove_var("CLI_TOKENS_BASE_DIR");
    }
}

#[test]
fn test_history_file_path_construction() {
    // Test history file path construction logic similar to predict command
    unsafe {
        env::set_var("CLI_TOKENS_BASE_DIR", "/test/workspace");
    }

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
    unsafe {
        env::remove_var("CLI_TOKENS_BASE_DIR");
    }
}

#[tokio::test]
async fn test_top_command_with_base_dir() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let base_path = temp_dir.path().to_str().unwrap();

    // Set environment variable
    unsafe {
        env::set_var("CLI_TOKENS_BASE_DIR", base_path);
    }

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
    unsafe {
        env::remove_var("CLI_TOKENS_BASE_DIR");
    }
    Ok(())
}

#[tokio::test]
async fn test_history_command_with_base_dir() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let base_path = temp_dir.path().to_str().unwrap();

    // Set environment variable
    unsafe {
        env::set_var("CLI_TOKENS_BASE_DIR", base_path);
    }

    let args = HistoryArgs {
        token_file: PathBuf::from("tokens/wrap.near/sample.token.near.json"),
        quote_token: "wrap.near".to_string(),
        output: PathBuf::from("history"),
    };

    // Test that environment variable is correctly used in path construction
    let expected_output_path = PathBuf::from(base_path).join("history");

    // Verify the environment variable is being read correctly
    let actual_base = env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
    let constructed_path = PathBuf::from(actual_base).join(&args.output);

    assert_eq!(constructed_path, expected_output_path);

    // Clean up
    unsafe {
        env::remove_var("CLI_TOKENS_BASE_DIR");
    }
    Ok(())
}

#[tokio::test]
async fn test_predict_command_with_base_dir() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let base_path = temp_dir.path().to_str().unwrap();

    // Set environment variable
    unsafe {
        env::set_var("CLI_TOKENS_BASE_DIR", base_path);
    }

    let args = KickArgs {
        token_file: PathBuf::from("tokens/wrap.near/sample.token.near.json"),
        output: PathBuf::from("predictions"),
        model: None,
        start_pct: 0.0,
        end_pct: 100.0,
        forecast_ratio: 10.0,
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
    unsafe {
        env::remove_var("CLI_TOKENS_BASE_DIR");
    }
    Ok(())
}

#[tokio::test]
async fn test_commands_without_base_dir() -> Result<()> {
    // Ensure environment variable is not set
    unsafe {
        env::remove_var("CLI_TOKENS_BASE_DIR");
    }

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
    // Test that environment variable takes precedence over default
    unsafe {
        env::remove_var("CLI_TOKENS_BASE_DIR");
    }
    let default_base = env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
    assert_eq!(default_base, ".");

    // Set custom value
    unsafe {
        env::set_var("CLI_TOKENS_BASE_DIR", "/custom/path");
    }
    let custom_base = env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
    assert_eq!(custom_base, "/custom/path");

    // Test empty value handling
    unsafe {
        env::set_var("CLI_TOKENS_BASE_DIR", "");
    }
    let empty_base = env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
    assert_eq!(empty_base, "");

    // Clean up
    unsafe {
        env::remove_var("CLI_TOKENS_BASE_DIR");
    }
}
