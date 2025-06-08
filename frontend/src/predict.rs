use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use dioxus::core_macro::component;
use dioxus::dioxus_core::Element;
use dioxus::prelude::*;
use plotters::prelude::{BLUE, RED};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use wasm_bindgen_futures::spawn_local;
use zaciraci_common::{
    ApiResponse,
    stats::{GetValuesRequest, ValueAtTime},
    types::TokenAccount,
};

use crate::chart::plots::{
    MultiPlotOptions, MultiPlotSeries, plot_multi_values_at_time_to_svg_with_options,
};
use crate::chronos_api::predict::{ChronosApiClient, ZeroShotPredictionRequest};
use crate::errors::PredictionError;
use crate::prediction_config::get_config;
use crate::prediction_utils::calculate_metrics;
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

/// ゼロショット予測ビューコンポーネント
#[component]
fn predict_zero_shot_view(
    server_client: Signal<Arc<crate::server_api::ApiClient>>,
    chronos_client: Signal<Arc<ChronosApiClient>>,
) -> Element {
    let mut quote = use_signal(|| get_config().default_quote_token.clone());
    let mut base = use_signal(|| "mark.gra-fun.near".to_string());

    // デフォルトで2日間の日付範囲を設定
    let now = Utc::now();
    let two_days_ago = now - Duration::days(2);

    let start_date = use_signal(|| two_days_ago.format("%Y-%m-%dT%H:%M").to_string());
    let end_date = use_signal(|| now.format("%Y-%m-%dT%H:%M").to_string());

    let mut model_name = use_signal(|| "chronos_default".to_string());
    let mut chart_svg = use_signal(|| None::<String>);
    let mut loading = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);
    let mut metrics = use_signal(HashMap::<String, f64>::new);
    let mut prediction_table_data = use_signal(Vec::<(String, String, String)>::new);

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
                    prediction_table_data.set(Vec::new());

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
                                error_message.set(Some(PredictionError::QuoteTokenParseError(e.to_string()).to_string()));
                                loading.set(false);
                                return;
                            }
                        };

                        let base_token = match TokenAccount::from_str(&base_val) {
                            Ok(token) => token,
                            Err(e) => {
                                error_message.set(Some(PredictionError::BaseTokenParseError(e.to_string()).to_string()));
                                loading.set(false);
                                return;
                            }
                        };

                        let start_datetime: DateTime<Utc> = match NaiveDateTime::parse_from_str(&start_val, "%Y-%m-%dT%H:%M") {
                            Ok(naive) => naive.and_utc(),
                            Err(e) => {
                                error_message.set(Some(PredictionError::StartDateParseError(e.to_string()).to_string()));
                                loading.set(false);
                                return;
                            }
                        };

                        let end_datetime: DateTime<Utc> = match NaiveDateTime::parse_from_str(&end_val, "%Y-%m-%dT%H:%M") {
                            Ok(naive) => naive.and_utc(),
                            Err(e) => {
                                error_message.set(Some(PredictionError::EndDateParseError(e.to_string()).to_string()));
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
                        match server_client.read().stats.get_values(&request).await {
                            Ok(ApiResponse::Success(response)) => {
                                let values_data = response.values;
                                if values_data.is_empty() {
                                    error_message.set(Some(PredictionError::DataNotFound.to_string()));
                                    loading.set(false);
                                    return;
                                }

                                // データを前半と後半に分割
                                let mid_point = values_data.len() / 2;
                                if mid_point < 2 {
                                    error_message.set(Some(PredictionError::InsufficientData.to_string()));
                                    loading.set(false);
                                    return;
                                }
                                let training_data = values_data[..mid_point].to_vec();
                                let test_data = values_data[mid_point..].to_vec();

                                if training_data.is_empty() || test_data.is_empty() {
                                    error_message.set(Some(PredictionError::InsufficientDataAfterSplit.to_string()));
                                    loading.set(false);
                                    return;
                                }

                                // 予測用のタイムスタンプと値を抽出
                                let timestamps: Vec<DateTime<Utc>> = training_data.iter()
                                    .map(|v| DateTime::<Utc>::from_naive_utc_and_offset(v.time, Utc))
                                    .collect();
                                let values: Vec<_> = training_data.iter().map(|v| v.value).collect();

                                // 予測対象の終了時刻（テストデータの最後）
                                let forecast_until = match test_data.last() {
                                    Some(last_point) => DateTime::<Utc>::from_naive_utc_and_offset(
                                        last_point.time,
                                        Utc
                                    ),
                                    None => {
                                        error_message.set(Some("テストデータが不足しています".to_string()));
                                        loading.set(false);
                                        return;
                                    }
                                };

                                // ZeroShotPredictionRequestを作成
                                let prediction_request = ZeroShotPredictionRequest::new(
                                    timestamps,
                                    values,
                                    forecast_until
                                ).with_model_name(model_val);

                                // 予測実行
                                match chronos_client.read().predict_zero_shot(&prediction_request).await {
                                    Ok(prediction_response) => {
                                        // 予測結果とテストデータの比較
                                        let actual_values: Vec<_> = test_data.iter().map(|v| v.value).collect();
                                        let forecast_values = prediction_response.forecast_values;

                                        // 予測精度の計算
                                        let calculated_metrics = calculate_metrics(&actual_values, &forecast_values);
                                        metrics.set(calculated_metrics);

                                        // 学習データをValueAtTime形式に変換
                                        let training_points: Vec<ValueAtTime> = training_data.to_vec();

                                        // テストデータをValueAtTime形式に変換
                                        let _test_points: Vec<ValueAtTime> = test_data.to_vec();

                                        // 予測データを変換
                                        let mut forecast_points: Vec<ValueAtTime> = Vec::new();

                                        // 予測データがあり、テストデータもある場合
                                        if !prediction_response.forecast_timestamp.is_empty() && !forecast_values.is_empty() && !test_data.is_empty() {
                                            // テストデータと予測データを接続（連続性を確保）

                                            // テストデータの最後のポイントを取得
                                            let last_test_point = match test_data.last() {
                                                Some(point) => point,
                                                None => {
                                                    error_message.set(Some("テストデータが不足しています".to_string()));
                                                    loading.set(false);
                                                    return;
                                                }
                                            };

                                            web_sys::console::log_1(&format!(
                                                "テストデータの最後のポイント: 時刻={}, 値={}",
                                                last_test_point.time, last_test_point.value
                                            ).into());

                                            // 予測データの調整（スケーリングと連続性の確保）

                                            // 予測データの時間範囲をデバッグ出力
                                            if !prediction_response.forecast_timestamp.is_empty() {
                                                if let (Some(first_timestamp), Some(last_timestamp)) =
                                                    (prediction_response.forecast_timestamp.first(), prediction_response.forecast_timestamp.last()) {
                                                    web_sys::console::log_1(&format!(
                                                        "予測データの時間範囲: {} から {} ({}個のデータポイント)",
                                                        first_timestamp, last_timestamp, prediction_response.forecast_timestamp.len()
                                                    ).into());
                                                }
                                            }

                                            // 予測値と実際の値の差を計算（補正係数）
                                            let correction_factor = match forecast_values.first() {
                                                Some(&first_value) if first_value != 0.0 => {
                                                    last_test_point.value / first_value
                                                }
                                                _ => 1.0 // ゼロ除算や配列が空の場合を防ぐ
                                            };

                                            web_sys::console::log_1(&format!(
                                                "補正係数: {}",
                                                correction_factor
                                            ).into());

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

                                                    // デバッグ情報（最初と最後のポイントの情報を表示）
                                                    if i == 0 || i == forecast_values.len() - 1 {
                                                        web_sys::console::log_1(&format!(
                                                            "予測ポイント[{}]: 時刻={}, 値={} (元の値={})",
                                                            i, timestamp.naive_utc(), adjusted_value, forecast_values[i]
                                                        ).into());
                                                    }

                                                    forecast_points.push(ValueAtTime {
                                                        time: timestamp.naive_utc(),
                                                        value: adjusted_value,
                                                    });
                                                }
                                            }

                                            // デバッグ情報の出力
                                            web_sys::console::log_1(&format!("変換後の予測ポイント数: {}", forecast_points.len()).into());

                                            // 最初と最後の予測ポイントの時間を表示
                                            if forecast_points.len() >= 2 {
                                                if let (Some(first), Some(last)) = (forecast_points.first(), forecast_points.last()) {
                                                    web_sys::console::log_1(&format!(
                                                        "最初の予測ポイント時刻: {}, 最後の予測ポイント時刻: {}",
                                                        first.time, last.time
                                                    ).into());
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

                                        // 全データを結合（まず学習データ、次にテストデータ）
                                        let mut all_actual_data = Vec::new();
                                        all_actual_data.extend(training_points.clone());
                                        all_actual_data.extend(test_data.clone());

                                        // 表示用のデータを準備（チャート描画前に行う）
                                        // 実際のデータと予測データを時間で整理
                                        let mut all_data_by_time: HashMap<NaiveDateTime, (Option<f64>, Option<f64>)> = HashMap::new();

                                        // 実際のデータを追加（オプションの1番目の要素に入れる）
                                        for point in &all_actual_data {
                                            all_data_by_time.entry(point.time)
                                                .and_modify(|entry| entry.0 = Some(point.value))
                                                .or_insert((Some(point.value), None));
                                        }

                                        // 予測データを追加（オプションの2番目の要素に入れる）
                                        for point in &forecast_points {
                                            all_data_by_time.entry(point.time)
                                                .and_modify(|entry| entry.1 = Some(point.value))
                                                .or_insert((None, Some(point.value)));
                                        }

                                        // 時刻でソートしたデータを作成（予測データがある時間帯のみ）
                                        let mut sorted_data: Vec<(NaiveDateTime, Option<f64>, Option<f64>)> = all_data_by_time
                                            .into_iter()
                                            .filter(|(_, (_, forecast))| forecast.is_some()) // 予測データがある時間帯のみ
                                            .map(|(time, (actual, forecast))| (time, actual, forecast))
                                            .collect();

                                        // 時刻でソート
                                        sorted_data.sort_by_key(|(time, _, _)| *time);

                                        // デバッグ出力
                                        web_sys::console::log_1(&format!("表示用データ件数: {}", sorted_data.len()).into());

                                        // 表示用データを設定
                                        let formatted_table_data = sorted_data.into_iter()
                                            .map(|(time, actual, forecast)| {
                                                let time_str = time.format("%Y-%m-%d %H:%M").to_string();
                                                let actual_str = actual.map(|v| format!("{:.4}", v)).unwrap_or_else(|| "-".to_string());
                                                let forecast_str = forecast.map(|v| format!("{:.4}", v)).unwrap_or_else(|| "-".to_string());
                                                (time_str, actual_str, forecast_str)
                                            })
                                            .collect::<Vec<_>>();

                                        // 系列を作成
                                        let mut plot_series = Vec::new();

                                        // 実際のデータ系列
                                        plot_series.push(MultiPlotSeries {
                                            values: all_actual_data,
                                            name: "実際の価格".to_string(),
                                            color: BLUE,
                                        });

                                        // 予測データ系列（空でなければ追加）
                                        if !forecast_points.is_empty() {
                                            // 予測データの時間範囲をログ出力
                                            if forecast_points.len() >= 2 {
                                                if let (Some(first), Some(last)) = (forecast_points.first(), forecast_points.last()) {
                                                    web_sys::console::log_1(&format!(
                                                        "描画前の予測データ: {} ポイント, 時間範囲: {} から {}",
                                                        forecast_points.len(),
                                                        first.time, last.time
                                                    ).into());
                                                }
                                            }

                                            plot_series.push(MultiPlotSeries {
                                                values: forecast_points,
                                                name: "予測価格".to_string(),
                                                color: RED,
                                            });
                                        }

                                        // 複数系列を同一チャートに描画するためのオプション設定
                                        let options = MultiPlotOptions {
                                            image_size: (800, 500),
                                            title: Some(format!("{} / {} (実際 vs 予測)", base_val, quote_val)),
                                            x_label: Some("時間".to_string()),
                                            y_label: Some("価格".to_string()),
                                        };

                                        // 複数系列を同一チャートにプロット
                                        let combined_svg = match plot_multi_values_at_time_to_svg_with_options(
                                            &plot_series, options
                                        ) {
                                            Ok(svg) => svg,
                                            Err(e) => {
                                                error_message.set(Some(format!("チャート作成エラー: {}", e)));
                                                loading.set(false);
                                                return;
                                            }
                                        };

                                        chart_svg.set(Some(combined_svg));

                                        prediction_table_data.set(formatted_table_data);
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
