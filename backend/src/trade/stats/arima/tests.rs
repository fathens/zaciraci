use crate::trade::stats::Point;
use crate::trade::stats::arima::*;
use bigdecimal::BigDecimal;
use chrono::{Duration, TimeZone, Utc};
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use std::str::FromStr;

// 乱数を使用したノイズ生成関数
fn simple_noise(i: usize, range: f64) -> f64 {
    // シード付き乱数生成器を使用（テストの再現性のため）
    let mut rng = SmallRng::seed_from_u64(i as u64);
    let random_value = rng.random::<f64>();
    random_value * range * 2.0 - range
}

// テスト用のヘルパー関数：テストデータの生成
fn create_test_points(count: usize, start_value: f64, increment: f64) -> Vec<Point> {
    let base_time = Utc.timestamp_opt(1_600_000_000, 0).unwrap().naive_utc();
    let mut points = Vec::with_capacity(count);

    for i in 0..count {
        let value = start_value + (i as f64 * increment);
        let rate = BigDecimal::from_str(&value.to_string()).unwrap();
        let timestamp = base_time + Duration::hours(i as i64);

        points.push(Point { rate, timestamp });
    }

    points
}

// テスト用のヘルパー関数：ノイズを含むテストデータの生成
fn create_random_test_points(count: usize, base_value: f64, noise_range: f64) -> Vec<Point> {
    let base_time = Utc.timestamp_opt(1_600_000_000, 0).unwrap().naive_utc();
    let mut points = Vec::with_capacity(count);

    for i in 0..count {
        // ベース値 + 乱数ノイズ
        let noise = simple_noise(i, noise_range);
        let value = base_value + noise;
        let rate = BigDecimal::from_str(&value.to_string()).unwrap();
        let timestamp = base_time + Duration::hours(i as i64);

        points.push(Point { rate, timestamp });
    }

    points
}

#[cfg(test)]
mod conversion_tests {
    use super::*;

    #[test]
    fn test_convert_to_f64() {
        // 正常な変換
        let decimal = BigDecimal::from_str("123.456").unwrap();
        let result = convert_to_f64(&decimal);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 123.456);

        // 0の変換
        let decimal = BigDecimal::from_str("0").unwrap();
        let result = convert_to_f64(&decimal);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0.0);

        // 負の値の変換
        let decimal = BigDecimal::from_str("-987.654").unwrap();
        let result = convert_to_f64(&decimal);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), -987.654);
    }

    #[test]
    fn test_convert_from_f64() {
        // 正常な変換
        let value = 123.456;
        let result = convert_from_f64(value);
        assert!(result.is_ok());

        // 浮動小数点精度の問題を考慮して、値が近いかどうかを確認
        let decimal_result = result.unwrap();
        let decimal_str = decimal_result.to_string();
        assert!(decimal_str.starts_with("123.456"));

        // 0の変換
        let value = 0.0;
        let result = convert_from_f64(value);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(), "0");

        // 負の値の変換
        let value = -987.654;
        let result = convert_from_f64(value);
        assert!(result.is_ok());

        // 浮動小数点精度の問題を考慮して数値的な近さを確認
        let decimal_result = result.unwrap();
        let original = BigDecimal::from_str("-987.654").unwrap();
        let diff = (decimal_result.clone() - original).abs();
        let epsilon = BigDecimal::from_str("0.0001").unwrap();

        assert!(
            diff < epsilon,
            "負の値の変換結果が期待から離れています: expected=-987.654, got={}",
            decimal_result
        );
    }

    #[test]
    fn test_roundtrip_conversion() {
        // BigDecimal -> f64 -> BigDecimal の往復変換
        let original = BigDecimal::from_str("123.45").unwrap();
        let as_f64 = convert_to_f64(&original).unwrap();
        let back_to_decimal = convert_from_f64(as_f64).unwrap();

        // 精度の問題で完全一致しない場合があるので、数値として近いかを確認
        let diff = &back_to_decimal - &original;
        let diff_abs = diff.abs();
        let epsilon = BigDecimal::from_str("0.0001").unwrap(); // 許容誤差

        assert!(
            diff_abs < epsilon,
            "差が許容誤差を超えています: original={}, converted={}, diff={}",
            original,
            back_to_decimal,
            diff
        );
    }
}

#[cfg(test)]
mod feature_generation_tests {
    use super::*;

    #[test]
    fn test_create_lag_features_basic() {
        // 10個のデータポイント、ラグ数 3
        let points = create_test_points(10, 1.0, 1.0); // 1.0, 2.0, 3.0, ..., 10.0
        let lag_count = 3;

        let result = create_lag_features(&points, lag_count);
        assert!(result.is_ok());

        let (features, targets) = result.unwrap();

        // 10個のデータポイントから、ラグ数3を使うと、7つのサンプルが生成される
        assert_eq!(features.len(), 7);
        assert_eq!(targets.len(), 7);

        // 最初の特徴量は [1.0, 2.0, 3.0] で、ターゲットは 4.0
        let first_feature = &features[0];
        assert_eq!(first_feature.len(), 3);
        assert_eq!(first_feature[0], 1.0);
        assert_eq!(first_feature[1], 2.0);
        assert_eq!(first_feature[2], 3.0);
        assert_eq!(targets[0], 4.0);

        // 最後の特徴量は [7.0, 8.0, 9.0] で、ターゲットは 10.0
        let last_feature = &features[6];
        assert_eq!(last_feature[0], 7.0);
        assert_eq!(last_feature[1], 8.0);
        assert_eq!(last_feature[2], 9.0);
        assert_eq!(targets[6], 10.0);
    }

    #[test]
    fn test_create_lag_features_insufficient_data() {
        // 3個のデータポイント、ラグ数 4 (データが不足)
        let points = create_test_points(3, 1.0, 1.0);
        let lag_count = 4;

        let result = create_lag_features(&points, lag_count);
        assert!(result.is_err());

        // エラーメッセージを確認
        match result {
            Err(e) => {
                let error_string = e.to_string();
                assert!(error_string.contains("insufficient data points"));
            }
            Ok(_) => panic!("Expected error for insufficient data"),
        }
    }

    #[test]
    fn test_generate_future_features() {
        // 10個のデータポイント、ラグ数 3
        let points = create_test_points(10, 1.0, 1.0);
        let lag_count = 3;
        let steps_ahead = 2;

        let result = generate_future_features(&points, lag_count, steps_ahead);
        assert!(result.is_ok());

        let features = result.unwrap();

        // 特徴量の数とデータを確認
        assert_eq!(features.len(), 3);
        assert_eq!(features[0], 8.0); // 最後から3つ目
        assert_eq!(features[1], 9.0); // 最後から2つ目
        assert_eq!(features[2], 10.0); // 最後のポイント
    }
}

#[cfg(test)]
mod prediction_tests {
    use super::*;

    #[test]
    fn test_predict_linear_trend() {
        // 線形に増加するデータで予測をテスト
        let points = create_test_points(15, 100.0, 5.0); // 100, 105, 110, ...
        let last_time = points.last().unwrap().timestamp;
        let target_time = last_time + Duration::hours(3); // 3時間後を予測

        let result = predict_future_rate(&points, target_time);
        assert!(result.is_ok());

        let predicted_value = result.unwrap();
        let predicted_f64 = convert_to_f64(&predicted_value).unwrap();

        // 線形トレンドなので、予測値は 100 + (15+3)*5 = 190 に近い値になるはず
        // ただし完全に一致するとは限らないので、許容範囲を設ける
        assert!(predicted_f64 > 170.0 && predicted_f64 < 210.0);
    }

    #[test]
    fn test_predict_constant_value() {
        // 一定値のデータで予測をテスト
        let points = create_test_points(10, 100.0, 0.0); // すべて100.0
        let last_time = points.last().unwrap().timestamp;
        let target_time = last_time + Duration::hours(5); // 5時間後を予測

        let result = predict_future_rate(&points, target_time);
        assert!(result.is_ok());

        let predicted_value = result.unwrap();
        let predicted_f64 = convert_to_f64(&predicted_value).unwrap();

        // 一定値なので、予測値も100に近いはず
        assert!((predicted_f64 - 100.0).abs() < 5.0);
    }

    #[test]
    fn test_predict_with_random_noise() {
        // ベース値100に±10のランダムノイズを加えたデータ
        let points = create_random_test_points(20, 100.0, 10.0);
        let last_time = points.last().unwrap().timestamp;
        let target_time = last_time + Duration::hours(2); // 2時間後を予測

        let result = predict_future_rate(&points, target_time);
        assert!(result.is_ok());

        let predicted_value = result.unwrap();
        let predicted_f64 = convert_to_f64(&predicted_value).unwrap();

        // ノイズがあるものの、ベース値100からあまり離れていないはず
        assert!(predicted_f64 > 80.0 && predicted_f64 < 120.0);
    }

    #[test]
    fn test_prediction_error_cases() {
        // ケース1: データ不足
        let few_points = create_test_points(3, 1.0, 1.0); // 最小必要数未満
        let last_time = few_points.last().unwrap().timestamp;
        let target_time = last_time + Duration::hours(1);

        let result = predict_future_rate(&few_points, target_time);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("insufficient data points")
        );

        // ケース2: 過去の時間を予測しようとする
        let points = create_test_points(10, 1.0, 1.0);
        let past_time = points.first().unwrap().timestamp - Duration::hours(1);

        let result = predict_future_rate(&points, past_time);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("must be in the future")
        );
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_end_to_end_prediction() {
        // 実際のシナリオを模擬したエンドツーエンドテスト

        // 1. 過去30ポイントのデータを生成（トレンド + サイクル + ノイズ）
        let base_time = Utc.timestamp_opt(1_600_000_000, 0).unwrap().naive_utc();
        let mut points = Vec::with_capacity(30);

        for i in 0..30 {
            // トレンド + 周期的な変動 + 小さなノイズ
            let trend = 100.0 + i as f64 * 2.0;
            let cycle = 10.0 * (i as f64 * 0.2).sin();

            let noise = simple_noise(i, 5.0);

            let value = trend + cycle + noise;
            let rate = BigDecimal::from_str(&value.to_string()).unwrap();
            let timestamp = base_time + Duration::hours(i as i64);

            points.push(Point { rate, timestamp });
        }

        // 2. 最後の時点から5ステップ先を予測
        let last_time = points.last().unwrap().timestamp;
        let target_time = last_time + Duration::hours(5);

        let result = predict_future_rate(&points, target_time);
        assert!(result.is_ok());

        // 3. 予測結果の合理性を確認
        let predicted_value = result.unwrap();
        let predicted_f64 = convert_to_f64(&predicted_value).unwrap();

        // トレンドに基づくと、おおよそ 100 + (30+5)*2 = 170 前後の値になるはず
        // 周期的な変動も考慮して、大きめの許容範囲を設定
        assert!(predicted_f64 > 150.0 && predicted_f64 < 190.0);

        // 4. ログなどの出力（テスト情報）
        println!("End-to-end prediction test:");
        println!(
            "  - Last data point: {:?} at {:?}",
            convert_to_f64(&points.last().unwrap().rate).unwrap(),
            points.last().unwrap().timestamp
        );
        println!(
            "  - Predicted value: {} at {:?}",
            predicted_value, target_time
        );
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_prediction_performance() {
        // 大量のデータポイントで予測パフォーマンスをテスト

        // 1000ポイントのデータを生成
        let points = create_test_points(1000, 100.0, 0.1);
        let last_time = points.last().unwrap().timestamp;
        let target_time = last_time + Duration::hours(10);

        let start = Instant::now();
        let result = predict_future_rate(&points, target_time);
        let duration = start.elapsed();

        assert!(result.is_ok());
        println!("Performance test with 1000 points: {:?}", duration);

        // 性能要件: 1000ポイントのデータで1秒未満
        assert!(duration.as_secs() < 1);
    }

    #[test]
    fn test_training_scaling() {
        // データサイズの増加に対するトレーニング時間のスケーリングをテスト

        let sizes = [100, 200, 500, 1000];
        let mut times = Vec::with_capacity(sizes.len());

        for &size in &sizes {
            let points = create_test_points(size, 100.0, 0.1);
            let last_time = points.last().unwrap().timestamp;
            let target_time = last_time + Duration::hours(1);

            let start = Instant::now();
            let _ = predict_future_rate(&points, target_time);
            let duration = start.elapsed();

            times.push((size, duration));
            println!("Training with {} points took: {:?}", size, duration);
        }

        // 理想的には線形または準線形のスケーリング（O(n)またはO(n log n)）
        // 厳密なチェックはしませんが、ログに出力して手動確認できるようにします
    }
}
