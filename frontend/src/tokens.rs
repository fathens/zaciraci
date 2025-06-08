use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use dioxus::prelude::*;
use std::sync::Arc;
use wasm_bindgen_futures::spawn_local;

use crate::prediction_config::get_config;
use crate::services::VolatilityPredictionService;
use crate::stats::DateRangeSelector;

#[component]
pub fn view() -> Element {
    let server_client = use_signal(crate::server_api::get_client);
    let chronos_client = use_signal(crate::chronos_api::predict::get_client);

    let volatility_service = use_signal(|| {
        Arc::new(VolatilityPredictionService::new(
            server_client(),
            chronos_client(),
        ))
    });

    // デフォルトで7日間の日付範囲を設定
    let now = Utc::now();
    let seven_days_ago = now - Duration::days(7);

    let start_date = use_signal(|| seven_days_ago.format("%Y-%m-%dT%H:%M").to_string());
    let end_date = use_signal(|| now.format("%Y-%m-%dT%H:%M").to_string());
    let mut limit = use_signal(|| 10_u32);

    let mut loading = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);
    let mut prediction_results = use_signal(Vec::<(String, String, String, String)>::new);
    let mut token_charts = use_signal(Vec::<TokenVolatilityData>::new);

    rsx! {
        div { class: "tokens-view",
            h2 { "ボラティリティトークン予測" }
            p { "ボラティリティの高いトークンを取得し、各トークンについてゼロショット予測を実行します。指定した日付範囲のデータを使用して予測を行います。" }

            // 日付範囲選択
            DateRangeSelector {
                start_date: start_date,
                end_date: end_date,
            }

            // 制限数の設定
            div { class: "limit-setting",
                style: "margin-top: 10px; margin-bottom: 10px;",
                label { class: "form-label", "取得トークン数:" }
                input {
                    class: "form-control",
                    r#type: "number",
                    value: "{limit}",
                    min: "1",
                    max: "50",
                    oninput: move |e| {
                        if let Ok(val) = e.value().parse::<u32>() {
                            limit.set(val);
                        }
                    },
                }
            }

            // 実行ボタン
            button {
                class: "btn btn-primary",
                disabled: "{loading}",
                onclick: move |_| {
                    loading.set(true);
                    error_message.set(None);
                    prediction_results.set(Vec::new());
                    token_charts.set(Vec::new());

                    let start_val = start_date().clone();
                    let end_val = end_date().clone();
                    let limit_value = limit();

                    // 非同期で処理を実行
                    spawn_local(async move {
                        // 入力値のバリデーション
                        let start_datetime: DateTime<Utc> = match NaiveDateTime::parse_from_str(&start_val, "%Y-%m-%dT%H:%M") {
                            Ok(naive) => naive.and_utc(),
                            Err(e) => {
                                error_message.set(Some(e.to_string()));
                                loading.set(false);
                                return;
                            }
                        };

                        let end_datetime: DateTime<Utc> = match NaiveDateTime::parse_from_str(&end_val, "%Y-%m-%dT%H:%M") {
                            Ok(naive) => naive.and_utc(),
                            Err(e) => {
                                error_message.set(Some(e.to_string()));
                                loading.set(false);
                                return;
                            }
                        };

                        // ボラティリティトークンを取得
                        let config = get_config();
                        let volatility_result = volatility_service().get_volatility_tokens(
                            start_datetime,
                            end_datetime,
                            limit_value,
                        ).await;

                        match volatility_result {
                            Ok(volatility_tokens_result) => {
                                let tokens = volatility_tokens_result.tokens;
                                if tokens.is_empty() {
                                    error_message.set(Some("ボラティリティトークンが見つかりません".to_string()));
                                    loading.set(false);
                                    return;
                                }

                                // 各トークンについて予測実行を準備
                                let mut results = Vec::new();

                                // 初期結果を設定（処理中表示用）
                                for (index, token) in tokens.iter().enumerate() {
                                    results.push((
                                        (index + 1).to_string(),
                                        token.to_string(),
                                        "処理中".to_string(),
                                        "-".to_string(),
                                    ));
                                }

                                prediction_results.set(results);

                                // 各トークンについて価格データを取得してチャートを生成
                                for (index, token) in tokens.iter().enumerate() {
                                    // 予測用データ取得期間（ユーザー指定の日付範囲を使用）
                                    let predict_start = start_datetime;
                                    let predict_end = end_datetime;

                                    // 予測結果を取得
                                    let prediction_result = volatility_service().predict_token(token, predict_start, predict_end, config.default_quote_token.clone()).await;

                                    match prediction_result {
                                        Ok(result) => {
                                            // 予測結果を更新
                                            let mut current_results = prediction_results();
                                            if index < current_results.len() {
                                                current_results[index] = (
                                                    (index + 1).to_string(),
                                                    token.to_string(),
                                                    format!("{:.6}", result.predicted_price),
                                                    format!("{:.2}%", result.accuracy),
                                                );
                                                prediction_results.set(current_results);
                                            }

                                            // チャートデータを追加
                                            let mut token_charts_vec = token_charts();
                                            token_charts_vec.push(TokenVolatilityData {
                                                token: token.to_string(),
                                                rank: index + 1,
                                                predicted_price: result.predicted_price,
                                                accuracy: result.accuracy,
                                                chart_svg: result.chart_svg,
                                            });
                                            token_charts.set(token_charts_vec);
                                        }
                                        Err(e) => {
                                            error_message.set(Some(e.to_string()));
                                        }
                                    }
                                }
                            },
                            Err(e) => {
                                error_message.set(Some(e.to_string()));
                            }
                        }

                        loading.set(false);
                    });
                },
                if loading() { "処理中..." } else { "予測実行" }
            }

            // エラーメッセージの表示
            if let Some(error) = error_message() {
                div {
                    class: "alert alert-danger",
                    style: "margin-top: 10px;",
                    "{error}"
                }
            }

            // 予測結果の表示
            if !prediction_results().is_empty() {
                div {
                    class: "results-container",
                    style: "margin-top: 20px; border: 1px solid #ddd; padding: 10px; border-radius: 5px;",
                    h3 { "予測結果" }
                    table {
                        class: "table table-striped",
                        style: "width: 100%;",
                        thead {
                            tr {
                                th { style: "text-align: center;", "順位" }
                                th { style: "text-align: left;", "トークン" }
                                th { style: "text-align: right;", "予測価格" }
                                th { style: "text-align: right;", "精度" }
                            }
                        }
                        tbody {
                            for (rank, token, prediction, accuracy) in prediction_results().iter() {
                                tr {
                                    td { style: "text-align: center;", "{rank}" }
                                    td { style: "text-align: left; font-family: monospace;", "{token}" }
                                    td { style: "text-align: right; font-family: monospace;", "{prediction}" }
                                    td { style: "text-align: right;", "{accuracy}" }
                                }
                            }
                        }
                    }
                }
            }

            // チャートデータの表示
            if !token_charts().is_empty() {
                div {
                    class: "chart-container",
                    style: "margin-top: 20px;",
                    h3 { "チャートデータ" }
                    for chart_data in token_charts().iter() {
                        div {
                            style: "margin-bottom: 30px; padding: 15px; border: 1px solid #ddd; border-radius: 8px; background-color: #f9f9f9;",
                            h4 {
                                style: "margin-bottom: 10px; color: #333; font-family: monospace;",
                                "#{chart_data.rank} {chart_data.token}"
                            }
                            div {
                                style: "display: flex; gap: 20px; margin-bottom: 10px; font-size: 14px;",
                                span {
                                    style: "font-weight: bold; color: #555;",
                                    "予測価格: "
                                    span {
                                        style: "font-family: monospace; color: #2c5aa0;",
                                        "{chart_data.predicted_price:.6}"
                                    }
                                }
                                span {
                                    style: "font-weight: bold; color: #555;",
                                    "精度: "
                                    span {
                                        style: "color: #28a745;",
                                        "{chart_data.accuracy:.2}%"
                                    }
                                }
                            }
                            // チャートを表示するためのSVGを生成する
                            if let Some(svg) = &chart_data.chart_svg {
                                div {
                                    class: "chart-container",
                                    style: "margin-top: 10px; width: 100%; overflow-x: auto;",
                                    dangerous_inner_html: "{svg}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
struct TokenVolatilityData {
    token: String,
    rank: usize,
    predicted_price: f64,
    accuracy: f64,
    chart_svg: Option<String>,
}
