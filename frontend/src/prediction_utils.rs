use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use zaciraci_common::stats::ValueAtTime;

use crate::chart::plots::{
    MultiPlotOptions, MultiPlotSeries, plot_multi_values_at_time_to_svg_with_options,
};
use crate::chronos_api::predict::{ChronosApiClient, ZeroShotPredictionRequest};
use crate::data_normalization::DataNormalizer;
use crate::errors::PredictionError;
use crate::prediction_config::get_config;
use plotters::prelude::{BLUE, RED};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PredictionResult {
    pub predicted_price: f64,
    pub accuracy: f64,
    pub chart_svg: Option<String>,
    pub metrics: HashMap<String, f64>,
    pub forecast_data: Vec<ValueAtTime>,
}

/// ゼロショット予測を実行し、予測精度を検証する関数
/// 
/// この関数は未来予測ではなく、予測モデルの精度検証を目的としています：
/// - データを90:10に分割（90%を学習、10%をテスト）
/// - 学習データでモデルを訓練し、テストデータ期間の予測を実行
/// - 予測結果と実際のテストデータを比較して精度を評価
/// - チャートで予測と実際の差異を視覚的に確認
pub async fn execute_zero_shot_prediction(
    values_data: &[ValueAtTime],
    model_name: String,
    chronos_client: Arc<ChronosApiClient>,
) -> Result<PredictionResult, PredictionError> {
    // データの検証・正規化処理
    let normalized_data = validate_and_normalize_data(values_data)?;
    
    // データを90:10に分割
    let (training_data, test_data) = split_data_for_prediction(&normalized_data)?;
    
    // ZeroShotPredictionRequestの作成
    let prediction_request = create_prediction_request(training_data, test_data, model_name)?;

    // 予測実行
    match chronos_client.predict_zero_shot(&prediction_request).await {
        Ok(prediction_response) => {
            // 予測結果とテストデータの比較
            let actual_values: Vec<_> = test_data.iter().map(|v| v.value).collect();
            let forecast_values = prediction_response.forecast_values;

            // 予測精度の計算
            let metrics = calculate_metrics(&actual_values, &forecast_values);

            // 予測データを変換
            let mut forecast_points: Vec<ValueAtTime> = Vec::new();

            // 予測データがあり、テストデータもある場合
            if !prediction_response.forecast_timestamp.is_empty()
                && !forecast_values.is_empty()
                && !test_data.is_empty()
            {
                // テストデータの最後のポイントを取得
                let last_test_point = match test_data.last() {
                    Some(point) => point,
                    None => return Err(PredictionError::InsufficientData),
                };

                // デバッグ出力：予測データの詳細
                println!("=== 予測データ解析 ===");
                println!("APIから返された予測値: {:?}", &forecast_values[..forecast_values.len().min(5)]);
                println!("予測タイムスタンプ数: {}", prediction_response.forecast_timestamp.len());
                println!("予測値数: {}", forecast_values.len());
                
                // 予測APIから返された最初の予測値を取得
                let first_api_forecast_value = forecast_values[0];
                println!("最初の予測値: {}", first_api_forecast_value);
                println!("テストデータ最後の値: {}", last_test_point.value);

                // 最初の予測値とテストデータの最後の値の差分を計算
                let offset = last_test_point.value - first_api_forecast_value;
                println!("適用するオフセット: {}", offset);

                // テストデータの最後のポイントを予測データの開始点として使用
                forecast_points.push(ValueAtTime {
                    time: last_test_point.time,
                    value: last_test_point.value,
                });

                // 予測データを差分調整して追加（形状を保持）
                for (i, timestamp) in prediction_response.forecast_timestamp.iter().enumerate() {
                    if i < forecast_values.len() {
                        // 予測値に差分を加算（形状を保持しつつレベル調整）
                        let adjusted_value = forecast_values[i] + offset;

                        forecast_points.push(ValueAtTime {
                            time: timestamp.naive_utc(),
                            value: adjusted_value,
                        });
                    }
                }
            } else {
                // テストデータがない場合や予測データがない場合は、そのまま変換
                for (i, timestamp) in prediction_response.forecast_timestamp.iter().enumerate() {
                    if i < forecast_values.len() {
                        forecast_points.push(ValueAtTime {
                            time: timestamp.naive_utc(),
                            value: forecast_values[i],
                        });
                    }
                }
            }

            // チャートSVGを生成
            let config = get_config();
            
            // デバッグ出力：時間軸の重なりをチェック
            println!("=== チャートデータの時間軸分析 ===");
            
            let actual_data = &normalized_data;
            
            if let (Some(actual_first), Some(actual_last)) = (actual_data.first(), actual_data.last()) {
                println!("実際データ時間範囲: {} ～ {}", actual_first.time, actual_last.time);
                println!("実際データ点数: {}", actual_data.len());
            }
            
            if let (Some(forecast_first), Some(forecast_last)) = (forecast_points.first(), forecast_points.last()) {
                println!("予測データ時間範囲: {} ～ {}", forecast_first.time, forecast_last.time);
                println!("予測データ点数: {}", forecast_points.len());
                
                // 重複チェック：実際データの最後と予測データの最初が重なっているか
                if let Some(actual_last) = actual_data.last() {
                    if actual_last.time == forecast_first.time {
                        println!("✅ 時間軸の接続OK: {} で接続", actual_last.time);
                    } else {
                        println!("⚠️ 時間軸のギャップ: 実際データ終了 {} vs 予測データ開始 {}", 
                                actual_last.time, forecast_first.time);
                    }
                }
            }
            
            let chart_svg = plot_multi_values_at_time_to_svg_with_options(
                &[
                    MultiPlotSeries {
                        name: "実際の価格".to_string(),
                        values: normalized_data.clone(),
                        color: BLUE,
                    },
                    MultiPlotSeries {
                        name: "予測価格".to_string(),
                        values: forecast_points.clone(),
                        color: RED,
                    },
                ],
                MultiPlotOptions {
                    title: Some("価格予測".to_string()),
                    image_size: config.chart_size(),
                    x_label: Some("時間".to_string()),
                    y_label: Some("価格".to_string()),
                },
            )
            .map_err(|e| PredictionError::ChartGenerationFailed(e.to_string()))?;

            // 予測価格（最後の予測値）
            let predicted_price = forecast_points.last().map(|p| p.value).unwrap_or(0.0);

            Ok(PredictionResult {
                predicted_price,
                accuracy: 100.0 - metrics.get("MAPE").unwrap_or(&0.0), // MAPEから精度を計算
                chart_svg: Some(chart_svg),
                metrics,
                forecast_data: forecast_points,
            })
        }
        Err(e) => Err(PredictionError::PredictionFailed(format!(
            "予測実行エラー: {}",
            e
        ))),
    }
}

/// 予測精度の評価指標を計算する関数
pub fn calculate_metrics(actual: &[f64], predicted: &[f64]) -> HashMap<String, f64> {
    let n = actual.len().min(predicted.len());
    if n == 0 {
        return HashMap::new();
    }

    // 二乗誤差和
    let mut squared_errors_sum = 0.0;
    // 絶対誤差和
    let mut absolute_errors_sum = 0.0;
    // 絶対パーセント誤差和
    let mut absolute_percent_errors_sum = 0.0;

    for i in 0..n {
        let error = actual[i] - predicted[i];
        squared_errors_sum += error * error;
        absolute_errors_sum += error.abs();

        // 分母がゼロに近い場合はパーセント誤差を計算しない
        if actual[i].abs() > 1e-10 {
            absolute_percent_errors_sum += (error.abs() / actual[i].abs()) * 100.0;
        }
    }

    let mut metrics = HashMap::new();
    metrics.insert("RMSE".to_string(), (squared_errors_sum / n as f64).sqrt());
    metrics.insert("MAE".to_string(), absolute_errors_sum / n as f64);
    metrics.insert("MAPE".to_string(), absolute_percent_errors_sum / n as f64);

    metrics
}

/// データの基本検証と正規化処理を行う関数
/// 
/// この関数は以下の処理を行います：
/// - データの基本検証（空データ、最小サイズチェック）
/// - 数値の妥当性検証（有限値、正の値）
/// - 時系列データの順序チェック
/// - データ正規化処理（設定に応じて）
/// 
/// # Arguments
/// * `values_data` - 検証・正規化する時系列データ
/// 
/// # Returns
/// `Result<Vec<ValueAtTime>, PredictionError>` - 検証・正規化されたデータまたはエラー
pub fn validate_and_normalize_data(
    values_data: &[ValueAtTime],
) -> Result<Vec<ValueAtTime>, PredictionError> {
    // データの基本検証
    if values_data.is_empty() {
        return Err(PredictionError::DataNotFound);
    }

    if values_data.len() < 4 {
        return Err(PredictionError::InsufficientData);
    }

    // 数値の妥当性検証と時系列チェック
    let mut previous_time: Option<chrono::NaiveDateTime> = None;
    for (i, point) in values_data.iter().enumerate() {
        // 数値検証
        if !point.value.is_finite() {
            return Err(PredictionError::InvalidData(format!(
                "Invalid value at index {}: {} (not finite)", i, point.value
            )));
        }
        if point.value <= 0.0 {
            return Err(PredictionError::InvalidData(format!(
                "Invalid value at index {}: {} (not positive)", i, point.value
            )));
        }
        
        // 時系列の順序チェック
        if let Some(prev_time) = previous_time {
            if point.time <= prev_time {
                return Err(PredictionError::InvalidData(format!(
                    "Time series order error at index {}: {} <= {} (not strictly increasing)", 
                    i, point.time, prev_time
                )));
            }
        }
        
        // 重複時刻チェック（既に前のチェックで検出されるが明示的に）
        if let Some(prev_time) = previous_time {
            if point.time == prev_time {
                return Err(PredictionError::InvalidData(format!(
                    "Duplicate timestamp at index {}: {}", i, point.time
                )));
            }
        }
        
        previous_time = Some(point.time);
    }

    // データ正規化処理を追加
    let config = get_config();
    let normalized_data = if config.enable_normalization {
        let normalizer = DataNormalizer::new(
            config.normalization_window,
            config.outlier_threshold,
            config.max_change_ratio,
        );
        
        let normalized_values = normalizer.normalize_data(values_data)
            .map_err(|e| PredictionError::InvalidData(format!("データ正規化エラー: {}", e)))?;
        
        // 品質指標を出力（デバッグ用）
        let quality_metrics = normalizer.calculate_data_quality_metrics(values_data, &normalized_values);
        println!("データ正規化完了:");
        quality_metrics.print_summary();
        
        normalized_values
    } else {
        values_data.to_vec()
    };
    
    Ok(normalized_data)
}

/// データを90:10に分割する関数（90%を学習、10%をテスト）
/// 
/// # Arguments
/// * `normalized_data` - 検証・正規化済みの時系列データ
/// 
/// # Returns
/// `Result<(&[ValueAtTime], &[ValueAtTime]), PredictionError>` - (学習データ, テストデータ)のタプルまたはエラー
pub fn split_data_for_prediction(
    normalized_data: &[ValueAtTime],
) -> Result<(&[ValueAtTime], &[ValueAtTime]), PredictionError> {
    // データを90:10に分割（90%を学習、10%をテスト）
    let split_point = (normalized_data.len() as f64 * 0.9) as usize;
    if split_point < 2 || (normalized_data.len() - split_point) < 1 {
        return Err(PredictionError::InsufficientData);
    }

    let training_data = &normalized_data[..split_point];
    let test_data = &normalized_data[split_point..];

    if training_data.is_empty() || test_data.is_empty() {
        return Err(PredictionError::InsufficientData);
    }

    Ok((training_data, test_data))
}

/// ZeroShotPredictionRequestを作成する関数
/// 
/// この関数は分割済みのデータからZeroShotPredictionRequestを作成します。
/// 
/// # Arguments
/// * `training_data` - 学習用データ（既に分割済み）
/// * `test_data` - テスト用データ（既に分割済み）
/// * `model_name` - 使用するモデル名
/// 
/// # Returns
/// `Result<ZeroShotPredictionRequest, PredictionError>` - 作成されたリクエストまたはエラー
pub fn create_prediction_request(
    training_data: &[ValueAtTime],
    test_data: &[ValueAtTime],
    model_name: String,
) -> Result<ZeroShotPredictionRequest, PredictionError> {

    // 予測用のタイムスタンプと値を抽出
    let timestamps: Vec<DateTime<Utc>> = training_data
        .iter()
        .map(|v| DateTime::<Utc>::from_naive_utc_and_offset(v.time, Utc))
        .collect();
    let values: Vec<_> = training_data.iter().map(|v| v.value).collect();

    // 予測対象の終了日時（テストデータの最後まで）
    let forecast_until = match test_data.last() {
        Some(last_point) => DateTime::<Utc>::from_naive_utc_and_offset(last_point.time, Utc),
        None => return Err(PredictionError::InsufficientData),
    };

    // ZeroShotPredictionRequestを作成
    let config = get_config();
    let prediction_request = if config.omit_model_name {
        // モデル名を省略（サーバーのデフォルトモデルを使用）
        println!("モデル名を省略してリクエストを送信（サーバーデフォルトを使用）");
        ZeroShotPredictionRequest::new(timestamps, values, forecast_until)
    } else {
        // モデル名を明示的に指定
        println!("モデル名を指定してリクエストを送信: {}", model_name);
        ZeroShotPredictionRequest::new(timestamps, values, forecast_until)
            .with_model_name(model_name)
    };
    
    Ok(prediction_request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;

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

        println!("=== データ系列分離テスト ===");

        // generate_prediction_chart_svg内部のロジックを直接テスト
        let mut all_actual_data = Vec::new();
        all_actual_data.extend(training_data.to_vec());

        // テストデータの最後の点以外を追加（重複を避けるため）
        if !test_data.is_empty() {
            let test_data_without_last = &test_data[..test_data.len() - 1];
            all_actual_data.extend(test_data_without_last.to_vec());
        }

        println!("元のtraining_data: {} points", training_data.len());
        println!("元のtest_data: {} points", test_data.len());
        println!("結合後のall_actual_data: {} points", all_actual_data.len());
        println!("forecast_data: {} points", forecast_data.len());

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

        println!("=== タイムスタンプ重複チェック ===");
        for forecast_time in &forecast_times {
            let overlap_count = actual_times
                .iter()
                .filter(|&&t| t == *forecast_time)
                .count();
            if overlap_count > 0 {
                println!(
                    "⚠️  重複発見: {:?} が実際データにも{}回存在",
                    forecast_time, overlap_count
                );
            } else {
                println!("✅ {:?} は重複なし", forecast_time);
            }
        }

        println!("✅ データ系列分離テスト完了");
    }

    #[test]
    fn test_forecast_shape_preservation() {
        println!("=== 予測データ形状保持テスト ===");
        
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
        
        println!("元の予測値: {:?}", original_forecast_values);
        println!("調整後の予測値: {:?}", adjusted_values);
        println!("差分オフセット: {}", offset);
        
        // 形状保持の検証：隣接する値の差分が保持されているか
        for i in 1..original_forecast_values.len() {
            let original_diff = original_forecast_values[i] - original_forecast_values[i-1];
            let adjusted_diff = adjusted_values[i] - adjusted_values[i-1];
            
            assert!(
                (original_diff - adjusted_diff).abs() < 1e-10,
                "形状が保持されていません: index {} で元の差分 {} vs 調整後の差分 {}",
                i, original_diff, adjusted_diff
            );
        }
        
        // レベル調整の検証：最初の値が正しく調整されているか
        assert!(
            (adjusted_values[0] - last_test_value).abs() < 1e-10,
            "レベル調整が正しくありません: 期待値 {} vs 実際値 {}",
            last_test_value, adjusted_values[0]
        );
        
        // 変動の検証：すべての値が同じでないことを確認（直線化していない）
        let all_same = adjusted_values.windows(2).all(|w| (w[0] - w[1]).abs() < 1e-10);
        assert!(!all_same, "予測が直線化されています（すべての値が同じ）");
        
        println!("✅ 形状保持テスト完了");
        println!("✅ レベル調整テスト完了");
        println!("✅ 非直線化テスト完了");
    }

    #[test]
    fn test_problematic_multiplication_approach() {
        println!("=== 問題のある乗算手法のテスト ===");
        
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
        
        println!("元の予測値: {:?}", original_forecast_values);
        println!("乗算調整後: {:?}", multiplied_values);
        println!("差分調整後: {:?}", adjusted_values);
        
        // 変動パターンの比較
        for i in 1..original_forecast_values.len() {
            let original_diff = original_forecast_values[i] - original_forecast_values[i-1];
            let multiplied_diff = multiplied_values[i] - multiplied_values[i-1];
            let adjusted_diff = adjusted_values[i] - adjusted_values[i-1];
            
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
        
        println!("✅ 乗算手法は形状をスケールすることを確認");
        println!("✅ 差分手法は形状を保持することを確認");
    }

    #[test]
    fn test_validate_and_normalize_data_with_valid_data() {
        println!("=== validate_and_normalize_data テスト（有効データ） ===");
        
        // 10個のテストデータを作成
        let mut test_data = Vec::new();
        for i in 0..10 {
            test_data.push(ValueAtTime {
                time: NaiveDateTime::parse_from_str(
                    &format!("2025-06-{:02} 00:00:00", i + 1), 
                    "%Y-%m-%d %H:%M:%S"
                ).unwrap(),
                value: 100.0 + i as f64,
            });
        }
        
        // 関数の実行
        let result = validate_and_normalize_data(&test_data);
        
        // 結果の検証
        assert!(result.is_ok(), "validate_and_normalize_data should succeed with valid data");
        let normalized_data = result.unwrap();
        
        // データ数が保持されていることを確認
        assert_eq!(normalized_data.len(), test_data.len());
        
        println!("✅ 有効データのテスト完了");
    }

    #[test]
    fn test_validate_and_normalize_data_with_empty_data() {
        println!("=== validate_and_normalize_data テスト（空データ） ===");
        
        let empty_data = vec![];
        
        // 関数の実行
        let result = validate_and_normalize_data(&empty_data);
        
        // 結果の検証（空データはエラーになるべき）
        assert!(result.is_err(), "validate_and_normalize_data should fail with empty data");
        
        if let Err(error) = result {
            match error {
                PredictionError::DataNotFound => {
                    println!("✅ 期待通りDataNotFoundエラーが発生");
                }
                _ => {
                    panic!("予期しないエラータイプ: {:?}", error);
                }
            }
        }
        
        println!("✅ 空データのテスト完了");
    }

    #[test]
    fn test_validate_and_normalize_data_insufficient_data() {
        println!("=== validate_and_normalize_data テスト（データ不足） ===");
        
        // 3個のデータ（最小要件の4個未満）
        let insufficient_data = vec![
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
                value: 1.0,
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
                value: 1.1,
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-03 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
                value: 1.2,
            },
        ];
        
        // 関数の実行
        let result = validate_and_normalize_data(&insufficient_data);
        
        // 結果の検証（データ不足はエラーになるべき）
        assert!(result.is_err(), "validate_and_normalize_data should fail with insufficient data");
        
        if let Err(error) = result {
            match error {
                PredictionError::InsufficientData => {
                    println!("✅ 期待通りInsufficientDataエラーが発生");
                }
                _ => {
                    panic!("予期しないエラータイプ: {:?}", error);
                }
            }
        }
        
        println!("✅ データ不足のテスト完了");
    }

    #[test]
    fn test_validate_and_normalize_data_invalid_values() {
        println!("=== validate_and_normalize_data テスト（無効な値） ===");
        
        // 無効な値を含むデータ（負の値、無限大、NaN）
        let invalid_data = vec![
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
                value: 1.0,
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
                value: -1.0, // 負の値
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-03 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
                value: 1.2,
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-04 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
                value: f64::INFINITY, // 無限大
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-05 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
                value: 1.5,
            },
        ];
        
        // 関数の実行
        let result = validate_and_normalize_data(&invalid_data);
        
        // 結果の検証（無効な値はエラーになるべき）
        assert!(result.is_err(), "validate_and_normalize_data should fail with invalid values");
        
        if let Err(error) = result {
            match error {
                PredictionError::InvalidData(_) => {
                    println!("✅ 期待通りInvalidDataエラーが発生");
                }
                _ => {
                    panic!("予期しないエラータイプ: {:?}", error);
                }
            }
        }
        
        println!("✅ 無効な値のテスト完了");
    }

    #[test]
    fn test_validate_and_normalize_data_time_order_validation() {
        println!("=== validate_and_normalize_data テスト（時間順序検証） ===");
        
        // 時間順序が間違っているデータ
        let unordered_data = vec![
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
                value: 1.0,
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-03 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(), // 順序が逆
                value: 1.1,
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(), // 前より古い
                value: 1.2,
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-04 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
                value: 1.3,
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-05 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
                value: 1.4,
            },
        ];
        
        // 関数の実行
        let result = validate_and_normalize_data(&unordered_data);
        
        // 結果の検証（時間順序エラーになるべき）
        assert!(result.is_err(), "validate_and_normalize_data should fail with unordered timestamps");
        
        if let Err(error) = result {
            match error {
                PredictionError::InvalidData(msg) => {
                    assert!(msg.contains("Time series order error"), "Error message should mention time series order");
                    println!("✅ 期待通り時間順序エラーが発生: {}", msg);
                }
                _ => {
                    panic!("予期しないエラータイプ: {:?}", error);
                }
            }
        }
        
        println!("✅ 時間順序検証のテスト完了");
    }

    #[test]
    fn test_create_prediction_request_with_split_data() {
        println!("=== create_prediction_request テスト（分割済みデータ） ===");
        
        // 10個のテストデータを作成
        let mut normalized_data = Vec::new();
        for i in 0..10 {
            normalized_data.push(ValueAtTime {
                time: NaiveDateTime::parse_from_str(
                    &format!("2025-06-{:02} 00:00:00", i + 1), 
                    "%Y-%m-%d %H:%M:%S"
                ).unwrap(),
                value: 100.0 + i as f64,
            });
        }
        
        // データを分割
        let (training_data, test_data) = split_data_for_prediction(&normalized_data).unwrap();
        let model_name = "test_model".to_string();
        
        // 関数の実行
        let result = create_prediction_request(training_data, test_data, model_name);
        
        // 結果の検証
        assert!(result.is_ok(), "create_prediction_request should succeed with split data");
        let prediction_request = result.unwrap();
        
        // リクエストデータの検証
        assert_eq!(prediction_request.timestamp.len(), training_data.len());
        assert_eq!(prediction_request.values.len(), training_data.len());
        
        println!("✅ 分割済みデータのテスト完了");
    }

    #[test]
    fn test_split_data_for_prediction_with_valid_data() {
        println!("=== split_data_for_prediction テスト（有効データ） ===");
        
        // 10個のテストデータを作成（90:10分割に十分な量）
        let mut test_data = Vec::new();
        for i in 0..10 {
            test_data.push(ValueAtTime {
                time: NaiveDateTime::parse_from_str(
                    &format!("2025-06-{:02} 00:00:00", i + 1), 
                    "%Y-%m-%d %H:%M:%S"
                ).unwrap(),
                value: 100.0 + i as f64,
            });
        }
        
        // 関数の実行
        let result = split_data_for_prediction(&test_data);
        
        // 結果の検証
        assert!(result.is_ok(), "split_data_for_prediction should succeed with valid data");
        let (training_data, test_data_slice) = result.unwrap();
        
        // データ分割の検証（90:10）
        let expected_training_size = (test_data.len() as f64 * 0.9) as usize; // 9
        let expected_test_size = test_data.len() - expected_training_size; // 1
        
        assert_eq!(training_data.len(), expected_training_size);
        assert_eq!(test_data_slice.len(), expected_test_size);
        
        // データの連続性の確認
        assert_eq!(training_data[training_data.len()-1].time.format("%Y-%m-%d").to_string(), "2025-06-09");
        assert_eq!(test_data_slice[0].time.format("%Y-%m-%d").to_string(), "2025-06-10");
        
        println!("✅ 有効データの分割テスト完了");
    }

    #[test]
    fn test_split_data_for_prediction_insufficient_data() {
        println!("=== split_data_for_prediction テスト（データ不足） ===");
        
        // 2個のデータ（分割後のtraining_dataが2個未満になる: split_point = 1）
        let insufficient_data = vec![
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
                value: 1.0,
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
                value: 1.1,
            },
        ];
        
        // 関数の実行
        let result = split_data_for_prediction(&insufficient_data);
        
        // 結果の検証（データ不足はエラーになるべき）
        assert!(result.is_err(), "split_data_for_prediction should fail with insufficient data");
        
        if let Err(error) = result {
            match error {
                PredictionError::InsufficientData => {
                    println!("✅ 期待通りInsufficientDataエラーが発生");
                }
                _ => {
                    panic!("予期しないエラータイプ: {:?}", error);
                }
            }
        }
        
        println!("✅ データ不足のテスト完了");
    }

    #[test]
    fn test_split_data_for_prediction_three_items_valid() {
        println!("=== split_data_for_prediction テスト（3個データ有効） ===");
        
        // 3個のデータ（90:10分割で2:1になる）
        let three_data = vec![
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
                value: 1.0,
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
                value: 1.1,
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-03 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
                value: 1.2,
            },
        ];
        
        // 関数の実行
        let result = split_data_for_prediction(&three_data);
        
        // 結果の検証
        assert!(result.is_ok(), "split_data_for_prediction should succeed with 3 items");
        let (training_data, test_data) = result.unwrap();
        
        // データ分割の検証
        assert_eq!(training_data.len(), 2); // 90% = 2.7 -> 2
        assert_eq!(test_data.len(), 1);     // 残り = 1
        
        // 値の確認
        assert_eq!(training_data[0].value, 1.0);
        assert_eq!(training_data[1].value, 1.1);
        assert_eq!(test_data[0].value, 1.2);
        
        println!("✅ 3個データ有効のテスト完了");
    }

    #[test]
    fn test_split_data_for_prediction_minimum_valid_data() {
        println!("=== split_data_for_prediction テスト（最小有効データ） ===");
        
        // 4個のデータ（90:10分割で3:1になる）
        let minimum_data = vec![
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
                value: 1.0,
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
                value: 1.1,
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-03 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
                value: 1.2,
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-04 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
                value: 1.3,
            },
        ];
        
        // 関数の実行
        let result = split_data_for_prediction(&minimum_data);
        
        // 結果の検証
        assert!(result.is_ok(), "split_data_for_prediction should succeed with minimum valid data");
        let (training_data, test_data) = result.unwrap();
        
        // データ分割の検証
        assert_eq!(training_data.len(), 3); // 90% = 3.6 -> 3
        assert_eq!(test_data.len(), 1);     // 残り = 1
        
        // 値の確認
        assert_eq!(training_data[0].value, 1.0);
        assert_eq!(training_data[1].value, 1.1);
        assert_eq!(training_data[2].value, 1.2);
        assert_eq!(test_data[0].value, 1.3);
        
        println!("✅ 最小有効データのテスト完了");
    }
}
