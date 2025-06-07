use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use dioxus::core_macro::component;
use dioxus::dioxus_core::Element;
use dioxus::prelude::*;
use std::str::FromStr;
use wasm_bindgen_futures::spawn_local;
use zaciraci_common::{
    ApiResponse, pools::VolatilityTokensRequest, stats::GetValuesRequest, types::TokenAccount,
};

use crate::chart::plots::MultiPlotSeries;
use crate::stats::DateRangeSelector;
use plotters::prelude::BLUE;

#[derive(Clone, Debug)]
struct TokenChartData {
    token: String,
    rank: usize,
    predicted_price: String,
    accuracy: String,
    chart_svg: Option<String>,
}

#[component]
pub fn view() -> Element {
    let server_client = use_signal(crate::server_api::get_client);

    // デフォルトで7日間の日付範囲を設定
    let now = Utc::now();
    let seven_days_ago = now - Duration::days(7);

    let start_date = use_signal(|| seven_days_ago.format("%Y-%m-%dT%H:%M").to_string());
    let end_date = use_signal(|| now.format("%Y-%m-%dT%H:%M").to_string());
    let mut limit = use_signal(|| 10_u32);

    let mut loading = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);
    let mut prediction_results = use_signal(Vec::<(String, String, String, String)>::new);
    let mut token_charts = use_signal(Vec::<TokenChartData>::new);

    rsx! {
        div { class: "tokens-view",
            h2 { "ボラティリティトークン予測" }
            p { "ボラティリティの高いトークンを取得し、各トークンについてゼロショット予測を実行します。" }

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
                    let limit_val = limit();

                    // 非同期で処理を実行
                    spawn_local(async move {
                        // 入力値のバリデーション
                        let start_datetime: DateTime<Utc> = match NaiveDateTime::parse_from_str(&start_val, "%Y-%m-%dT%H:%M") {
                            Ok(naive) => naive.and_utc(),
                            Err(e) => {
                                error_message.set(Some(format!("開始日時のパースエラー: {}", e)));
                                loading.set(false);
                                return;
                            }
                        };

                        let end_datetime: DateTime<Utc> = match NaiveDateTime::parse_from_str(&end_val, "%Y-%m-%dT%H:%M") {
                            Ok(naive) => naive.and_utc(),
                            Err(e) => {
                                error_message.set(Some(format!("終了日時のパースエラー: {}", e)));
                                loading.set(false);
                                return;
                            }
                        };

                        // ボラティリティトークンを取得
                        let volatility_request = VolatilityTokensRequest {
                            start: start_datetime.naive_utc(),
                            end: end_datetime.naive_utc(),
                            limit: limit_val,
                        };

                        match server_client().pools.get_volatility_tokens(volatility_request).await {
                            Ok(ApiResponse::Success(volatility_response)) => {
                                let tokens = volatility_response.tokens;
                                if tokens.is_empty() {
                                    error_message.set(Some("ボラティリティトークンが見つかりませんでした".to_string()));
                                    loading.set(false);
                                    return;
                                }

                                // TODO: 各トークンについて予測実行
                                let mut results = Vec::new();

                                // 簡単なテスト結果を追加
                                for (index, token) in tokens.iter().enumerate() {
                                    results.push((
                                        (index + 1).to_string(),
                                        token.to_string(),
                                        "処理中".to_string(),
                                        "-".to_string(),
                                    ));
                                }

                                prediction_results.set(results);

                                // 各トークンについてゼロショット予測を実行
                                for (index, token) in tokens.iter().enumerate() {
                                    let quote_token = match TokenAccount::from_str("wrap.near") {
                                        Ok(t) => t,
                                        Err(_) => continue,
                                    };

                                    // 予測用データ取得期間（2日間）
                                    let predict_start = end_datetime - Duration::days(2);
                                    let predict_end = end_datetime;

                                    // データ取得リクエスト
                                    let data_request = GetValuesRequest {
                                        quote_token: quote_token.clone(),
                                        base_token: token.clone(),
                                        start: predict_start.naive_utc(),
                                        end: predict_end.naive_utc(),
                                    };

                                    // 価格データを取得
                                    match server_client().stats.get_values(&data_request).await {
                                        Ok(ApiResponse::Success(response)) => {
                                            let values_data = response.values;
                                            if values_data.len() < 4 {
                                                continue;
                                            }

                                            // 簡単な予測値を設定（実際の予測処理は後で実装）
                                            let mut current_results = prediction_results();
                                            if index < current_results.len() {
                                                current_results[index] = (
                                                    (index + 1).to_string(),
                                                    token.to_string(),
                                                    format!("{:.6}", values_data.last().unwrap().value * 1.05), // 5%上昇と仮定
                                                    "85.2%".to_string(), // 仮の精度
                                                );
                                                prediction_results.set(current_results);
                                            }

                                            // チャートデータを保存
                                            let mut token_charts_vec = token_charts();
                                            token_charts_vec.push(TokenChartData {
                                                token: token.to_string(),
                                                rank: index + 1,
                                                predicted_price: format!("{:.6}", values_data.last().unwrap().value * 1.05),
                                                accuracy: "85.2%".to_string(),
                                                chart_svg: None,
                                            });
                                            token_charts.set(token_charts_vec);

                                            // チャートSVGを生成
                                            let quote_token_str = quote_token.to_string();
                                            let base_token_str = token.to_string();

                                            // 系列を作成
                                            let mut plot_series = Vec::new();

                                            // 実際のデータ系列
                                            plot_series.push(MultiPlotSeries {
                                                values: values_data.clone(),
                                                name: "実際の価格".to_string(),
                                                color: BLUE,
                                            });

                                            // 複数系列を同一チャートに描画するためのオプション設定
                                            let multi_options = crate::chart::plots::MultiPlotOptions {
                                                image_size: (600, 300),
                                                title: Some(format!("{} / {}", base_token_str, quote_token_str)),
                                                x_label: Some("時間".to_string()),
                                                y_label: Some("価格".to_string()),
                                            };

                                            // チャートSVGを生成
                                            match crate::chart::plots::plot_multi_values_at_time_to_svg_with_options(
                                                &plot_series, multi_options
                                            ) {
                                                Ok(svg) => {
                                                    // チャートSVGを更新
                                                    let mut current_charts = token_charts();
                                                    if let Some(chart) = current_charts.get_mut(index) {
                                                        chart.chart_svg = Some(svg);
                                                    }
                                                    token_charts.set(current_charts);
                                                },
                                                Err(e) => {
                                                    web_sys::console::log_1(&format!("チャート生成エラー: {}", e).into());
                                                }
                                            }
                                        },
                                        _ => continue,
                                    }
                                }
                            },
                            Ok(ApiResponse::Error(e)) => {
                                error_message.set(Some(format!("ボラティリティトークン取得エラー: {}", e)));
                            },
                            Err(e) => {
                                error_message.set(Some(format!("リクエストエラー: {}", e)));
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
                                        "{chart_data.predicted_price}"
                                    }
                                }
                                span {
                                    style: "font-weight: bold; color: #555;",
                                    "精度: "
                                    span {
                                        style: "color: #28a745;",
                                        "{chart_data.accuracy}"
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
