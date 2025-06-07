use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use zaciraci_common::{
    stats::ValueAtTime,
    types::TokenAccount,
};

use crate::chart::plots::MultiPlotSeries;
use crate::chronos_api::predict::{ChronosApiClient, ZeroShotPredictionRequest};
use plotters::prelude::{BLUE, RED};

#[derive(Clone, Debug)]
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
    let timestamps: Vec<DateTime<Utc>> = training_data.iter()
        .map(|v| DateTime::<Utc>::from_naive_utc_and_offset(v.time, Utc))
        .collect();
    let values: Vec<_> = training_data.iter().map(|v| v.value).collect();

    // 予測対象の終了時刻（テストデータの最後）
    let forecast_until = DateTime::<Utc>::from_naive_utc_and_offset(
        test_data.last().unwrap().time,
        Utc
    );

    // ZeroShotPredictionRequestを作成
    let prediction_request = ZeroShotPredictionRequest::new(
        timestamps,
        values,
        forecast_until
    ).with_model_name(model_name);

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
            if !prediction_response.forecast_timestamp.is_empty() && !forecast_values.is_empty() && !test_data.is_empty() {
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
            let chart_svg = generate_prediction_chart_svg(&training_data, &test_data, &forecast_points)?;

            // 予測価格（最後の予測値）
            let predicted_price = forecast_points.last()
                .map(|p| p.value)
                .unwrap_or(0.0);

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
        Err(e) => Err(format!("予測実行エラー: {}", e))
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
    // 全データを結合（まず学習データ、次にテストデータ）
    let mut all_actual_data = Vec::new();
    all_actual_data.extend(training_data.to_vec());
    all_actual_data.extend(test_data.to_vec());

    // 系列を作成
    let mut plot_series = Vec::new();

    // 実際のデータ系列
    plot_series.push(MultiPlotSeries {
        values: all_actual_data,
        name: "実際の価格".to_string(),
        color: BLUE,
    });

    // 予測データ系列（空でなければ追加）
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
        &plot_series, multi_options
    ) {
        Ok(svg_content) => Ok(svg_content),
        Err(e) => Err(format!("チャート生成エラー: {}", e))
    }
}
