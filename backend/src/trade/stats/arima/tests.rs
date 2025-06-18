use crate::logging::*;
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
        // テストの目的: ラグ特徴量生成の基本機能が正しく動作することを確認する
        // テストの内容:
        // 1. 線形に増加する値を持つデータポイントを作成(1.0, 2.0, 3.0, ...)
        // 2. ラグ数3で特徴量を生成
        // 3. 特徴量の形状、値、およびターゲット値が期待通りであることを検証

        // テストデータ: 10個のデータポイント、線形増加(1.0, 2.0, 3.0, ..., 10.0)
        let points = create_test_points(10, 1.0, 1.0);
        let lag_count = 3;

        // ラグ特徴量の生成を実行
        let result = create_lag_features(&points, lag_count);
        assert!(result.is_ok(), "ラグ特徴量の生成に失敗しました");

        let (features, targets) = result.unwrap();

        // 期待されるサンプル数の検証:
        // N個のデータポイントとラグ数kから生成される特徴量の数は N - k 個になるはず
        let expected_sample_count = points.len() - lag_count;
        assert_eq!(
            features.len(),
            expected_sample_count,
            "生成された特徴量の数が期待値と異なります。期待値: {}, 実際: {}",
            expected_sample_count,
            features.len()
        );
        assert_eq!(
            targets.len(),
            expected_sample_count,
            "ターゲットの数が期待値と異なります。期待値: {}, 実際: {}",
            expected_sample_count,
            targets.len()
        );

        // 最初の特徴量とターゲットの検証
        // 最初の特徴量は [1.0, 2.0, 3.0]、ターゲットは 4.0 になるはず
        let first_feature = &features[0];
        assert_eq!(first_feature.len(), lag_count, "特徴量の次元数が不正です");

        for (i, &value) in first_feature.iter().enumerate() {
            assert_eq!(
                value,
                (i + 1) as f64,
                "最初の特徴量の第{}要素が期待値と異なります",
                i
            );
        }
        assert_eq!(
            targets[0],
            (lag_count + 1) as f64,
            "最初のターゲットが期待値と異なります"
        );

        // 最後の特徴量とターゲットの検証
        // 最後の特徴量は [7.0, 8.0, 9.0]、ターゲットは 10.0 になるはず
        let last_index = expected_sample_count - 1;
        let last_feature = &features[last_index];

        // 最後の特徴量の各要素を検証
        assert_eq!(
            last_feature[0], 7.0,
            "最後の特徴量の第0要素が期待値と異なります"
        );
        assert_eq!(
            last_feature[1], 8.0,
            "最後の特徴量の第1要素が期待値と異なります"
        );
        assert_eq!(
            last_feature[2], 9.0,
            "最後の特徴量の第2要素が期待値と異なります"
        );
        assert_eq!(
            targets[last_index], 10.0,
            "最後のターゲットが期待値と異なります"
        );

        // 全ての特徴量の形状の検証
        for (i, feature) in features.iter().enumerate() {
            assert_eq!(
                feature.len(),
                lag_count,
                "特徴量{}の次元数が不正です。期待値: {}, 実際: {}",
                i,
                lag_count,
                feature.len()
            );
        }
    }

    #[test]
    fn test_create_lag_features_insufficient_data() {
        // テストの目的: データポイント不足時のエラー処理が適切に機能することを確認する
        // ラグ特徴量の生成には少なくともラグ数+1個のデータポイントが必要
        // 十分なデータがない場合は適切なエラーメッセージを返すべき

        // ケース1: 明らかに不足するデータ量（ラグ数 > データ数）
        let points_few = create_test_points(3, 1.0, 1.0);
        let lag_count = 4; // データポイント数より大きいラグ数

        let result = create_lag_features(&points_few, lag_count);
        assert!(result.is_err(), "データ不足時にエラーが発生すべき");

        // エラーメッセージの詳細を検証
        match result {
            Err(e) => {
                let error_string = e.to_string();
                assert!(
                    error_string.contains("insufficient data points"),
                    "適切なエラーメッセージが含まれていません: {}",
                    error_string
                );
                // エラーメッセージには最小必要データ数（lag_count+1）が含まれるはず
                let min_required = (lag_count + 1).to_string();
                assert!(
                    error_string.contains(&min_required),
                    "エラーメッセージに最小必要データ数が含まれていません: {}",
                    error_string
                );
            }
            Ok(_) => panic!("データ不足時にエラーが発生しませんでした"),
        }

        // ケース2: 境界値テスト - ちょうど必要最小限のデータ量
        let points_exact = create_test_points(lag_count + 1, 1.0, 1.0);
        let result_exact = create_lag_features(&points_exact, lag_count);
        assert!(
            result_exact.is_ok(),
            "最小必要数のデータでも特徴量生成が成功すべき"
        );

        if let Ok((features, targets)) = result_exact {
            assert_eq!(features.len(), 1, "生成される特徴量は1つだけのはず");
            assert_eq!(targets.len(), 1, "生成されるターゲットは1つだけのはず");
            assert_eq!(
                features[0].len(),
                lag_count,
                "特徴量の次元数はラグ数と等しいはず"
            );
        }

        // ケース3: 境界値テスト - 必要最小限より1つ少ないデータ量
        let points_almost = create_test_points(lag_count, 1.0, 1.0);
        let result_almost = create_lag_features(&points_almost, lag_count);
        assert!(
            result_almost.is_err(),
            "最小必要数-1のデータ量ではエラーが発生すべき"
        );
    }

    #[test]
    fn test_generate_future_features() {
        let log = DEFAULT.new(o!("function" => "test_generate_future_features"));

        // テストの目的: 将来予測のための特徴量が正しく生成されることを確認する
        // 将来予測では、直近のデータポイントからラグ数に基づいて特徴量を作成する
        // 例えば、ラグ数3の場合、最新の3つの値が特徴量として使用される

        // 基本ケース: 標準的なパラメータでのテスト
        let points = create_test_points(10, 1.0, 1.0); // 1.0, 2.0, ..., 10.0
        let lag_count = 3;
        let steps_ahead = 2;

        debug!(
            log,
            "基本ケース: ラグ数 {}, 予測ステップ {}", lag_count, steps_ahead
        );

        let result = generate_future_features(&points, lag_count, steps_ahead);
        assert!(result.is_ok(), "将来特徴量の生成に失敗しました");

        let features = result.unwrap();

        // 特徴量の次元数検証: ラグ数と一致するはず
        assert_eq!(
            features.len(),
            lag_count,
            "特徴量の次元数はラグ数と一致するはず。期待値: {}, 実際: {}",
            lag_count,
            features.len()
        );

        // 特徴量の値検証: データの最後から遡って最新のlag_count個の値が使われるはず
        // steps_aheadは予測する未来の時点を示すが、特徴量自体には影響しない
        for (i, &feature) in features.iter().enumerate() {
            let expected_index = points.len() - lag_count + i;
            let expected_value = (expected_index + 1) as f64; // テストデータは1から始まる連番

            assert_eq!(
                feature, expected_value,
                "特徴量[{}]の値が期待と異なります。期待値: {}, 実際: {}",
                i, expected_value, feature
            );
        }

        // 追加ケース: 異なるステップ数での検証
        let different_steps = [0, 1, 5];

        for &step in &different_steps {
            debug!(
                log,
                "異なるステップ数のケース: ラグ数 {}, 予測ステップ {}", lag_count, step
            );

            let result = generate_future_features(&points, lag_count, step);
            assert!(
                result.is_ok(),
                "将来特徴量の生成に失敗しました (ステップ数: {})",
                step
            );

            let features = result.unwrap();

            // 特徴量の次元数は常にラグ数と一致するはず（ステップ数に関わらず）
            assert_eq!(
                features.len(),
                lag_count,
                "特徴量の次元数はラグ数と一致するはず。期待値: {}, 実際: {}",
                lag_count,
                features.len()
            );

            // 特徴量の内容はステップ数に関わらず同じはず
            // なぜならステップ数は予測対象の時点を指定するだけで、入力特徴量は変わらないため
            assert_eq!(features[0], 8.0, "最初の特徴量が期待値と異なります");
            assert_eq!(features[1], 9.0, "2番目の特徴量が期待値と異なります");
            assert_eq!(features[2], 10.0, "3番目の特徴量が期待値と異なります");
        }
    }
}

#[cfg(test)]
mod prediction_tests {
    use super::*;

    #[test]
    fn test_predict_linear_trend() {
        let log = DEFAULT.new(o!("function" => "test_predict_linear_trend"));
        
        // 線形に増加するデータで予測をテスト
        let points = create_test_points(15, 100.0, 5.0); // 100, 105, 110, ...
        let last_time = points.last().unwrap().timestamp;

        // 複数の予測時間間隔でテストする
        let time_intervals = [
            Duration::hours(1),  // 短期予測
            Duration::hours(3),  // 中期予測
            Duration::hours(24), // 長期予測
        ];

        for &interval in &time_intervals {
            let target_time = last_time + interval;
            let hours = interval.num_hours();

            let result = predict_future_rate(&points, target_time);
            assert!(result.is_ok());

            let predicted_value = result.unwrap();
            let predicted_f64 = convert_to_f64(&predicted_value).unwrap();

            // 線形トレンドなので、予測値は 100 + (15+hours)*5 に近い値になるはず
            let expected_base = 100.0 + (15.0 + hours as f64) * 5.0;
            let tolerance = 5.0 + (hours as f64 * 0.5); // 時間が長いほど許容誤差を増やす

            assert!(
                (predicted_f64 - expected_base).abs() < tolerance,
                "時間間隔 {}時間の予測値が期待範囲外: 予測値={}, 期待値={} ± {}",
                hours,
                predicted_f64,
                expected_base,
                tolerance
            );

            debug!(log, "時間間隔 {}時間の予測: {}", hours, predicted_f64);
        }
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
    fn test_prediction_error_insufficient_data() {
        // テストケース: 最小必要数よりも少ないデータポイントでの予測
        // ARIMAモデルには一定量以上のデータポイントが必要なため、
        // データが不足している場合は適切なエラーを返すべき

        // 少ないデータポイントを準備（最小必要数未満）
        let few_points = create_test_points(3, 1.0, 1.0);
        let last_time = few_points.last().unwrap().timestamp;
        let target_time = last_time + Duration::hours(1);

        // 予測を実行
        let result = predict_future_rate(&few_points, target_time);

        // エラーが返されることを検証
        assert!(result.is_err(), "データポイント不足時にエラーが発生すべき");

        // エラーメッセージの内容を検証
        let error_message = result.unwrap_err().to_string();
        assert!(
            error_message.contains("insufficient data points"),
            "エラーメッセージがデータ不足を示すべき: {}",
            error_message
        );
    }

    #[test]
    fn test_prediction_error_past_time() {
        // テストケース: 過去の時間に対する予測
        // 予測は未来の時間に対してのみ行うべきであり、
        // 過去の時間に対する予測は意味がないためエラーを返すべき

        // 通常の量のデータポイントを準備
        let points = create_test_points(10, 1.0, 1.0);

        // 最初のデータポイントより過去の時間を指定
        let past_time = points.first().unwrap().timestamp - Duration::hours(1);

        // 予測を実行
        let result = predict_future_rate(&points, past_time);

        // エラーが返されることを検証
        assert!(
            result.is_err(),
            "過去の時間に対する予測でエラーが発生すべき"
        );

        // エラーメッセージの内容を検証
        let error_message = result.unwrap_err().to_string();
        assert!(
            error_message.contains("must be in the future"),
            "エラーメッセージが過去時間の問題を示すべき: {}",
            error_message
        );
    }

    #[test]
    fn test_prediction_error_empty_dataset() {
        // テストケース: 空のデータセットに対する予測
        // データポイントが存在しない場合は予測できないため、
        // 適切なエラーを返すべき

        // 空のデータセットを準備
        let empty_points: Vec<Point> = Vec::new();

        // 任意の予測時間を設定
        let target_time = Utc.timestamp_opt(1_600_010_000, 0).unwrap().naive_utc();

        // 予測を実行
        let result = predict_future_rate(&empty_points, target_time);

        // エラーが返されることを検証
        assert!(result.is_err(), "空のデータセットでエラーが発生すべき");

        // エラーメッセージの内容を検証
        let error_message = result.unwrap_err().to_string();
        assert!(
            error_message.contains("empty") || error_message.contains("insufficient"),
            "エラーメッセージが空のデータセットの問題を示すべき: {}",
            error_message
        );
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_end_to_end_prediction() {
        let log = DEFAULT.new(o!("function" => "test_end_to_end_prediction"));
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
        debug!(log, "End-to-end prediction test:");
        debug!(
            log,
            "  - Last data point: {:?} at {:?}",
            convert_to_f64(&points.last().unwrap().rate).unwrap(),
            points.last().unwrap().timestamp
        );
        debug!(
            log,
            "  - Predicted value: {} at {:?}", predicted_value, target_time
        );
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_prediction_performance() {
        let log = DEFAULT.new(o!("function" => "test_prediction_performance"));

        // 大量のデータポイントで予測パフォーマンスをテスト

        // 1000ポイントのデータを生成
        let points = create_test_points(1000, 100.0, 0.1);
        let last_time = points.last().unwrap().timestamp;
        let target_time = last_time + Duration::hours(10);

        let start = Instant::now();
        let result = predict_future_rate(&points, target_time);
        let duration = start.elapsed();

        assert!(result.is_ok());
        debug!(log, "Performance test with 1000 points: {:?}", duration);

        // 性能要件: 1000ポイントのデータで1秒未満
        assert!(duration.as_secs() < 1);
    }

    #[test]
    fn test_training_scaling() {
        let log = DEFAULT.new(o!("function" => "test_training_scaling"));

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
            debug!(log, "Training with {} points took: {:?}", size, duration);
        }

        // 理想的には線形または準線形のスケーリング（O(n)またはO(n log n)）
        // 厳密なチェックはしませんが、ログに出力して手動確認できるようにします
    }
}
