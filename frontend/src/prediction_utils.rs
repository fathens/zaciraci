use chrono::{DateTime, NaiveDateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use zaciraci_common::{stats::ValueAtTime, types::TokenAccount};

use crate::chart::plots::MultiPlotSeries;
use crate::chronos_api::predict::{ChronosApiClient, ZeroShotPredictionRequest};
use plotters::prelude::{BLUE, RED};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PredictionResult {
    pub predicted_price: f64,
    pub accuracy: f64,
    pub chart_svg: Option<String>,
    pub metrics: HashMap<String, f64>,
    pub training_data: Vec<ValueAtTime>,
    pub test_data: Vec<ValueAtTime>,
    pub forecast_data: Vec<ValueAtTime>,
}

/// ゼロショット予測を実行する共通関数
pub async fn execute_zero_shot_prediction(
    _quote_token: TokenAccount,
    _base_token: TokenAccount,
    values_data: Vec<ValueAtTime>,
    model_name: String,
    chronos_client: Arc<ChronosApiClient>,
) -> Result<PredictionResult, String> {
    if values_data.is_empty() {
        return Err("データが見つかりませんでした".to_string());
    }

    // データを前半と後半に分割
    let mid_point = values_data.len() / 2;
    if mid_point < 2 {
        return Err("予測用のデータが不足しています".to_string());
    }

    let training_data = values_data[..mid_point].to_vec();
    let test_data = values_data[mid_point..].to_vec();

    if training_data.is_empty() || test_data.is_empty() {
        return Err("データ分割後のデータが不足しています".to_string());
    }

    // 予測用のタイムスタンプと値を抽出
    let timestamps: Vec<DateTime<Utc>> = training_data
        .iter()
        .map(|v| DateTime::<Utc>::from_naive_utc_and_offset(v.time, Utc))
        .collect();
    let values: Vec<_> = training_data.iter().map(|v| v.value).collect();

    // 予測対象の終了時刻（テストデータの最後）
    let forecast_until =
        DateTime::<Utc>::from_naive_utc_and_offset(test_data.last().unwrap().time, Utc);

    // ZeroShotPredictionRequestを作成
    let prediction_request = ZeroShotPredictionRequest::new(timestamps, values, forecast_until)
        .with_model_name(model_name);

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
                let last_test_point = test_data.last().unwrap();

                // 予測APIから返された最初の予測値を取得
                let first_api_forecast_value = forecast_values[0];

                // 予測値と実際の値の差を計算（補正係数）
                let correction_factor = if first_api_forecast_value != 0.0 {
                    last_test_point.value / first_api_forecast_value
                } else {
                    1.0 // ゼロ除算を防ぐ
                };

                // テストデータの最後のポイントから滑らかに続けるために、
                // 最後のテストポイントを予測データの開始点として使用
                forecast_points.push(ValueAtTime {
                    time: last_test_point.time,
                    value: last_test_point.value,
                });

                // 予測データを補正して追加
                for (i, timestamp) in prediction_response.forecast_timestamp.iter().enumerate() {
                    if i < forecast_values.len() {
                        // 予測値を実際のデータのスケールに合わせる
                        let adjusted_value = forecast_values[i] * correction_factor;

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
            let chart_svg =
                generate_prediction_chart_svg(&training_data, &test_data, &forecast_points)?;

            // 予測価格（最後の予測値）
            let predicted_price = forecast_points.last().map(|p| p.value).unwrap_or(0.0);

            Ok(PredictionResult {
                predicted_price,
                accuracy: 100.0 - metrics.get("MAPE").unwrap_or(&0.0), // MAPEから精度を計算
                chart_svg: Some(chart_svg),
                metrics,
                training_data,
                test_data,
                forecast_data: forecast_points,
            })
        }
        Err(e) => Err(format!("予測実行エラー: {}", e)),
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

/// 予測チャートのSVGを生成
fn generate_prediction_chart_svg(
    training_data: &[ValueAtTime],
    test_data: &[ValueAtTime],
    forecast_data: &[ValueAtTime],
) -> Result<String, String> {
    // 実際のデータを結合（ただし、テストデータの最後の点は予測データと重複するため除外）
    let mut all_actual_data = Vec::new();
    all_actual_data.extend(training_data.to_vec());

    // テストデータの最後の点以外を追加（重複を避けるため）
    if !test_data.is_empty() {
        let test_data_without_last = &test_data[..test_data.len() - 1];
        all_actual_data.extend(test_data_without_last.to_vec());
    }

    // 系列を作成
    let mut plot_series = Vec::new();

    // 実際のデータ系列（最後のテストポイントを除く）
    if !all_actual_data.is_empty() {
        plot_series.push(MultiPlotSeries {
            values: all_actual_data,
            name: "実際の価格".to_string(),
            color: BLUE,
        });
    }

    // 予測データ系列（空でなければ追加、接続点を含む）
    if !forecast_data.is_empty() {
        plot_series.push(MultiPlotSeries {
            values: forecast_data.to_vec(),
            name: "予測価格".to_string(),
            color: RED,
        });
    }

    // 複数系列を同一チャートに描画するためのオプション設定
    let multi_options = crate::chart::plots::MultiPlotOptions {
        image_size: (600, 300),
        title: Some("価格予測".to_string()),
        x_label: Some("時間".to_string()),
        y_label: Some("価格".to_string()),
    };

    // チャートSVGを生成
    match crate::chart::plots::plot_multi_values_at_time_to_svg_with_options(
        &plot_series,
        multi_options,
    ) {
        Ok(svg_content) => Ok(svg_content),
        Err(e) => Err(format!("チャート生成エラー: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_data() -> (Vec<ValueAtTime>, Vec<ValueAtTime>, Vec<ValueAtTime>) {
        // トレーニングデータ（5ポイント）
        let training_data = vec![
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
                value: 1.05,
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-04 00:00:00", "%Y-%m-%d %H:%M:%S")
                    .unwrap(),
                value: 1.15,
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-05 00:00:00", "%Y-%m-%d %H:%M:%S")
                    .unwrap(),
                value: 1.2,
            },
        ];

        // テストデータ（3ポイント）
        let test_data = vec![
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-06 00:00:00", "%Y-%m-%d %H:%M:%S")
                    .unwrap(),
                value: 1.25,
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-07 00:00:00", "%Y-%m-%d %H:%M:%S")
                    .unwrap(),
                value: 1.3,
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-08 00:00:00", "%Y-%m-%d %H:%M:%S")
                    .unwrap(),
                value: 1.35,
            },
        ];

        // 予測データ（テストデータの最後の点から開始して3ポイント）
        let forecast_data = vec![
            // 接続点：テストデータの最後の点と同じ
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-08 00:00:00", "%Y-%m-%d %H:%M:%S")
                    .unwrap(),
                value: 1.35, // テストデータの最後の値と同じ
            },
            // 予測点
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-09 00:00:00", "%Y-%m-%d %H:%M:%S")
                    .unwrap(),
                value: 1.4,
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-10 00:00:00", "%Y-%m-%d %H:%M:%S")
                    .unwrap(),
                value: 1.45,
            },
        ];

        (training_data, test_data, forecast_data)
    }

    #[test]
    fn test_generate_prediction_chart_svg() {
        let (training_data, test_data, forecast_data) = create_test_data();

        println!("=== テストデータ詳細 ===");
        println!("Training data ({} points):", training_data.len());
        for (i, point) in training_data.iter().enumerate() {
            println!("  [{}] {:?} -> {}", i, point.time, point.value);
        }

        println!("Test data ({} points):", test_data.len());
        for (i, point) in test_data.iter().enumerate() {
            println!("  [{}] {:?} -> {}", i, point.time, point.value);
        }

        println!("Forecast data ({} points):", forecast_data.len());
        for (i, point) in forecast_data.iter().enumerate() {
            println!("  [{}] {:?} -> {}", i, point.time, point.value);
        }

        // 重複チェック：テストデータの最後と予測データの最初
        let last_test = test_data.last().unwrap();
        let first_forecast = forecast_data.first().unwrap();
        println!("=== 重複チェック ===");
        println!(
            "Last test point: {:?} -> {}",
            last_test.time, last_test.value
        );
        println!(
            "First forecast point: {:?} -> {}",
            first_forecast.time, first_forecast.value
        );
        println!("Time match: {}", last_test.time == first_forecast.time);
        println!("Value match: {}", last_test.value == first_forecast.value);

        // チャート生成
        match generate_prediction_chart_svg(&training_data, &test_data, &forecast_data) {
            Ok(svg_content) => {
                println!("=== SVG生成成功 ===");
                println!("SVG length: {} characters", svg_content.len());

                // SVGの内容を簡単にチェック
                assert!(svg_content.contains("<svg"));
                assert!(svg_content.contains("実際の価格"));
                assert!(svg_content.contains("予測価格"));

                // SVGの先頭と末尾を表示
                let preview_len = std::cmp::min(500, svg_content.len());
                println!("=== SVG プレビュー（先頭{}文字）===", preview_len);
                println!("{}", &svg_content[..preview_len]);

                if svg_content.len() > 500 {
                    println!("...(省略)...");
                    let end_start = std::cmp::max(500, svg_content.len() - 200);
                    println!("=== SVG プレビュー（末尾）===");
                    println!("{}", &svg_content[end_start..]);
                }
            }
            Err(e) => {
                panic!("SVG生成エラー: {}", e);
            }
        }
    }

    #[test]
    fn test_empty_data_handling() {
        println!("=== 空データハンドリングテスト ===");

        let empty_data = vec![];
        let (training_data, test_data, _) = create_test_data();

        // 空の予測データでテスト
        match generate_prediction_chart_svg(&training_data, &test_data, &empty_data) {
            Ok(svg_content) => {
                println!("✅ 空の予測データでもSVG生成成功");
                assert!(svg_content.contains("実際の価格"));
                assert!(!svg_content.contains("予測価格")); // 予測データがないので含まれない
            }
            Err(e) => {
                panic!("空の予測データでエラー: {}", e);
            }
        }

        // 空のテストデータでテスト
        match generate_prediction_chart_svg(&training_data, &empty_data, &empty_data) {
            Ok(svg_content) => {
                println!("✅ 空のテストデータでもSVG生成成功");
                assert!(svg_content.contains("実際の価格"));
            }
            Err(e) => {
                panic!("空のテストデータでエラー: {}", e);
            }
        }
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
}
