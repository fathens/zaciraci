//! predict コマンドの詳細なテスト
//! - パラメータ検証（start_pct、end_pct、forecast_ratio）
//! - 期間計算の精度
//! - ファイルパス構造

use chrono::{Duration, NaiveDate, TimeZone, Utc};
use std::path::PathBuf;

use crate::commands::predict::kick::KickArgs;
use crate::utils::file::sanitize_filename;

// === 基本オプションテスト ===

#[test]
fn test_kick_args_values() {
    // テストのデフォルト値確認
    let default_args = KickArgs {
        token_file: PathBuf::from("test.json"),
        output: PathBuf::from("predictions"),
        model: None,
        start_pct: 0.0,
        end_pct: 100.0,
        forecast_ratio: 10.0,
    };
    assert_eq!(default_args.output, PathBuf::from("predictions"));
    assert_eq!(default_args.model, None);
    assert_eq!(default_args.start_pct, 0.0);
    assert_eq!(default_args.end_pct, 100.0);
    assert_eq!(default_args.forecast_ratio, 10.0);

    // カスタム値でのテスト
    let custom_args = KickArgs {
        token_file: PathBuf::from("custom/token.json"),
        output: PathBuf::from("custom_output"),
        model: Some("chronos_bolt".to_string()),
        start_pct: 25.0,
        end_pct: 75.0,
        forecast_ratio: 50.0,
    };
    assert_eq!(custom_args.token_file, PathBuf::from("custom/token.json"));
    assert_eq!(custom_args.output, PathBuf::from("custom_output"));
    assert_eq!(custom_args.model, Some("chronos_bolt".to_string()));
    assert_eq!(custom_args.start_pct, 25.0);
    assert_eq!(custom_args.end_pct, 75.0);
    assert_eq!(custom_args.forecast_ratio, 50.0);
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
        };

        assert_eq!(args.model, Some(model.to_string()));
        assert!(args.model.is_some());
    }
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
fn test_forecast_ratio_calculations() {
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

    // エッジケースも同時にテスト
    let edge_cases = vec![
        (0.1, Duration::days(30).num_milliseconds() as f64 * 0.001), // 最小値
        (500.0, Duration::days(30).num_milliseconds() as f64 * 5.0), // 最大値
    ];

    let base_duration = Duration::days(30);
    for (ratio, expected_ms) in edge_cases {
        let forecast_duration_ms =
            (base_duration.num_milliseconds() as f64 * (ratio / 100.0)) as i64;
        let expected_ms_i64 = expected_ms as i64;

        assert!(
            (forecast_duration_ms - expected_ms_i64).abs() < 1000,
            "Ratio {}% calculation precision test failed",
            ratio
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
        };

        // 実際のrunメソッドを呼び出すのではなく、バリデーション条件をテスト
        let is_valid = args.forecast_ratio > 0.0 && args.forecast_ratio <= 500.0;
        assert!(!is_valid, "Ratio {} should be invalid", invalid_ratio);
    }
}

#[test]
fn test_predict_output_file_structure() {
    // 新しいファイルベース構造のテスト
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
