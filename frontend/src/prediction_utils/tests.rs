use super::*;
use chrono::{DateTime, NaiveDateTime, Utc};
use std::collections::HashMap;
use zaciraci_common::stats::ValueAtTime;

fn create_test_data() -> (Vec<ValueAtTime>, Vec<ValueAtTime>, Vec<ValueAtTime>) {
    // トレーニングデータ（5ポイント）
    let training_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S")
                .expect("有効な日付形式"),
            value: 1.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S")
                .expect("有効な日付形式"),
            value: 1.1,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-03 00:00:00", "%Y-%m-%d %H:%M:%S")
                .expect("有効な日付形式"),
            value: 1.05,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-04 00:00:00", "%Y-%m-%d %H:%M:%S")
                .expect("有効な日付形式"),
            value: 1.15,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-05 00:00:00", "%Y-%m-%d %H:%M:%S")
                .expect("有効な日付形式"),
            value: 1.2,
        },
    ];

    // テストデータ（3ポイント）
    let test_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-06 00:00:00", "%Y-%m-%d %H:%M:%S")
                .expect("有効な日付形式"),
            value: 1.25,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-07 00:00:00", "%Y-%m-%d %H:%M:%S")
                .expect("有効な日付形式"),
            value: 1.3,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-08 00:00:00", "%Y-%m-%d %H:%M:%S")
                .expect("有効な日付形式"),
            value: 1.35,
        },
    ];

    // 予測データ（テストデータの最後の点から開始して3ポイント）
    let forecast_data = vec![
        // 接続点：テストデータの最後の点と同じ
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-08 00:00:00", "%Y-%m-%d %H:%M:%S")
                .expect("有効な日付形式"),
            value: 1.35, // テストデータの最後の値と同じ
        },
        // 予測点
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-09 00:00:00", "%Y-%m-%d %H:%M:%S")
                .expect("有効な日付形式"),
            value: 1.4,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-10 00:00:00", "%Y-%m-%d %H:%M:%S")
                .expect("有効な日付形式"),
            value: 1.45,
        },
    ];

    (training_data, test_data, forecast_data)
}

#[test]
fn test_data_series_separation() {
    let (training_data, test_data, forecast_data) = create_test_data();

    log::debug!("=== データ系列分離テスト ===");

    // generate_prediction_chart_svg内部のロジックを直接テスト
    let mut all_actual_data = Vec::new();
    all_actual_data.extend(training_data.to_vec());

    // テストデータの最後の点以外を追加（重複を避けるため）
    if !test_data.is_empty() {
        let test_data_without_last = &test_data[..test_data.len() - 1];
        all_actual_data.extend(test_data_without_last.to_vec());
    }

    log::debug!("元のtraining_data: {} points", training_data.len());
    log::debug!("元のtest_data: {} points", test_data.len());
    log::debug!("結合後のall_actual_data: {} points", all_actual_data.len());
    log::debug!("forecast_data: {} points", forecast_data.len());

    // 期待値チェック
    let expected_actual_points = training_data.len() + test_data.len() - 1;
    assert_eq!(
        all_actual_data.len(),
        expected_actual_points,
        "実際データの点数が期待値と異なります"
    );

    // タイムスタンプの重複チェック
    let actual_times: Vec<_> = all_actual_data.iter().map(|v| v.time).collect();
    let forecast_times: Vec<_> = forecast_data.iter().map(|v| v.time).collect();

    log::debug!("=== タイムスタンプ重複チェック ===");
    for forecast_time in &forecast_times {
        let overlap_count = actual_times
            .iter()
            .filter(|&&t| t == *forecast_time)
            .count();
        if overlap_count > 0 {
            log::debug!(
                "⚠️  重複発見: {:?} が実際データにも{}回存在",
                forecast_time,
                overlap_count
            );
        } else {
            log::debug!("✅ {:?} は重複なし", forecast_time);
        }
    }

    log::debug!("✅ データ系列分離テスト完了");
}

#[test]
fn test_forecast_shape_preservation() {
    log::debug!("=== 予測データ形状保持テスト ===");

    // サンプルの予測データ（変動パターンを持つ）
    let original_forecast_values = vec![100.0, 105.0, 95.0, 110.0, 90.0];
    let last_test_value = 200.0; // テストデータの最後の値
    let first_forecast_value = original_forecast_values[0]; // 100.0

    // 差分調整の計算（修正後の手法）
    let offset = last_test_value - first_forecast_value; // 200.0 - 100.0 = 100.0
    let adjusted_values: Vec<f64> = original_forecast_values
        .iter()
        .map(|&v| v + offset)
        .collect();

    log::debug!("元の予測値: {:?}", original_forecast_values);
    log::debug!("調整後の予測値: {:?}", adjusted_values);
    log::debug!("差分オフセット: {}", offset);

    // 形状保持の検証：隣接する値の差分が保持されているか
    for i in 1..original_forecast_values.len() {
        let original_diff = original_forecast_values[i] - original_forecast_values[i - 1];
        let adjusted_diff = adjusted_values[i] - adjusted_values[i - 1];

        assert!(
            (original_diff - adjusted_diff).abs() < 1e-10,
            "形状が保持されていません: index {} で元の差分 {} vs 調整後の差分 {}",
            i,
            original_diff,
            adjusted_diff
        );
    }

    // レベル調整の検証：最初の値が正しく調整されているか
    assert!(
        (adjusted_values[0] - last_test_value).abs() < 1e-10,
        "レベル調整が正しくありません: 期待値 {} vs 実際値 {}",
        last_test_value,
        adjusted_values[0]
    );

    // 変動の検証：すべての値が同じでないことを確認（直線化していない）
    let all_same = adjusted_values
        .windows(2)
        .all(|w| (w[0] - w[1]).abs() < 1e-10);
    assert!(!all_same, "予測が直線化されています（すべての値が同じ）");

    log::debug!("✅ 形状保持テスト完了");
    log::debug!("✅ レベル調整テスト完了");
    log::debug!("✅ 非直線化テスト完了");
}

#[test]
fn test_problematic_multiplication_approach() {
    log::debug!("=== 問題のある乗算手法のテスト ===");

    // サンプルの予測データ（変動パターンを持つ）
    let original_forecast_values = vec![100.0, 105.0, 95.0, 110.0, 90.0];
    let last_test_value = 200.0;
    let first_forecast_value = original_forecast_values[0];

    // 乗算手法（修正前の問題のある手法）
    let correction_factor = last_test_value / first_forecast_value; // 2.0
    let multiplied_values: Vec<f64> = original_forecast_values
        .iter()
        .map(|&v| v * correction_factor)
        .collect();

    // 差分手法（修正後の正しい手法）
    let offset = last_test_value - first_forecast_value; // 100.0
    let adjusted_values: Vec<f64> = original_forecast_values
        .iter()
        .map(|&v| v + offset)
        .collect();

    log::debug!("元の予測値: {:?}", original_forecast_values);
    log::debug!("乗算調整後: {:?}", multiplied_values);
    log::debug!("差分調整後: {:?}", adjusted_values);

    // 変動パターンの比較
    for i in 1..original_forecast_values.len() {
        let original_diff = original_forecast_values[i] - original_forecast_values[i - 1];
        let multiplied_diff = multiplied_values[i] - multiplied_values[i - 1];
        let adjusted_diff = adjusted_values[i] - adjusted_values[i - 1];

        // 差分手法では形状が保持される
        assert!(
            (original_diff - adjusted_diff).abs() < 1e-10,
            "差分手法で形状が保持されていません"
        );

        // 乗算手法では形状が変わる（スケールされる）
        let expected_multiplied_diff = original_diff * correction_factor;
        assert!(
            (multiplied_diff - expected_multiplied_diff).abs() < 1e-10,
            "乗算手法の計算が正しくありません"
        );

        // 乗算手法は元の形状を保持しない（スケールする）
        if original_diff != 0.0 {
            assert!(
                (original_diff - multiplied_diff).abs() > 1e-10,
                "乗算手法が意図せず形状を保持しています（テストデータに問題あり）"
            );
        }
    }

    log::debug!("✅ 乗算手法は形状をスケールすることを確認");
    log::debug!("✅ 差分手法は形状を保持することを確認");
}

#[test]
fn test_validate_and_normalize_data_with_valid_data() {
    log::debug!("=== validate_and_normalize_data テスト（有効データ） ===");

    // 10個のテストデータを作成
    let mut test_data = Vec::new();
    for i in 0..10 {
        test_data.push(ValueAtTime {
            time: NaiveDateTime::parse_from_str(
                &format!("2025-06-{:02} 00:00:00", i + 1),
                "%Y-%m-%d %H:%M:%S",
            )
            .unwrap(),
            value: 100.0 + i as f64,
        });
    }

    // 関数の実行
    let result = validate_and_normalize_data(&test_data);

    // 結果の検証
    assert!(
        result.is_ok(),
        "validate_and_normalize_data should succeed with valid data"
    );
    let normalized_data = result.unwrap();

    // データ数が保持されていることを確認
    assert_eq!(normalized_data.len(), test_data.len());

    log::debug!("✅ 有効データのテスト完了");
}

#[test]
fn test_validate_and_normalize_data_with_empty_data() {
    log::debug!("=== validate_and_normalize_data テスト（空データ） ===");

    let empty_data = vec![];

    // 関数の実行
    let result = validate_and_normalize_data(&empty_data);

    // 結果の検証（空データはエラーになるべき）
    assert!(
        result.is_err(),
        "validate_and_normalize_data should fail with empty data"
    );

    if let Err(error) = result {
        match error {
            PredictionError::DataNotFound => {
                log::debug!("✅ 期待通りDataNotFoundエラーが発生");
            }
            _ => {
                panic!("予期しないエラータイプ: {:?}", error);
            }
        }
    }

    log::debug!("✅ 空データのテスト完了");
}

#[test]
fn test_validate_and_normalize_data_insufficient_data() {
    log::debug!("=== validate_and_normalize_data テスト（データ不足） ===");

    // 3個のデータ（最小要件の4個未満）
    let insufficient_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            value: 1.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            value: 1.1,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-03 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            value: 1.2,
        },
    ];

    // 関数の実行
    let result = validate_and_normalize_data(&insufficient_data);

    // 結果の検証（データ不足はエラーになるべき）
    assert!(
        result.is_err(),
        "validate_and_normalize_data should fail with insufficient data"
    );

    if let Err(error) = result {
        match error {
            PredictionError::InsufficientData => {
                log::debug!("✅ 期待通りInsufficientDataエラーが発生");
            }
            _ => panic!("予期しないエラータイプ: {:?}", error),
        }
    }

    log::debug!("✅ データ不足のテスト完了");
}

#[test]
fn test_validate_and_normalize_data_invalid_values() {
    log::debug!("=== validate_and_normalize_data テスト（無効な値） ===");

    // 無効な値を含むデータ（負の値、無限大、NaN）
    let invalid_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            value: 1.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            value: -1.0, // 負の値
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-03 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            value: 1.2,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-04 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            value: f64::INFINITY, // 無限大
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-05 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            value: 1.5,
        },
    ];

    // 関数の実行
    let result = validate_and_normalize_data(&invalid_data);

    // 結果の検証（無効な値はエラーになるべき）
    assert!(
        result.is_err(),
        "validate_and_normalize_data should fail with invalid values"
    );

    if let Err(error) = result {
        match error {
            PredictionError::InvalidData(_) => {
                log::debug!("✅ 期待通りInvalidDataエラーが発生");
            }
            _ => panic!("予期しないエラータイプ: {:?}", error),
        }
    }

    log::debug!("✅ 無効な値のテスト完了");
}

#[test]
fn test_validate_and_normalize_data_time_order_validation() {
    log::debug!("=== validate_and_normalize_data テスト（時間順序検証） ===");

    // 時間順序が間違っているデータ
    let unordered_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            value: 1.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-03 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(), // 順序が逆
            value: 1.1,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(), // 前より古い
            value: 1.2,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-04 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            value: 1.3,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-05 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            value: 1.4,
        },
    ];

    // 関数の実行
    let result = validate_and_normalize_data(&unordered_data);

    // 結果の検証（時間順序エラーになるべき）
    assert!(
        result.is_err(),
        "validate_and_normalize_data should fail with unordered timestamps"
    );

    if let Err(error) = result {
        match error {
            PredictionError::InvalidData(msg) => {
                assert!(
                    msg.contains("Time series order error"),
                    "Error message should mention time series order"
                );
                log::debug!("✅ 期待通り時間順序エラーが発生: {}", msg);
            }
            _ => panic!("予期しないエラータイプ: {:?}", error),
        }
    }

    log::debug!("✅ 時間順序検証のテスト完了");
}

#[test]
fn test_create_prediction_request_with_split_data() {
    log::debug!("=== create_prediction_request テスト（分割済みデータ） ===");

    // 10個のテストデータを作成
    let mut normalized_data = Vec::new();
    for i in 0..10 {
        normalized_data.push(ValueAtTime {
            time: NaiveDateTime::parse_from_str(
                &format!("2025-06-{:02} 00:00:00", i + 1),
                "%Y-%m-%d %H:%M:%S",
            )
            .unwrap(),
            value: 100.0 + i as f64,
        });
    }

    // データを分割
    let (training_data, test_data) = split_data_for_prediction(&normalized_data).unwrap();
    let model_name = "test_model".to_string();

    // 関数の実行
    let result = create_prediction_request(training_data, test_data, model_name);

    // 結果の検証
    assert!(
        result.is_ok(),
        "create_prediction_request should succeed with split data"
    );
    let prediction_request = result.unwrap();

    // リクエストデータの検証
    assert_eq!(prediction_request.timestamp.len(), training_data.len());
    assert_eq!(prediction_request.values.len(), training_data.len());

    log::debug!("✅ 分割済みデータのテスト完了");
}

#[test]
fn test_split_data_for_prediction_with_valid_data() {
    log::debug!("=== split_data_for_prediction テスト（有効データ） ===");

    // 10個のテストデータを作成（90:10分割に十分な量）
    let mut test_data = Vec::new();
    for i in 0..10 {
        test_data.push(ValueAtTime {
            time: NaiveDateTime::parse_from_str(
                &format!("2025-06-{:02} 00:00:00", i + 1),
                "%Y-%m-%d %H:%M:%S",
            )
            .unwrap(),
            value: 100.0 + i as f64,
        });
    }

    // 関数の実行
    let result = split_data_for_prediction(&test_data);

    // 結果の検証
    assert!(
        result.is_ok(),
        "split_data_for_prediction should succeed with valid data"
    );
    let (training_data, test_data_slice) = result.unwrap();

    // データ分割の検証（90:10）
    let expected_training_size = (test_data.len() as f64 * 0.9) as usize; // 9
    let expected_test_size = test_data.len() - expected_training_size; // 1

    assert_eq!(training_data.len(), expected_training_size);
    assert_eq!(test_data_slice.len(), expected_test_size);

    // データの連続性の確認
    assert_eq!(
        training_data[training_data.len() - 1]
            .time
            .format("%Y-%m-%d")
            .to_string(),
        "2025-06-09"
    );
    assert_eq!(
        test_data_slice[0].time.format("%Y-%m-%d").to_string(),
        "2025-06-10"
    );

    log::debug!("✅ 有効データの分割テスト完了");
}

#[test]
fn test_split_data_for_prediction_insufficient_data() {
    log::debug!("=== split_data_for_prediction テスト（データ不足） ===");

    // 2個のデータ（分割後のtraining_dataが2個未満になる: split_point = 1）
    let insufficient_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            value: 1.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            value: 1.1,
        },
    ];

    // 関数の実行
    let result = split_data_for_prediction(&insufficient_data);

    // 結果の検証（データ不足はエラーになるべき）
    assert!(
        result.is_err(),
        "split_data_for_prediction should fail with insufficient data"
    );

    if let Err(error) = result {
        match error {
            PredictionError::InsufficientData => {
                log::debug!("✅ 期待通りInsufficientDataエラーが発生");
            }
            _ => panic!("予期しないエラータイプ: {:?}", error),
        }
    }

    log::debug!("✅ データ不足のテスト完了");
}

#[test]
fn test_split_data_for_prediction_three_items_valid() {
    log::debug!("=== split_data_for_prediction テスト（3個データ有効） ===");

    // 3個のデータ（90:10分割で2:1になる）
    let three_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            value: 1.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            value: 1.1,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-03 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            value: 1.2,
        },
    ];

    // 関数の実行
    let result = split_data_for_prediction(&three_data);

    // 結果の検証
    assert!(
        result.is_ok(),
        "split_data_for_prediction should succeed with 3 items"
    );
    let (training_data, test_data) = result.unwrap();

    // データ分割の検証
    assert_eq!(training_data.len(), 2); // 90% = 2.7 -> 2
    assert_eq!(test_data.len(), 1); // 残り = 1

    // 値の確認
    assert_eq!(training_data[0].value, 1.0);
    assert_eq!(training_data[1].value, 1.1);
    assert_eq!(test_data[0].value, 1.2);

    log::debug!("✅ 3個データ有効のテスト完了");
}

#[test]
fn test_split_data_for_prediction_minimum_valid_data() {
    log::debug!("=== split_data_for_prediction テスト（最小有効データ） ===");

    // 4個のデータ（90:10分割で3:1になる）
    let minimum_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            value: 1.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            value: 1.1,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-03 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            value: 1.2,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-04 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            value: 1.3,
        },
    ];

    // 関数の実行
    let result = split_data_for_prediction(&minimum_data);

    // 結果の検証
    assert!(
        result.is_ok(),
        "split_data_for_prediction should succeed with minimum valid data"
    );
    let (training_data, test_data) = result.unwrap();

    // データ分割の検証
    assert_eq!(training_data.len(), 3); // 90% = 3.6 -> 3
    assert_eq!(test_data.len(), 1); // 残り = 1

    // 値の確認
    assert_eq!(training_data[0].value, 1.0);
    assert_eq!(training_data[1].value, 1.1);
    assert_eq!(training_data[2].value, 1.2);
    assert_eq!(test_data[0].value, 1.3);

    log::debug!("✅ 最小有効データのテスト完了");
}

#[test]
fn test_validate_prediction_response_valid_data() {
    log::debug!("=== validate_prediction_response テスト（有効データ） ===");

    let forecast_values = vec![1.0, 1.1, 1.2];
    let forecast_timestamp = vec![
        DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            Utc,
        ),
        DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            Utc,
        ),
        DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2025-06-03 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            Utc,
        ),
    ];

    let result = validate_prediction_response(&forecast_values, &forecast_timestamp);
    assert!(
        result.is_ok(),
        "Valid prediction response should pass validation"
    );

    log::debug!("✅ 有効データの検証テスト完了");
}

#[test]
fn test_validate_prediction_response_empty_values() {
    log::debug!("=== validate_prediction_response テスト（空の予測値） ===");

    let forecast_values = vec![];
    let forecast_timestamp = vec![DateTime::<Utc>::from_naive_utc_and_offset(
        NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
        Utc,
    )];

    let result = validate_prediction_response(&forecast_values, &forecast_timestamp);
    assert!(
        result.is_err(),
        "Empty forecast values should fail validation"
    );

    if let Err(error) = result {
        match error {
            PredictionError::InvalidData(msg) => {
                assert!(
                    msg.contains("empty"),
                    "Error message should mention empty data"
                );
                log::debug!("✅ 期待通りのエラーメッセージ: {}", msg);
            }
            _ => panic!("予期しないエラータイプ: {:?}", error),
        }
    }

    log::debug!("✅ 空の予測値テスト完了");
}

#[test]
fn test_validate_prediction_response_length_mismatch() {
    log::debug!("=== validate_prediction_response テスト（長さ不一致） ===");

    let forecast_values = vec![1.0, 1.1];
    let forecast_timestamp = vec![
        DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            Utc,
        ),
        DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            Utc,
        ),
        DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2025-06-03 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            Utc,
        ),
    ];

    let result = validate_prediction_response(&forecast_values, &forecast_timestamp);
    assert!(result.is_err(), "Mismatched lengths should fail validation");

    if let Err(error) = result {
        match error {
            PredictionError::InvalidData(msg) => {
                assert!(
                    msg.contains("does not match"),
                    "Error message should mention length mismatch"
                );
                log::debug!("✅ 期待通りのエラーメッセージ: {}", msg);
            }
            _ => panic!("予期しないエラータイプ: {:?}", error),
        }
    }

    log::debug!("✅ 長さ不一致テスト完了");
}

#[test]
fn test_transform_forecast_data_with_test_data() {
    log::debug!("=== transform_forecast_data テスト（テストデータあり） ===");

    let forecast_values = vec![100.0, 105.0, 95.0];
    let forecast_timestamp = vec![
        DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2025-06-04 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            Utc,
        ),
        DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2025-06-05 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            Utc,
        ),
        DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2025-06-06 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            Utc,
        ),
    ];

    let result =
        transform_forecast_data(&forecast_values, &forecast_timestamp).expect("変換に失敗しました");

    // 結果の検証
    assert_eq!(result.len(), 3); // 予測点のみ（接続点なし）

    // 修正後：オフセット調整なし、元の予測値がそのまま使用される
    assert_eq!(result[0].value, 100.0); // 最初の予測値
    assert_eq!(result[1].value, 105.0); // 2番目の予測値
    assert_eq!(result[2].value, 95.0); // 3番目の予測値

    // 形状保持の検証
    let original_diff_1_2 = forecast_values[1] - forecast_values[0]; // 5.0
    let adjusted_diff_1_2 = result[1].value - result[0].value;
    assert!(
        (original_diff_1_2 - adjusted_diff_1_2).abs() < 1e-10,
        "形状が保持されていません"
    );

    log::debug!("✅ テストデータありの変換テスト完了");
}

#[test]
fn test_transform_forecast_data_success() {
    log::debug!("=== transform_forecast_data テスト（正常変換） ===");

    let forecast_values = vec![100.0, 105.0, 95.0];
    let forecast_timestamp = vec![
        DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            Utc,
        ),
        DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            Utc,
        ),
        DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2025-06-03 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            Utc,
        ),
    ];

    let result = transform_forecast_data(&forecast_values, &forecast_timestamp);

    // 結果の検証（正常に変換されるべき）
    assert!(
        result.is_ok(),
        "transform_forecast_data should succeed with valid data"
    );
    let forecast_data = result.unwrap();
    assert_eq!(forecast_data.len(), 3);
    assert_eq!(forecast_data[0].value, 100.0);
    assert_eq!(forecast_data[1].value, 105.0);
    assert_eq!(forecast_data[2].value, 95.0);

    log::debug!("✅ 正常変換テスト完了");
}

#[test]
fn test_transform_forecast_data_empty_forecast_values() {
    log::debug!("=== transform_forecast_data テスト（空の予測値） ===");

    let forecast_values = vec![];
    let forecast_timestamp = vec![DateTime::<Utc>::from_naive_utc_and_offset(
        NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
        Utc,
    )];

    // 空の予測値なのでエラーが返されることを期待
    let result = transform_forecast_data(&forecast_values, &forecast_timestamp);
    match result {
        Ok(_) => panic!("空の予測値でエラーが発生しませんでした"),
        Err(e) => match e {
            PredictionError::InvalidData(msg) => {
                assert!(msg.contains("予測値が空です"));
                log::debug!("✅ 期待通りのエラーメッセージ: {}", msg);
            }
            _ => panic!("予期しないエラータイプ: {:?}", e),
        },
    }

    log::debug!("✅ 空の予測値テスト完了");
}

#[test]
fn test_transform_forecast_data_length_mismatch() {
    log::debug!("=== transform_forecast_data テスト（長さの不一致） ===");

    let forecast_values = vec![100.0, 105.0, 95.0];
    let forecast_timestamp = vec![
        DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            Utc,
        ),
        DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            Utc,
        ),
    ];

    // 予測値とタイムスタンプの長さが一致しないのでエラーが返されることを期待
    let result = transform_forecast_data(&forecast_values, &forecast_timestamp);
    match result {
        Ok(_) => panic!("長さの不一致でエラーが発生しませんでした"),
        Err(e) => match e {
            PredictionError::InvalidData(msg) => {
                assert!(msg.contains("予測値の数(3)と予測タイムスタンプの数(2)が一致しません"));
                log::debug!("✅ 期待通りのエラーメッセージ: {}", msg);
            }
            _ => panic!("予期しないエラータイプ: {:?}", e),
        },
    }

    log::debug!("✅ 長さ不一致テスト完了");
}

#[test]
fn test_create_prediction_result() {
    log::debug!("=== create_prediction_result テスト ===");

    let forecast_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            value: 100.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            value: 105.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-03 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            value: 110.0, // 最後の値（predicted_priceになる）
        },
    ];

    let chart_svg = "<svg>test chart</svg>".to_string();

    let mut metrics = HashMap::new();
    metrics.insert("MAPE".to_string(), 15.0); // 従来のMAPE
    metrics.insert("NORMALIZED_MAPE".to_string(), 15.0); // 15%の正規化MAPE -> 85%の精度
    metrics.insert("RMSE".to_string(), 2.5);

    let result =
        create_prediction_result(forecast_data.clone(), chart_svg.clone(), metrics.clone());

    // 結果の検証
    assert_eq!(result.predicted_price, 110.0); // 最後の予測値
    assert_eq!(result.accuracy, 85.0); // 100 - 15 = 85%
    assert_eq!(result.chart_svg.unwrap(), chart_svg);
    assert_eq!(result.metrics.get("MAPE").unwrap(), &15.0);
    assert_eq!(result.metrics.get("RMSE").unwrap(), &2.5);
    assert_eq!(result.forecast_data.len(), 3);

    log::debug!("✅ PredictionResult作成テスト完了");
}

#[test]
fn test_create_prediction_result_empty_forecast() {
    log::debug!("=== create_prediction_result テスト（空の予測データ） ===");

    let forecast_data = vec![]; // 空の予測データ
    let chart_svg = "<svg>test chart</svg>".to_string();
    let metrics = HashMap::new();

    let result = create_prediction_result(forecast_data, chart_svg, metrics);

    // 空の場合の検証
    assert_eq!(result.predicted_price, 0.0); // デフォルト値
    assert_eq!(result.accuracy, 100.0); // NORMALIZED_MAPEがない場合は100%

    log::debug!("✅ 空の予測データテスト完了");
}

#[test]
fn test_generate_prediction_chart_svg_basic() {
    log::debug!("=== generate_prediction_chart_svg 基本テスト ===");

    let (training_data, test_data, forecast_data) = create_test_data();

    // 実際データ（training + test）
    let mut actual_data = training_data.clone();
    actual_data.extend(test_data);

    let result = generate_prediction_chart_svg(&actual_data, &forecast_data);

    assert!(result.is_ok(), "SVG生成は成功するべき");
    let svg = result.unwrap();

    // SVGの基本構造を検証
    assert!(svg.contains("<svg"), "SVG開始タグが含まれるべき");
    assert!(svg.contains("</svg>"), "SVG終了タグが含まれるべき");
    assert!(
        svg.contains("実際の価格"),
        "実際データの系列名が含まれるべき"
    );
    assert!(svg.contains("予測価格"), "予測データの系列名が含まれるべき");
    assert!(svg.contains("価格予測"), "チャートタイトルが含まれるべき");

    log::debug!("✅ 基本SVG生成テスト完了");
}

#[test]
fn test_generate_prediction_chart_svg_empty_actual_data() {
    log::debug!("=== generate_prediction_chart_svg 空の実際データテスト ===");

    let (_, _, forecast_data) = create_test_data();
    let empty_actual_data = vec![];

    let result = generate_prediction_chart_svg(&empty_actual_data, &forecast_data);

    assert!(result.is_err(), "空の実際データはエラーになるべき");

    log::debug!("✅ 空の実際データテスト完了");
}

#[test]
fn test_generate_prediction_chart_svg_empty_forecast_data() {
    log::debug!("=== generate_prediction_chart_svg 空の予測データテスト ===");

    let (training_data, test_data, _) = create_test_data();
    let mut actual_data = training_data.clone();
    actual_data.extend(test_data);
    let empty_forecast_data = vec![];

    let result = generate_prediction_chart_svg(&actual_data, &empty_forecast_data);

    assert!(result.is_err(), "空の予測データはエラーになるべき");

    log::debug!("✅ 空の予測データテスト完了");
}

#[test]
fn test_generate_prediction_chart_svg_single_point() {
    log::debug!("=== generate_prediction_chart_svg 単一ポイントテスト ===");

    let actual_data = vec![ValueAtTime {
        time: NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S")
            .expect("有効な日付形式"),
        value: 100.0,
    }];

    let forecast_data = vec![ValueAtTime {
        time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S")
            .expect("有効な日付形式"),
        value: 110.0,
    }];

    let result = generate_prediction_chart_svg(&actual_data, &forecast_data);

    assert!(result.is_ok(), "単一ポイントでもSVG生成は成功するべき");
    let svg = result.unwrap();

    assert!(svg.contains("<svg"), "SVG開始タグが含まれるべき");
    assert!(svg.contains("</svg>"), "SVG終了タグが含まれるべき");

    log::debug!("✅ 単一ポイントテスト完了");
}

#[test]
fn test_generate_prediction_chart_svg_extreme_values() {
    log::debug!("=== generate_prediction_chart_svg 極端な値テスト ===");

    let actual_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S")
                .expect("有効な日付形式"),
            value: 0.001,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S")
                .expect("有効な日付形式"),
            value: 1_000_000.0,
        },
    ];

    let forecast_data = vec![ValueAtTime {
        time: NaiveDateTime::parse_from_str("2025-06-03 00:00:00", "%Y-%m-%d %H:%M:%S")
            .expect("有効な日付形式"),
        value: 500_000.0,
    }];

    let result = generate_prediction_chart_svg(&actual_data, &forecast_data);

    assert!(result.is_ok(), "極端な値でもSVG生成は成功するべき");
    let svg = result.unwrap();

    assert!(svg.contains("<svg"), "SVG開始タグが含まれるべき");
    assert!(svg.contains("</svg>"), "SVG終了タグが含まれるべき");

    log::debug!("✅ 極端な値テスト完了");
}
