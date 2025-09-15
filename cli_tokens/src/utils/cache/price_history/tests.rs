use super::*;
use chrono::{TimeZone, Utc};
use std::env;
use tempfile::tempdir;
use tokio::fs;

#[tokio::test]
async fn test_real_cache_behavior() {
    // Set up temporary directory for testing
    let temp_dir = tempdir().unwrap();
    env::set_var("CLI_TOKENS_BASE_DIR", temp_dir.path());

    let quote_token = "wrap.near";
    let base_token = "akaia.tkn.near";

    // Create a mock cache file similar to what we have
    let history_dir = get_price_history_dir(quote_token, base_token);
    fs::create_dir_all(&history_dir).await.unwrap();

    let start_time = Utc.with_ymd_and_hms(2025, 7, 11, 0, 0, 0).unwrap();
    let end_time = Utc.with_ymd_and_hms(2025, 8, 21, 23, 59, 0).unwrap();
    let filename = create_history_filename(start_time, end_time);
    let file_path = history_dir.join(filename);

    // Create some mock data with data in the requested range
    let mock_values = vec![
        ValueAtTime {
            time: start_time.naive_utc(),
            value: 1.0,
        },
        ValueAtTime {
            time: Utc
                .with_ymd_and_hms(2025, 8, 10, 12, 0, 0)
                .unwrap()
                .naive_utc(),
            value: 1.5,
        },
        ValueAtTime {
            time: end_time.naive_utc(),
            value: 2.0,
        },
    ];

    let history_data = HistoryFileData {
        metadata: HistoryMetadata {
            generated_at: Utc::now(),
            start_date: start_time.format("%Y-%m-%d").to_string(),
            end_date: end_time.format("%Y-%m-%d").to_string(),
            base_token: base_token.to_string(),
            quote_token: quote_token.to_string(),
        },
        price_history: PriceHistory {
            values: mock_values,
        },
    };

    let json_content = serde_json::to_string_pretty(&history_data).unwrap();
    fs::write(&file_path, json_content).await.unwrap();

    // Test: Request fully covered by cache
    let request_start = Utc.with_ymd_and_hms(2025, 8, 10, 0, 0, 0).unwrap();
    let request_end = Utc.with_ymd_and_hms(2025, 8, 11, 0, 0, 0).unwrap();

    let cache_result = check_history_cache(quote_token, base_token, request_start, request_end)
        .await
        .unwrap();

    assert!(
        cache_result.is_some(),
        "Cache should contain data for this range"
    );

    // Test: Request partially outside cache
    let request_start = Utc.with_ymd_and_hms(2025, 8, 22, 0, 0, 0).unwrap();
    let request_end = Utc.with_ymd_and_hms(2025, 8, 23, 0, 0, 0).unwrap();

    let cache_result = check_history_cache(quote_token, base_token, request_start, request_end)
        .await
        .unwrap();

    assert!(
        cache_result.is_none(),
        "Cache should not return data for range outside its coverage"
    );
}

#[test]
fn test_filename_parsing() {
    // Test the actual filename format we have in cache
    let filename = "history-20250711_0000-20250821_2359.json";

    if filename.starts_with("history-") && filename.ends_with(".json") {
        let date_part = &filename[8..filename.len() - 5];

        if let Some((start_str, end_str)) = date_part.split_once('-') {
            let file_start = parse_datetime(start_str);
            let file_end = parse_datetime(end_str);

            assert!(file_start.is_ok());
            assert!(file_end.is_ok());

            let start_dt = file_start.unwrap();
            let end_dt = file_end.unwrap();

            assert_eq!(
                start_dt,
                Utc.with_ymd_and_hms(2025, 7, 11, 0, 0, 0).unwrap()
            );
            assert_eq!(
                end_dt,
                Utc.with_ymd_and_hms(2025, 8, 21, 23, 59, 0).unwrap()
            );
        }
    }
}

#[tokio::test]
async fn test_find_overlapping_files_debug() {
    // Set up temporary directory with actual cache structure
    let temp_dir = tempdir().unwrap();
    env::set_var("CLI_TOKENS_BASE_DIR", temp_dir.path());

    let quote_token = "wrap.near";
    let base_token = "akaia.tkn.near";
    let history_dir = get_price_history_dir(quote_token, base_token);
    fs::create_dir_all(&history_dir).await.unwrap();

    // Create a file with the exact format we have in real cache
    let filename = "history-20250711_0000-20250821_2359.json";
    let file_path = history_dir.join(filename);

    // Create mock data that covers the full requested range
    let mut mock_values = Vec::new();

    // Add data points covering the full range from file start to file end
    let file_start = Utc.with_ymd_and_hms(2025, 7, 11, 0, 0, 0).unwrap();
    let file_end = Utc.with_ymd_and_hms(2025, 8, 21, 23, 59, 0).unwrap();

    // Add a data point at the beginning
    mock_values.push(ValueAtTime {
        time: file_start.naive_utc(),
        value: 1.0,
    });

    // Add data points in the requested range (2025-08-10 to 2025-08-11)
    mock_values.push(ValueAtTime {
        time: Utc
            .with_ymd_and_hms(2025, 8, 10, 12, 0, 0)
            .unwrap()
            .naive_utc(),
        value: 1.5,
    });

    mock_values.push(ValueAtTime {
        time: Utc
            .with_ymd_and_hms(2025, 8, 11, 0, 0, 0)
            .unwrap()
            .naive_utc(),
        value: 2.0,
    });

    // Add a data point at the end
    mock_values.push(ValueAtTime {
        time: file_end.naive_utc(),
        value: 2.5,
    });

    let history_data = HistoryFileData {
        metadata: HistoryMetadata {
            generated_at: Utc::now(),
            start_date: "2025-07-11".to_string(),
            end_date: "2025-08-21".to_string(),
            base_token: base_token.to_string(),
            quote_token: quote_token.to_string(),
        },
        price_history: PriceHistory {
            values: mock_values,
        },
    };

    let json_content = serde_json::to_string_pretty(&history_data).unwrap();
    fs::write(&file_path, json_content).await.unwrap();

    // Test simulation request that should use cache
    let request_start = Utc.with_ymd_and_hms(2025, 8, 10, 0, 0, 0).unwrap();
    let request_end = Utc.with_ymd_and_hms(2025, 8, 11, 0, 0, 0).unwrap();

    let overlapping_files =
        find_overlapping_history_files(&history_dir, request_start, request_end)
            .await
            .unwrap();

    assert_eq!(
        overlapping_files.len(),
        1,
        "Should find exactly one overlapping file"
    );

    // Test the complete cache check flow
    let cache_result = check_history_cache(quote_token, base_token, request_start, request_end)
        .await
        .unwrap();

    assert!(cache_result.is_some(), "Cache should return data");
}
