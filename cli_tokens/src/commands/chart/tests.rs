use super::*;
use bigdecimal::BigDecimal;
use chrono::Utc;
use std::path::PathBuf;

use common::prediction::{ConfidenceInterval, PredictionPoint};
use common::types::TokenPrice;

// === parse_size テスト ===

#[test]
fn test_parse_size_valid_formats() {
    let test_cases = vec![
        ("1200x800", (1200, 800)),
        ("800x600", (800, 600)),
        ("1920x1080", (1920, 1080)),
        ("640x480", (640, 480)),
        ("100x100", (100, 100)),
    ];

    for (input, expected) in test_cases {
        let result = parse_size(input);
        assert!(result.is_ok(), "Failed to parse size: {}", input);
        assert_eq!(result.unwrap(), expected);
    }
}

#[test]
fn test_parse_size_invalid_formats() {
    let invalid_cases = vec![
        "1200",      // 分割文字なし
        "1200x",     // 高さなし
        "x800",      // 幅なし
        "axb",       // 非数値
        "1200y800",  // 間違った分割文字
        "0x800",     // 幅0
        "1200x0",    // 高さ0
        "",          // 空文字
        "1200x800x", // 余分な部分
    ];

    for invalid_input in invalid_cases {
        let result = parse_size(invalid_input);
        assert!(result.is_err(), "Should fail for input: {}", invalid_input);
    }
}

// === extract_quote_token_from_path テスト ===

#[test]
fn test_extract_quote_token_from_path_valid() {
    let test_cases = vec![
        ("/path/wrap.near/token.json", Some("wrap.near")),
        ("/base/usdc.near/test.json", Some("usdc.near")),
        ("./wrap.near/token.json", Some("wrap.near")),
        ("wrap.near/token.json", Some("wrap.near")),
    ];

    for (input_path, expected) in test_cases {
        let path = PathBuf::from(input_path);
        let result = extract_quote_token_from_path(&path);
        assert_eq!(result, expected.map(|s| s.to_string()));
    }
}

#[test]
fn test_extract_quote_token_from_path_tokens_directory() {
    // tokensディレクトリの場合はNone
    let test_cases = vec![
        ("/path/tokens/token.json", None),
        ("./tokens/token.json", None),
        ("tokens/token.json", None),
    ];

    for (input_path, expected) in test_cases {
        let path = PathBuf::from(input_path);
        let result = extract_quote_token_from_path(&path);
        assert_eq!(result, expected);
    }
}

#[test]
fn test_extract_quote_token_from_path_edge_cases() {
    let test_cases = vec![
        ("/token.json", None), // ルートファイル
        ("token.json", None),  // 親ディレクトリなし
        ("/", None),           // ルートディレクトリ
    ];

    for (input_path, expected) in test_cases {
        let path = PathBuf::from(input_path);
        let result = extract_quote_token_from_path(&path);
        assert_eq!(result, expected);
    }
}

// === generate_output_filename テスト ===

#[test]
fn test_generate_output_filename_history() {
    let detected = DetectedFiles {
        history: Some(PathBuf::from("history.json")),
        prediction: None,
        token_name: "wrap.near".to_string(),
        quote_token: "wrap.near".to_string(),
    };

    let filename = generate_output_filename(&detected, &ChartType::History, false);
    assert_eq!(filename, "wrap.near_history.png");
}

#[test]
fn test_generate_output_filename_prediction() {
    let detected = DetectedFiles {
        history: None,
        prediction: Some(PathBuf::from("prediction.json")),
        token_name: "usdc.near".to_string(),
        quote_token: "wrap.near".to_string(),
    };

    let filename = generate_output_filename(&detected, &ChartType::Prediction, false);
    assert_eq!(filename, "usdc.near_prediction.png");

    let filename_with_confidence =
        generate_output_filename(&detected, &ChartType::Prediction, true);
    assert_eq!(
        filename_with_confidence,
        "usdc.near_prediction_with_confidence.png"
    );
}

#[test]
fn test_generate_output_filename_combined() {
    let detected = DetectedFiles {
        history: Some(PathBuf::from("history.json")),
        prediction: Some(PathBuf::from("prediction.json")),
        token_name: "test.token".to_string(),
        quote_token: "wrap.near".to_string(),
    };

    let filename = generate_output_filename(&detected, &ChartType::Combined, false);
    assert_eq!(filename, "test.token_combined.png");

    let filename_with_confidence = generate_output_filename(&detected, &ChartType::Combined, true);
    assert_eq!(
        filename_with_confidence,
        "test.token_combined_with_confidence.png"
    );
}

#[test]
fn test_generate_output_filename_auto() {
    // 履歴のみ
    let detected_history_only = DetectedFiles {
        history: Some(PathBuf::from("history.json")),
        prediction: None,
        token_name: "token1".to_string(),
        quote_token: "wrap.near".to_string(),
    };
    let filename = generate_output_filename(&detected_history_only, &ChartType::Auto, false);
    assert_eq!(filename, "token1_history.png");

    // 予測のみ
    let detected_prediction_only = DetectedFiles {
        history: None,
        prediction: Some(PathBuf::from("prediction.json")),
        token_name: "token2".to_string(),
        quote_token: "wrap.near".to_string(),
    };
    let filename = generate_output_filename(&detected_prediction_only, &ChartType::Auto, false);
    assert_eq!(filename, "token2_prediction.png");

    // 両方
    let detected_both = DetectedFiles {
        history: Some(PathBuf::from("history.json")),
        prediction: Some(PathBuf::from("prediction.json")),
        token_name: "token3".to_string(),
        quote_token: "wrap.near".to_string(),
    };
    let filename = generate_output_filename(&detected_both, &ChartType::Auto, false);
    assert_eq!(filename, "token3_combined.png");

    let filename_with_confidence = generate_output_filename(&detected_both, &ChartType::Auto, true);
    assert_eq!(
        filename_with_confidence,
        "token3_combined_with_confidence.png"
    );
}

// === calculate_time_range テスト ===

#[test]
fn test_calculate_time_range_history_only() {
    let now = Utc::now();
    let one_hour_ago = now - chrono::Duration::hours(1);
    let two_hours_ago = now - chrono::Duration::hours(2);

    let chart_data = ChartData {
        history: Some(vec![
            (two_hours_ago, 100.0),
            (one_hour_ago, 105.0),
            (now, 110.0),
        ]),
        predictions: None,
        token_name: "test".to_string(),
        quote_token: "wrap.near".to_string(),
        time_range: None,
    };

    let result = calculate_time_range(&chart_data);
    assert!(result.is_some());
    let (min_time, max_time) = result.unwrap();
    assert_eq!(min_time, two_hours_ago);
    assert_eq!(max_time, now);
}

#[test]
fn test_calculate_time_range_predictions_only() {
    let now = Utc::now();
    let future1 = now + chrono::Duration::hours(1);
    let future2 = now + chrono::Duration::hours(2);

    let predictions = vec![
        PredictionPoint {
            timestamp: now,
            value: TokenPrice::from_near_per_token(BigDecimal::from(100)),
            confidence_interval: None,
        },
        PredictionPoint {
            timestamp: future1,
            value: TokenPrice::from_near_per_token(BigDecimal::from(105)),
            confidence_interval: None,
        },
        PredictionPoint {
            timestamp: future2,
            value: TokenPrice::from_near_per_token(BigDecimal::from(110)),
            confidence_interval: None,
        },
    ];

    let chart_data = ChartData {
        history: None,
        predictions: Some(predictions),
        token_name: "test".to_string(),
        quote_token: "wrap.near".to_string(),
        time_range: None,
    };

    let result = calculate_time_range(&chart_data);
    assert!(result.is_some());
    let (min_time, max_time) = result.unwrap();
    assert_eq!(min_time, now);
    assert_eq!(max_time, future2);
}

#[test]
fn test_calculate_time_range_combined() {
    let now = Utc::now();
    let past = now - chrono::Duration::hours(1);
    let future = now + chrono::Duration::hours(1);

    let predictions = vec![
        PredictionPoint {
            timestamp: now,
            value: TokenPrice::from_near_per_token(BigDecimal::from(100)),
            confidence_interval: None,
        },
        PredictionPoint {
            timestamp: future,
            value: TokenPrice::from_near_per_token(BigDecimal::from(105)),
            confidence_interval: None,
        },
    ];

    let chart_data = ChartData {
        history: Some(vec![(past, 95.0)]),
        predictions: Some(predictions),
        token_name: "test".to_string(),
        quote_token: "wrap.near".to_string(),
        time_range: None,
    };

    let result = calculate_time_range(&chart_data);
    assert!(result.is_some());
    let (min_time, max_time) = result.unwrap();
    assert_eq!(min_time, past);
    assert_eq!(max_time, future);
}

#[test]
fn test_calculate_time_range_empty_data() {
    let chart_data = ChartData {
        history: None,
        predictions: None,
        token_name: "test".to_string(),
        quote_token: "wrap.near".to_string(),
        time_range: None,
    };

    let result = calculate_time_range(&chart_data);
    assert!(result.is_none());
}

// === calculate_value_range テスト ===

#[test]
fn test_calculate_value_range_history_only() {
    let now = Utc::now();
    let chart_data = ChartData {
        history: Some(vec![(now, 100.0), (now, 200.0), (now, 150.0)]),
        predictions: None,
        token_name: "test".to_string(),
        quote_token: "wrap.near".to_string(),
        time_range: None,
    };

    let result = calculate_value_range(&chart_data);
    assert!(result.is_ok());
    let (min_val, max_val) = result.unwrap();

    // パディング計算: range = 200 - 100 = 100, padding = 100 * 0.1 = 10
    assert_eq!(min_val, 90.0); // 100 - 10
    assert_eq!(max_val, 210.0); // 200 + 10
}

#[test]
fn test_calculate_value_range_with_confidence_intervals() {
    let now = Utc::now();
    let predictions = vec![
        PredictionPoint {
            timestamp: now,
            value: TokenPrice::from_near_per_token(BigDecimal::from(100)),
            confidence_interval: Some(ConfidenceInterval {
                lower: TokenPrice::from_near_per_token(BigDecimal::from(80)),
                upper: TokenPrice::from_near_per_token(BigDecimal::from(120)),
            }),
        },
        PredictionPoint {
            timestamp: now,
            value: TokenPrice::from_near_per_token(BigDecimal::from(110)),
            confidence_interval: Some(ConfidenceInterval {
                lower: TokenPrice::from_near_per_token(BigDecimal::from(90)),
                upper: TokenPrice::from_near_per_token(BigDecimal::from(130)),
            }),
        },
    ];

    let chart_data = ChartData {
        history: None,
        predictions: Some(predictions),
        token_name: "test".to_string(),
        quote_token: "wrap.near".to_string(),
        time_range: None,
    };

    let result = calculate_value_range(&chart_data);
    assert!(result.is_ok());
    let (min_val, max_val) = result.unwrap();

    // 信頼区間を含めた範囲: 80.0 - 130.0, range = 50, padding = 5
    assert_eq!(min_val, 75.0); // 80 - 5
    assert_eq!(max_val, 135.0); // 130 + 5
}

#[test]
fn test_calculate_value_range_empty_data() {
    let chart_data = ChartData {
        history: None,
        predictions: None,
        token_name: "test".to_string(),
        quote_token: "wrap.near".to_string(),
        time_range: None,
    };

    let result = calculate_value_range(&chart_data);
    assert!(result.is_err());
}

#[test]
fn test_calculate_value_range_single_value() {
    let now = Utc::now();
    let chart_data = ChartData {
        history: Some(vec![(now, 100.0)]),
        predictions: None,
        token_name: "test".to_string(),
        quote_token: "wrap.near".to_string(),
        time_range: None,
    };

    let result = calculate_value_range(&chart_data);
    assert!(result.is_ok());
    let (min_val, max_val) = result.unwrap();

    // 単一値の場合: range = 0, padding = 0, そのまま
    assert_eq!(min_val, 100.0);
    assert_eq!(max_val, 100.0);
}
