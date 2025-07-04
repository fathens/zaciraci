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
use crate::model_registry::{RECOMMENDED_MODELS, get_model_info};
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
    let mut quote = use_signal(|| get_config().quote_token.to_string());
    let mut base = use_signal(|| "mark.gra-fun.near".to_string());

    // デフォルトで2日間の日付範囲を設定
    let now = Utc::now();
    let two_days_ago = now - Duration::days(2);

    let start_date = use_signal(|| two_days_ago.format("%Y-%m-%dT%H:%M").to_string());
    let end_date = use_signal(|| now.format("%Y-%m-%dT%H:%M").to_string());

    let mut model_name = use_signal(|| get_config().default_model_name.clone());
    let mut omit_model_name = use_signal(|| get_config().omit_model_name);
    let mut chart_svg = use_signal(|| None::<String>);
    let mut loading = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);
    let mut metrics = use_signal(HashMap::<String, f64>::new);
    let mut prediction_table_data = use_signal(Vec::<(String, String, String)>::new);

    rsx! {
        div { class: "predict-zero-shot-view",
            h2 { "ゼロショット予測" }
            p { "過去の価格データから将来の価格を予測します。90%のデータを使って残り10%の期間を予測し、実際のデータと比較します。" }

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
                style: "margin-top: 10px; margin-bottom: 20px; padding: 15px; border: 1px solid #ddd; border-radius: 5px;",

                h4 { style: "margin-bottom: 10px; color: #333;", "予測モデル選択" }

                label { class: "form-label", style: "font-weight: bold;", "モデル:" }
                select {
                    class: "form-select",
                    style: "margin-bottom: 10px;",
                    value: "{model_name}",
                    onchange: move |e| model_name.set(e.value()),

                    optgroup { label: "サーバー最適化",
                        option {
                            value: "chronos_default",
                            "Server Default (DeepAR) - 自動最適化, 高精度"
                        }
                    }

                    optgroup { label: "推奨モデル (Chronos Bolt)",
                        for model in RECOMMENDED_MODELS {
                            option {
                                value: "{model.id}",
                                "{model.name} ({model.parameters}M) - {model.speed.as_str()}, {model.accuracy.as_str()}"
                            }
                        }
                    }

                    optgroup { label: "レガシーモデル",
                        option { value: "chronos-t5-small", "Chronos T5 Small (46M) - 中速, 中精度" }
                        option { value: "chronos-t5-base", "Chronos T5 Base (200M) - 低速, 高精度" }
                        option { value: "chronos-t5-tiny", "Chronos T5 Tiny (8M) - 中速, 低精度" }
                    }

                    optgroup { label: "統計モデル",
                        option { value: "prophet", "Prophet - Facebook開発" }
                        option { value: "arima", "ARIMA - 古典的時系列分析" }
                    }
                }

                // 選択されたモデルの詳細情報を表示
                if let Some(selected_model) = get_model_info(&model_name()) {
                    div { class: "model-info",
                        style: "margin-top: 10px; padding: 10px; background-color: #f8f9fa; border-radius: 3px;",

                        p { style: "margin: 0 0 5px 0; font-size: 14px;",
                            strong { "説明: " }
                            "{selected_model.description}"
                        }

                        p { style: "margin: 0 0 5px 0; font-size: 14px;",
                            strong { "推奨用途: " }
                            "{selected_model.recommended_for}"
                        }

                        if selected_model.parameters > 0 {
                            p { style: "margin: 0; font-size: 14px;",
                                strong { "パラメータ数: " }
                                "{selected_model.parameters}M"
                            }
                        }
                    }
                }

                // モデル省略オプション
                div { class: "model-omit-option",
                    style: "margin-top: 15px; padding: 15px; background-color: #fff3cd; border: 1px solid #ffeaa7; border-radius: 5px;",

                    h5 { style: "margin: 0 0 10px 0; color: #856404; font-size: 16px;",
                        "🤖 サーバーデフォルトモデル設定"
                    }

                    label { class: "form-label",
                        style: "display: flex; align-items: center; font-size: 14px; cursor: pointer; margin-bottom: 10px;",
                        input {
                            r#type: "checkbox",
                            checked: omit_model_name(),
                            onchange: move |e| omit_model_name.set(e.checked()),
                            style: "margin-right: 8px;",
                        }
                        "モデル指定を省略（サーバーのデフォルトモデルを使用）"
                    }

                    if omit_model_name() {
                        div { class: "server-default-info",
                            style: "padding: 12px; background-color: #e8f4fd; border: 1px solid #bee5eb; border-radius: 4px; margin-top: 10px;",

                            p { style: "margin: 0 0 8px 0; font-size: 13px; color: #0c5460; font-weight: bold;",
                                "🔍 サーバーデフォルト動作の詳細:"
                            }

                            ul { style: "margin: 0; padding-left: 18px; font-size: 12px; color: #0c5460;",
                                li { style: "margin-bottom: 4px;",
                                    "表示名: ", strong { "\"chronos_default\"" }
                                }
                                li { style: "margin-bottom: 4px;",
                                    "実際のモデル: ", strong { "AutoGluon TimeSeries DeepAR" }
                                }
                                li { style: "margin-bottom: 4px;",
                                    "プリセット: ", strong { "medium_quality" }
                                }
                                li { style: "margin-bottom: 4px;",
                                    "最適化: サーバー側で自動的に最適なハイパーパラメータを選択"
                                }
                            }

                            div { style: "margin-top: 10px; padding: 8px; background-color: #d1ecf1; border-radius: 3px;",
                                p { style: "margin: 0; font-size: 11px; color: #0c5460;",
                                    "💡 ", strong { "推奨事項:" }
                                }
                                p { style: "margin: 2px 0 0 0; font-size: 11px; color: #0c5460;",
                                    "• ", strong { "開発・実験: " }, "省略して最新の最適化を利用"
                                }
                                p { style: "margin: 2px 0 0 0; font-size: 11px; color: #0c5460;",
                                    "• ", strong { "本番環境: " }, "明示指定で一貫した結果を確保"
                                }
                            }
                        }
                    } else {
                        div { class: "manual-selection-info",
                            style: "padding: 10px; background-color: #f8f9fa; border: 1px solid #dee2e6; border-radius: 4px; margin-top: 10px;",

                            p { style: "margin: 0; font-size: 12px; color: #495057;",
                                "✅ 上記で選択されたモデルが明示的に使用されます。"
                            }
                            p { style: "margin: 4px 0 0 0; font-size: 11px; color: #6c757d;",
                                "予測結果の再現性と一貫性が保証されます。"
                            }
                        }
                    }
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
                    let omit_model_val = omit_model_name();

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

                                // AutoGluonの最小要件（5点）を満たすようにデータを分割
                                // 全データが少ない場合は学習データを優先し、テストデータを最小限に
                                let total_points = values_data.len();
                                let min_training_points = 6; // AutoGluonの要件（5点）＋余裕（1点）
                                let min_test_points = 1; // テストには最低1点

                                let (training_data, test_data) = if total_points < min_training_points + min_test_points {
                                    // データが非常に少ない場合はエラー
                                    error_message.set(Some(format!(
                                        "データポイントが不足しています。最低{}点必要ですが、{}点しかありません。",
                                        min_training_points + min_test_points, total_points
                                    )));
                                    loading.set(false);
                                    return;
                                } else if total_points <= 10 {
                                    // 少ないデータの場合：学習データを最低6点確保、残りをテスト
                                    let training_size = std::cmp::max(min_training_points, total_points - min_test_points);
                                    (values_data[..training_size].to_vec(), values_data[training_size..].to_vec())
                                } else {
                                    // 十分なデータがある場合：従来通り9:1分割
                                    let mid_point = (total_points as f64 * 0.9) as usize;
                                    let training_size = std::cmp::max(min_training_points, mid_point);
                                    (values_data[..training_size].to_vec(), values_data[training_size..].to_vec())
                                };

                                // データ分割の詳細をログ出力
                                web_sys::console::log_1(&format!(
                                    "=== データ分割詳細 ===\n\
                                     全データ数: {}\n\
                                     学習データ数: {}\n\
                                     テストデータ数: {}",
                                    total_points, training_data.len(), test_data.len()
                                ).into());

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
                                let prediction_request = if omit_model_val {
                                    // モデル名を省略（サーバーのデフォルトモデルを使用）
                                    ZeroShotPredictionRequest::new(timestamps.clone(), values.clone(), forecast_until)
                                } else {
                                    // モデル名を明示的に指定
                                    ZeroShotPredictionRequest::new(timestamps.clone(), values.clone(), forecast_until)
                                        .with_model_name(model_val.clone())
                                };

                                // リクエスト情報をログ出力
                                web_sys::console::log_1(&format!(
                                    "=== Chronos API リクエスト情報 ===\n\
                                     学習データ数: {}\n\
                                     予測終了時刻: {}\n\
                                     モデル名: {}\n\
                                     学習データ値のサンプル: {:?}",
                                    values.len(),
                                    forecast_until,
                                    if omit_model_val { "サーバーデフォルト".to_string() } else { model_val.clone() },
                                    values.iter().take(5).cloned().collect::<Vec<_>>()
                                ).into());

                                // 非同期予測実行（ポーリングでプログレス表示）
                                match chronos_client.read().predict_with_polling(
                                    &prediction_request,
                                    Some(Box::new(|progress, message| {
                                        web_sys::console::log_1(&format!("予測進捗: {:.1}% - {}", progress * 100.0, message).into());
                                    }))
                                ).await {
                                    Ok(prediction_response) => {
                                        // 予測結果とテストデータの比較
                                        let actual_values: Vec<_> = test_data.iter().map(|v| v.value).collect();
                                        let forecast_values = prediction_response.forecast_values;

                                        // 実際のデータの統計情報をログ出力
                                        web_sys::console::log_1(&format!(
                                            "=== 実際のデータ（テストデータ）統計 ===\n\
                                             データ数: {}\n\
                                             最小値: {}\n\
                                             最大値: {}\n\
                                             平均値: {}",
                                            actual_values.len(),
                                            actual_values.iter().fold(f64::INFINITY, |a, &b| a.min(b)),
                                            actual_values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b)),
                                            actual_values.iter().sum::<f64>() / actual_values.len() as f64
                                        ).into());

                                        // 学習データの統計情報もログ出力
                                        let training_values: Vec<_> = training_data.iter().map(|v| v.value).collect();
                                        web_sys::console::log_1(&format!(
                                            "=== 学習データ統計 ===\n\
                                             データ数: {}\n\
                                             最小値: {}\n\
                                             最大値: {}\n\
                                             平均値: {}",
                                            training_values.len(),
                                            training_values.iter().fold(f64::INFINITY, |a, &b| a.min(b)),
                                            training_values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b)),
                                            training_values.iter().sum::<f64>() / training_values.len() as f64
                                        ).into());

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

                                            // 詳細なAPIレスポンス情報をログ出力
                                            web_sys::console::log_1(&format!(
                                                "=== 予測データ詳細分析 ===\n\
                                                 予測値の数: {}\n\
                                                 予測タイムスタンプの数: {}\n\
                                                 最後のテストポイント値: {}",
                                                forecast_values.len(),
                                                prediction_response.forecast_timestamp.len(),
                                                last_test_point.value
                                            ).into());

                                            // 予測値の統計情報を出力
                                            if !forecast_values.is_empty() {
                                                let min_forecast = forecast_values.iter().fold(f64::INFINITY, |a, &b| a.min(b));
                                                let max_forecast = forecast_values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
                                                let mean_forecast = forecast_values.iter().sum::<f64>() / forecast_values.len() as f64;

                                                web_sys::console::log_1(&format!(
                                                    "予測値の統計:\n\
                                                     - 最小値: {}\n\
                                                     - 最大値: {}\n\
                                                     - 平均値: {}\n\
                                                     - 最初の値: {}\n\
                                                     - 最後の値: {}",
                                                    min_forecast,
                                                    max_forecast,
                                                    mean_forecast,
                                                    forecast_values[0],
                                                    forecast_values[forecast_values.len() - 1]
                                                ).into());

                                                // 先頭10個と末尾10個の予測値を出力
                                                let head_values: Vec<_> = forecast_values.iter().take(10).cloned().collect();
                                                let tail_values: Vec<_> = forecast_values.iter().rev().take(10).cloned().collect();
                                                web_sys::console::log_1(&format!(
                                                    "予測値サンプル（先頭10個）: {:?}",
                                                    head_values
                                                ).into());
                                                web_sys::console::log_1(&format!(
                                                    "予測値サンプル（末尾10個）: {:?}",
                                                    tail_values
                                                ).into());
                                            }

                                            // 予測値の補正係数を計算（大きな値での精度問題を回避）
                                            let correction_factor = if forecast_values.is_empty() {
                                                1.0
                                            } else {
                                                let first_forecast = forecast_values[0];
                                                let forecast_mean = forecast_values.iter().sum::<f64>() / forecast_values.len() as f64;

                                                web_sys::console::log_1(&format!(
                                                    "補正係数計算前の値:\n\
                                                     - 最後のテストポイント値: {}\n\
                                                     - 最初の予測値: {}\n\
                                                     - 予測値の平均: {}",
                                                    last_test_point.value,
                                                    first_forecast,
                                                    forecast_mean
                                                ).into());

                                                if first_forecast != 0.0 && forecast_mean != 0.0 {
                                                    // 比率計算で異常な値を防ぐため上限と下限を設定
                                                    let base_ratio = (last_test_point.value / first_forecast).clamp(0.1, 10.0);
                                                    let mean_ratio = (last_test_point.value / forecast_mean).clamp(0.1, 10.0);

                                                    web_sys::console::log_1(&format!(
                                                        "比率計算:\n\
                                                         - base_ratio: {} / {} = {}\n\
                                                         - mean_ratio: {} / {} = {}",
                                                        last_test_point.value, first_forecast, base_ratio,
                                                        last_test_point.value, forecast_mean, mean_ratio
                                                    ).into());

                                                    // 加重平均を計算し、さらに全体の上限も設定
                                                    let weighted_ratio = 0.7 * base_ratio + 0.3 * mean_ratio;
                                                    let final_ratio = weighted_ratio.clamp(0.2, 5.0); // 最終的な上限：5倍、下限：0.2倍

                                                    web_sys::console::log_1(&format!(
                                                        "最終補正係数計算:\n\
                                                         - weighted_ratio: {}\n\
                                                         - final_ratio (制限後): {}",
                                                        weighted_ratio,
                                                        final_ratio
                                                    ).into());

                                                    final_ratio
                                                } else {
                                                    web_sys::console::log_1(&"補正係数をデフォルト値 1.0 に設定（0除算回避）".into());
                                                    1.0
                                                }
                                            };

                                            web_sys::console::log_1(&format!(
                                                "最終補正係数: {}",
                                                correction_factor
                                            ).into());

                                            // 予測データは実データから独立して表示する
                                            // （実データとの連続性よりも予測の独立性を重視）

                                            // 予測データを補正して追加
                                            web_sys::console::log_1(&"=== 予測値の補正適用 ===".into());
                                            for (i, timestamp) in prediction_response.forecast_timestamp.iter().enumerate() {
                                                if i < forecast_values.len() {
                                                    // 予測値を実際のデータのスケールに合わせる
                                                    let original_value = forecast_values[i];
                                                    let adjusted_value = original_value * correction_factor;

                                                    // 最初の5個、最後の5個、または大きな値の変化があった場合の詳細ログ
                                                    if i < 5 || i >= forecast_values.len() - 5 || (original_value - adjusted_value).abs() > 1000.0 {
                                                        web_sys::console::log_1(&format!(
                                                            "予測ポイント[{}]: 時刻={}, 元の値={}, 補正後の値={}, 変化量={}",
                                                            i, timestamp.naive_utc(), original_value, adjusted_value, adjusted_value - original_value
                                                        ).into());
                                                    }

                                                    forecast_points.push(ValueAtTime {
                                                        time: timestamp.naive_utc(),
                                                        value: adjusted_value,
                                                    });
                                                }
                                            }

                                            // 補正後の統計情報
                                            if !forecast_points.is_empty() {
                                                let adjusted_values: Vec<f64> = forecast_points.iter().map(|p| p.value).collect();
                                                let min_adjusted = adjusted_values.iter().fold(f64::INFINITY, |a, &b| a.min(b));
                                                let max_adjusted = adjusted_values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
                                                let mean_adjusted = adjusted_values.iter().sum::<f64>() / adjusted_values.len() as f64;

                                                web_sys::console::log_1(&format!(
                                                    "補正後の予測値統計:\n\
                                                     - 最小値: {}\n\
                                                     - 最大値: {}\n\
                                                     - 平均値: {}",
                                                    min_adjusted,
                                                    max_adjusted,
                                                    mean_adjusted
                                                ).into());
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
                                            legend_on_left: None, // デフォルト位置を使用
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
                                        // 予測エラーが発生しても実際のデータは表示する
                                        error_message.set(Some(format!("予測実行エラー: {}", e)));

                                        // 学習データとテストデータを結合して実際のデータを表示
                                        let mut all_actual_data = Vec::new();
                                        all_actual_data.extend(training_data.clone());
                                        all_actual_data.extend(test_data.clone());

                                        // 実際のデータのみでチャートを作成
                                        let plot_series = vec![MultiPlotSeries {
                                            values: all_actual_data.clone(),
                                            name: "実際の価格".to_string(),
                                            color: BLUE,
                                        }];

                                        // 複数系列を同一チャートに描画するためのオプション設定
                                        let options = MultiPlotOptions {
                                            image_size: (800, 500),
                                            title: Some(format!("{} / {} (実際のデータのみ - 予測失敗)", base_val, quote_val)),
                                            x_label: Some("時間".to_string()),
                                            y_label: Some("価格".to_string()),
                                            legend_on_left: None,
                                        };

                                        // 実際のデータのみでチャートを描画
                                        let error_svg = match plot_multi_values_at_time_to_svg_with_options(
                                            &plot_series, options
                                        ) {
                                            Ok(svg) => svg,
                                            Err(chart_error) => {
                                                error_message.set(Some(format!("予測実行エラー: {} / チャート作成エラー: {}", e, chart_error)));
                                                String::new()
                                            }
                                        };

                                        if !error_svg.is_empty() {
                                            chart_svg.set(Some(error_svg));
                                        }

                                        // テーブル用データを作成（予測失敗を示す）
                                        let error_table_data = test_data.iter()
                                            .map(|point| {
                                                let time_str = point.time.format("%Y-%m-%d %H:%M").to_string();
                                                let actual_str = format!("{:.4}", point.value);
                                                let forecast_str = "予測失敗".to_string();
                                                (time_str, actual_str, forecast_str)
                                            })
                                            .collect::<Vec<_>>();

                                        prediction_table_data.set(error_table_data);
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

            // 使用されたモデル情報の表示（エラー時でも表示）
            if !metrics().is_empty() || error_message().is_some() && (!prediction_table_data().is_empty() || chart_svg().is_some()) {
                div {
                    class: "model-info-container",
                    style: "margin-top: 20px; border: 1px solid #e3f2fd; padding: 15px; border-radius: 5px; background-color: #f8f9fa;",

                    h3 { style: "margin: 0 0 10px 0; color: #1976d2;", "📊 予測実行情報" }

                    div { style: "display: flex; flex-wrap: wrap; gap: 15px; margin-bottom: 15px;",

                        div { style: "flex: 1; min-width: 200px; padding: 10px; background-color: white; border-radius: 4px; border: 1px solid #e0e0e0;",
                            p { style: "margin: 0 0 5px 0; font-weight: bold; color: #555;", "使用モデル:" }
                            p { style: "margin: 0; font-size: 14px;",
                                if omit_model_name() {
                                    span { style: "color: #1976d2;", "chronos_default" }
                                    span { style: "color: #666; font-size: 12px;", " (サーバー自動選択)" }
                                } else {
                                    span { style: "color: #1976d2;", "{model_name()}" }
                                    span { style: "color: #666; font-size: 12px;", " (明示指定)" }
                                }
                            }
                        }

                        if omit_model_name() {
                            div { style: "flex: 1; min-width: 200px; padding: 10px; background-color: #fff3e0; border-radius: 4px; border: 1px solid #ffcc02;",
                                p { style: "margin: 0 0 5px 0; font-weight: bold; color: #ef6c00;", "実際の処理:" }
                                p { style: "margin: 0; font-size: 13px; color: #ef6c00;", "AutoGluon TimeSeries" }
                                p { style: "margin: 0; font-size: 12px; color: #ef6c00;", "DeepAR (medium_quality)" }
                            }
                        }

                        div { style: "flex: 1; min-width: 200px; padding: 10px; background-color: white; border-radius: 4px; border: 1px solid #e0e0e0;",
                            p { style: "margin: 0 0 5px 0; font-weight: bold; color: #555;", "データ正規化:" }
                            p { style: "margin: 0; font-size: 14px; color: #4caf50;",
                                if get_config().enable_normalization { "有効" } else { "無効" }
                            }
                        }
                    }
                }

                div {
                    class: "metrics-container",
                    style: "margin-top: 15px; border: 1px solid #ddd; padding: 15px; border-radius: 5px;",
                    h3 { style: "margin: 0 0 10px 0;", "📈 予測精度" }

                    if !metrics().is_empty() {
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
                    } else if error_message().is_some() {
                        div {
                            style: "padding: 20px; text-align: center; color: #dc3545; background-color: #f8d7da; border: 1px solid #f5c6cb; border-radius: 4px;",
                            h4 { style: "margin: 0 0 10px 0;", "⚠️ 予測処理失敗" }
                            p { style: "margin: 0; font-size: 14px;", "予測精度の計算ができませんでした" }
                            p { style: "margin: 5px 0 0 0; font-size: 12px; color: #721c24;", "実際のデータは表示されています" }
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

            // 予測結果テーブルの表示
            if !prediction_table_data().is_empty() {
                div {
                    class: "prediction-table-container",
                    style: "margin-top: 20px; border: 1px solid #ddd; padding: 15px; border-radius: 5px;",

                    h3 { style: "margin: 0 0 15px 0;", "📋 予測結果詳細" }

                    div {
                        style: "max-height: 400px; overflow-y: auto; border: 1px solid #e0e0e0; border-radius: 4px;",
                        table {
                            class: "table table-striped",
                            style: "margin-bottom: 0; font-size: 14px;",
                            thead {
                                style: "position: sticky; top: 0; background-color: #f8f9fa; z-index: 10;",
                                tr {
                                    th { style: "border-bottom: 2px solid #dee2e6; padding: 12px 8px; text-align: center;", "時刻" }
                                    th { style: "border-bottom: 2px solid #dee2e6; padding: 12px 8px; text-align: center; color: #0066cc;", "実際の価格" }
                                    th { style: "border-bottom: 2px solid #dee2e6; padding: 12px 8px; text-align: center; color: #cc0000;", "予測価格" }
                                }
                            }
                            tbody {
                                for (i, (time_str, actual_str, forecast_str)) in prediction_table_data().iter().enumerate() {
                                    tr {
                                        style: if i % 2 == 0 { "background-color: #f9f9f9;" } else { "" },
                                        td {
                                            style: "padding: 8px; border-bottom: 1px solid #e0e0e0; font-family: monospace; font-size: 12px;",
                                            "{time_str}"
                                        }
                                        td {
                                            style: "padding: 8px; border-bottom: 1px solid #e0e0e0; text-align: right; font-family: monospace;",
                                            "{actual_str}"
                                        }
                                        td {
                                            style: format!("padding: 8px; border-bottom: 1px solid #e0e0e0; text-align: right; font-family: monospace; color: {};",
                                                if forecast_str == "予測失敗" { "#dc3545" } else { "#000" }
                                            ),
                                            if forecast_str == "予測失敗" {
                                                span { style: "font-weight: bold;", "{forecast_str}" }
                                            } else {
                                                "{forecast_str}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    div {
                        style: "margin-top: 10px; font-size: 12px; color: #666;",
                        p { style: "margin: 2px 0;", "• 青色: 実際の価格データ" }
                        p { style: "margin: 2px 0;", "• 黒色: 正常な予測価格" }
                        p { style: "margin: 2px 0;", "• 赤色: 予測失敗" }
                    }
                }
            }
        }
    }
}
