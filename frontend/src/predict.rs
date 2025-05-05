use chrono::{DateTime, Duration, Utc};
use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;
use zaciraci_common::{
    ApiResponse,
    types::TokenAccount,
    stats::{GetValuesRequest, ValueAtTime},
};
use std::str::FromStr;
use std::collections::HashMap;
use std::sync::Arc;

use crate::chronos_api::predict::{ChronosApiClient, ZeroShotPredictionRequest};
use crate::stats::DateRangeSelector;

/// 予測ビューのメインコンポーネント
#[component]
pub fn view() -> Element {
    let client = use_signal(crate::server_api::get_client);
    let chronos_client = use_signal(crate::chronos_api::predict::get_client);

    rsx! {
        div { class: "predict-container",
            style: "display: flex; flex-direction: column; width: 100%;",
            h1 { "価格予測 (Zero-Shot)" }
            
            // 予測インターフェース
            div { class: "predict-section",
                predict_zero_shot_view {
                    server_client: client,
                    chronos_client: chronos_client,
                }
            }
        }
    }
}

/// 予測精度の評価指標を計算する関数
fn calculate_metrics(actual: &[f64], predicted: &[f64]) -> HashMap<String, f64> {
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

/// ゼロショット予測ビューコンポーネント
#[component]
fn predict_zero_shot_view(
    server_client: Signal<Arc<crate::server_api::ApiClient>>,
    chronos_client: Signal<Arc<ChronosApiClient>>,
) -> Element {
    let mut quote = use_signal(|| "wrap.near".to_string());
    let mut base = use_signal(|| "mark.gra-fun.near".to_string());
    
    // デフォルトで2日間の日付範囲を設定
    let now = Utc::now();
    let two_days_ago = now - Duration::days(2);
    
    let start_date = use_signal(|| two_days_ago.format("%Y-%m-%dT%H:%M:%S").to_string());
    let end_date = use_signal(|| now.format("%Y-%m-%dT%H:%M:%S").to_string());
    
    let mut model_name = use_signal(|| "chronos_default".to_string());
    let mut chart_svg = use_signal(|| None::<String>);
    let mut loading = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);
    let mut metrics = use_signal(|| HashMap::<String, f64>::new());

    rsx! {
        div { class: "predict-zero-shot-view",
            h2 { "ゼロショット予測" }
            p { "過去の価格データから将来の価格を予測します。前半1日分のデータを使って後半1日分を予測し、実際のデータと比較します。" }
            
            // トークン選択
            div { class: "token-selection",
                style: "display: flex; gap: 10px; margin-bottom: 10px;",
                div {
                    label { class: "form-label", "Quote Token:" }
                    input {
                        class: "form-control",
                        value: "{quote}",
                        oninput: move |e| quote.set(e.value()),
                    }
                }
                div {
                    label { class: "form-label", "Base Token:" }
                    input {
                        class: "form-control",
                        value: "{base}",
                        oninput: move |e| base.set(e.value()),
                    }
                }
            }
            
            // 日付範囲選択
            DateRangeSelector {
                start_date: start_date,
                end_date: end_date,
            }
            
            // モデル設定
            div { class: "model-settings",
                style: "margin-top: 10px; margin-bottom: 10px;",
                label { class: "form-label", "予測モデル:" }
                select {
                    class: "form-select",
                    value: "{model_name}",
                    onchange: move |e| model_name.set(e.value()),
                    option { value: "chronos_default", "Chronos Default" }
                    option { value: "prophet", "Prophet" }
                    option { value: "arima", "ARIMA" }
                }
            }
            
            // 予測実行ボタン
            button {
                class: "btn btn-primary",
                disabled: "{loading}",
                onclick: move |_| {
                    loading.set(true);
                    error_message.set(None);
                    chart_svg.set(None);
                    metrics.set(HashMap::new());

                    let quote_val = quote().clone();
                    let base_val = base().clone();
                    let start_val = start_date().clone();
                    let end_val = end_date().clone();
                    let model_val = model_name().clone();
                    
                    // 非同期で予測処理を実行
                    spawn_local(async move {
                        // 入力値のバリデーション
                        let quote_token = match TokenAccount::from_str(&quote_val) {
                            Ok(token) => token,
                            Err(e) => {
                                error_message.set(Some(format!("Quote tokenのパースエラー: {}", e)));
                                loading.set(false);
                                return;
                            }
                        };
                        
                        let base_token = match TokenAccount::from_str(&base_val) {
                            Ok(token) => token,
                            Err(e) => {
                                error_message.set(Some(format!("Base tokenのパースエラー: {}", e)));
                                loading.set(false);
                                return;
                            }
                        };
                        
                        let start_datetime: chrono::DateTime<Utc> = match start_val.parse() {
                            Ok(date) => date,
                            Err(e) => {
                                error_message.set(Some(format!("開始日時のパースエラー: {}", e)));
                                loading.set(false);
                                return;
                            }
                        };
                        
                        let end_datetime: chrono::DateTime<Utc> = match end_val.parse() {
                            Ok(date) => date,
                            Err(e) => {
                                error_message.set(Some(format!("終了日時のパースエラー: {}", e)));
                                loading.set(false);
                                return;
                            }
                        };
                        
                        // 期間の検証
                        let duration = end_datetime.signed_duration_since(start_datetime);
                        if duration.num_hours() < 24 {
                            error_message.set(Some("期間は少なくとも24時間以上必要です".to_string()));
                            loading.set(false);
                            return;
                        }
                        
                        // データ取得リクエスト
                        let request = GetValuesRequest {
                            quote_token,
                            base_token,
                            start: start_datetime.naive_utc(),
                            end: end_datetime.naive_utc(),
                        };
                        
                        // 価格データを取得
                        match server_client().stats.get_values(&request).await {
                            Ok(ApiResponse::Success(response)) => {
                                let values_data = response.values;
                                if values_data.is_empty() {
                                    error_message.set(Some("データが見つかりませんでした".to_string()));
                                    loading.set(false);
                                    return;
                                }
                                
                                // データを前半と後半に分割
                                let mid_point = values_data.len() / 2;
                                if mid_point < 2 {
                                    error_message.set(Some("予測用のデータが不足しています".to_string()));
                                    loading.set(false);
                                    return;
                                }
                                
                                let training_data = values_data[..mid_point].to_vec();
                                let test_data = values_data[mid_point..].to_vec();
                                
                                if training_data.is_empty() || test_data.is_empty() {
                                    error_message.set(Some("データ分割後のデータが不足しています".to_string()));
                                    loading.set(false);
                                    return;
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
                                ).with_model_name(model_val);
                                
                                // 予測実行
                                match chronos_client().predict_zero_shot(&prediction_request).await {
                                    Ok(prediction_response) => {
                                        // 予測結果とテストデータの比較
                                        let actual_values: Vec<_> = test_data.iter().map(|v| v.value).collect();
                                        let forecast_values = prediction_response.forecast_values;
                                        
                                        // 予測精度の計算
                                        let calculated_metrics = calculate_metrics(&actual_values, &forecast_values);
                                        metrics.set(calculated_metrics);
                                        
                                        // 学習データをValueAtTime形式に変換
                                        let training_points: Vec<ValueAtTime> = training_data.iter()
                                            .map(|p| p.clone())
                                            .collect();
                                        
                                        // テストデータをValueAtTime形式に変換
                                        let test_points: Vec<ValueAtTime> = test_data.iter()
                                            .map(|p| p.clone())
                                            .collect();
                                        
                                        // 予測データをValueAtTime形式に変換
                                        let mut forecast_points: Vec<ValueAtTime> = Vec::new();
                                        for (i, timestamp) in prediction_response.forecast_timestamp.iter().enumerate() {
                                            if i < forecast_values.len() {
                                                forecast_points.push(ValueAtTime {
                                                    time: timestamp.naive_utc(),
                                                    value: forecast_values[i],
                                                });
                                            }
                                        }
                                        
                                        // 全データを結合（まず学習データ、次にテストデータ）
                                        let mut all_actual_data = Vec::new();
                                        all_actual_data.extend(training_points.clone());
                                        all_actual_data.extend(test_points.clone());
                                        
                                        // チャートをプロット（実際のデータ）
                                        let actual_options = crate::chart::plots::PlotOptions {
                                            image_size: (800, 400),
                                            title: Some(format!("{} / {} (実際の価格)", base_val, quote_val)),
                                            x_label: Some("時間".to_string()),
                                            y_label: Some("価格".to_string()),
                                            line_color: plotters::style::RGBColor(0, 0, 255), // 青色
                                            ..Default::default()
                                        };
                                        
                                        let actual_svg = match crate::chart::plots::plot_values_at_time_to_svg_with_options(
                                            &all_actual_data, actual_options
                                        ) {
                                            Ok(svg) => svg,
                                            Err(e) => {
                                                error_message.set(Some(format!("実際データのチャート作成エラー: {}", e)));
                                                loading.set(false);
                                                return;
                                            }
                                        };
                                        
                                        // チャートをプロット（予測データ）
                                        let forecast_options = crate::chart::plots::PlotOptions {
                                            image_size: (800, 400),
                                            title: Some(format!("{} / {} (予測価格)", base_val, quote_val)),
                                            x_label: Some("時間".to_string()),
                                            y_label: Some("価格".to_string()),
                                            line_color: plotters::style::RGBColor(255, 0, 0), // 赤色
                                            ..Default::default()
                                        };
                                        
                                        let forecast_svg = match crate::chart::plots::plot_values_at_time_to_svg_with_options(
                                            &forecast_points, forecast_options
                                        ) {
                                            Ok(svg) => svg,
                                            Err(e) => {
                                                error_message.set(Some(format!("予測データのチャート作成エラー: {}", e)));
                                                loading.set(false);
                                                return;
                                            }
                                        };
                                        
                                        // 両方のSVGを結合
                                        let combined_svg = format!(
                                            "<div style='display: flex; flex-direction: column; gap: 20px;'><div>{}</div><div>{}</div></div>",
                                            actual_svg, forecast_svg
                                        );
                                        
                                        chart_svg.set(Some(combined_svg));
                                    },
                                    Err(e) => {
                                        error_message.set(Some(format!("予測実行エラー: {}", e)));
                                    }
                                }
                            },
                            Ok(ApiResponse::Error(e)) => {
                                error_message.set(Some(e));
                            },
                            Err(e) => {
                                error_message.set(Some(format!("データ取得エラー: {}", e)));
                            },
                        }
                        
                        loading.set(false);
                    });
                },
                if loading() { "予測処理中..." } else { "予測実行" }
            }
            
            // エラーメッセージの表示
            if let Some(error) = error_message() {
                div {
                    class: "alert alert-danger",
                    style: "margin-top: 10px;",
                    "{error}"
                }
            }
            
            // 予測精度の表示
            if !metrics().is_empty() {
                div {
                    class: "metrics-container",
                    style: "margin-top: 20px; border: 1px solid #ddd; padding: 10px; border-radius: 5px;",
                    h3 { "予測精度" }
                    table {
                        class: "table",
                        thead {
                            tr {
                                th { "指標" }
                                th { "値" }
                            }
                        }
                        tbody {
                            for (metric, value) in metrics().iter() {
                                tr {
                                    td { "{metric}" }
                                    td { "{value:.4}" }
                                }
                            }
                        }
                    }
                }
            }
            
            // チャートの表示
            if let Some(svg) = chart_svg() {
                div {
                    class: "chart-container",
                    style: "margin-top: 20px; width: 100%; overflow-x: auto;",
                    dangerous_inner_html: "{svg}"
                }
            }
        }
    }
}
