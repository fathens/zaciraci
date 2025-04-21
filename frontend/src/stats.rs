use chrono::{Duration, Utc};
use dioxus::prelude::*;
use dioxus_markdown::Markdown;
use std::str::FromStr;
use wasm_bindgen_futures::spawn_local;
use zaciraci_common::{
    ollama::{ChatRequest, Message},
    stats::{DescribesRequest, GetValuesRequest, ValueAtTime},
    types::TokenAccount,
};

/// 日付範囲選択コンポーネント
#[component]
pub fn DateRangeSelector(
    start_date: Signal<String>,
    end_date: Signal<String>,
    #[props(default = "")] style: &'static str,
) -> Element {
    rsx! {
        div { class: "date-container",
            style: "display: flex; gap: 10px; align-items: center; margin-bottom: 10px; {style}",
            div {
                label { class: "form-label", "Start Date:" }
                input {
                    class: "form-control",
                    type: "datetime-local",
                    value: "{start_date}",
                    oninput: move |e| start_date.set(e.value()),
                }
            }
            div {
                label { class: "form-label", "End Date:" }
                input {
                    class: "form-control",
                    type: "datetime-local",
                    value: "{end_date}",
                    oninput: move |e| end_date.set(e.value()),
                }
            }
        }
    }
}

#[component]
pub fn charts_view() -> Element {
    let client = use_signal(crate::server_api::get_client);

    let mut quote = use_signal(|| "wrap.near".to_string());
    let mut base = use_signal(|| "mark.gra-fun.near".to_string());
    let now = Utc::now();
    let one_hour_ago = now - Duration::hours(1);
    let start_date = use_signal(|| one_hour_ago.format("%Y-%m-%dT%H:%M:%S").to_string());
    let end_date = use_signal(|| now.format("%Y-%m-%dT%H:%M:%S").to_string());
    let mut period = use_signal(|| "1m".to_string());
    let mut values = use_signal(|| None::<Vec<ValueAtTime>>);
    let mut chart_svg = use_signal(|| None::<String>);
    let mut loading = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);

    rsx! {
        div { class: "chart-view",
            h2 { "価格チャート" }
            div { class: "quote-container",
                style: "display: flex; align-items: center; margin-bottom: 10px;",
                label { class: "form-label", "Quote Token:" }
                input {
                    class: "form-control",
                    value: "{quote}",
                    oninput: move |e| quote.set(e.value()),
                }
            }
            div { class: "base-container",
                style: "display: flex; align-items: center; margin-bottom: 10px;",
                label { class: "form-label", "Base Token:" }
                input {
                    class: "form-control",
                    value: "{base}",
                    oninput: move |e| base.set(e.value()),
                }
            }
            DateRangeSelector {
                start_date: start_date,
                end_date: end_date,
                style: "margin-bottom: 10px;",
            }
            div { class: "period-container",
                style: "display: flex; align-items: center; margin-bottom: 10px;",
                label { class: "form-label", "Period:" }
                input {
                    class: "form-control",
                    value: "{period}",
                    oninput: move |e| period.set(e.value()),
                }
                span { class: "form-text", style: "margin-left: 5px;", "(例: 1m, 5m, 1h)" }
            }
            button {
                class: "btn btn-primary",
                disabled: "{loading}",
                onclick: move |_| {
                    loading.set(true);
                    error_message.set(None);
                    chart_svg.set(None);
                    
                    spawn_local(async move {
                        let quote_token_account = TokenAccount::from_str(&quote().clone()).unwrap();
                        let base_token_account = TokenAccount::from_str(&base().clone()).unwrap();
                        let request = GetValuesRequest {
                            quote_token: quote_token_account,
                            base_token: base_token_account,
                            start: match start_date().parse() {
                                Ok(date) => date,
                                Err(e) => {
                                    error_message.set(Some(format!("開始日時のパースエラー: {}", e)));
                                    loading.set(false);
                                    return;
                                }
                            },
                            end: match end_date().parse() {
                                Ok(date) => date,
                                Err(e) => {
                                    error_message.set(Some(format!("終了日時のパースエラー: {}", e)));
                                    loading.set(false);
                                    return;
                                }
                            },
                        };
                        
                        match client().stats.get_values(&request).await {
                            Ok(response) => {
                                values.set(Some(response.values));
                                
                                if let Some(values_data) = values() {
                                    if values_data.is_empty() {
                                        error_message.set(Some("データが見つかりませんでした".to_string()));
                                    } else {
                                        // チャートをプロット
                                        let options = crate::chart::plots::PlotOptions {
                                            image_size: (800, 400),
                                            title: Some(format!("{} / {}", quote(), base())),
                                            x_label: Some("時間".to_string()),
                                            y_label: Some("価格".to_string()),
                                            ..Default::default()
                                        };
                                        
                                        match crate::chart::plots::plot_values_at_time_to_svg_with_options(
                                            &values_data, options
                                        ) {
                                            Ok(svg) => chart_svg.set(Some(svg)),
                                            Err(e) => error_message.set(Some(format!("チャート作成エラー: {}", e))),
                                        }
                                    }
                                }
                            },
                            Err(e) => {
                                error_message.set(Some(format!("データ取得エラー: {}", e)));
                            },
                        }
                        
                        loading.set(false);
                    });
                },
                if loading() { "読み込み中..." } else { "チャート表示" }
            }
            
            // エラーメッセージの表示
            if let Some(error) = error_message() {
                div { class: "alert alert-danger", "{error}" }
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

#[component]
pub fn stats_analysis_view() -> Element {
    let client = use_signal(crate::server_api::get_client);

    let mut quote = use_signal(|| "wrap.near".to_string());
    let mut base = use_signal(|| "mark.gra-fun.near".to_string());
    let now = Utc::now();
    let one_hour_ago = now - Duration::hours(1);
    let start_date = use_signal(|| one_hour_ago.format("%Y-%m-%dT%H:%M:%S").to_string());
    let end_date = use_signal(|| now.format("%Y-%m-%dT%H:%M:%S").to_string());
    let mut period = use_signal(|| "1m".to_string());
    let mut descs = use_signal(|| "".to_string());
    
    rsx! {
        div { class: "stats-analysis-view",
            h2 { "統計分析" }
            div { class: "quote-container",
                style: "display: flex; align-items: center; margin-bottom: 10px;",
                label { class: "form-label", "Quote Token:" }
                input {
                    class: "form-control",
                    value: "{quote}",
                    oninput: move |e| quote.set(e.value()),
                }
            }
            div { class: "base-container",
                style: "display: flex; align-items: center; margin-bottom: 10px;",
                label { class: "form-label", "Base Token:" }
                input {
                    class: "form-control",
                    value: "{base}",
                    oninput: move |e| base.set(e.value()),
                }
            }
            DateRangeSelector {
                start_date: start_date,
                end_date: end_date,
                style: "margin-bottom: 10px;",
            }
            div { class: "period-container",
                style: "display: flex; align-items: center; margin-bottom: 10px;",
                label { class: "form-label", "Period:" }
                input {
                    class: "form-control",
                    value: "{period}",
                    oninput: move |e| period.set(e.value()),
                }
                span { class: "form-text", style: "margin-left: 5px;", "(例: 1m, 5m, 1h)" }
            }
            button {
                class: "btn btn-primary",
                onclick: move |_| {
                    descs.set("Loading...".to_string());
                    spawn_local(async move {
                        let request = DescribesRequest {
                            quote_token: quote().clone(),
                            base_token: base().clone(),
                            start: start_date().parse().unwrap(),
                            end: end_date().parse().unwrap(),
                            period: parse_duration(&period()).unwrap(),
                        };
                        match client().stats.describes(&request).await {
                            Ok(text) => descs.set(text),
                            Err(e) => {
                                descs.set(format!("Error: {}", e));
                            },
                        }
                    });
                },
                "Get Describes"
            }
            
            div { class: "descs-container",
                style: "width: 100%; margin-top: 20px;",
                textarea {
                    class: "form-control",
                    style: "width: 100%;",
                    rows: "8",
                    value: "{descs}",
                    oninput: move |e| descs.set(e.value()),
                }
            }
        }
    }
}

#[component]
pub fn forecast_view() -> Element {
    let client = use_signal(crate::server_api::get_client);

    let _quote = use_signal(|| "wrap.near".to_string());
    let _base = use_signal(|| "mark.gra-fun.near".to_string());
    let now = Utc::now();
    let one_hour_ago = now - Duration::hours(1);
    let _start_date = use_signal(|| one_hour_ago.format("%Y-%m-%dT%H:%M:%S").to_string());
    let _end_date = use_signal(|| now.format("%Y-%m-%dT%H:%M:%S").to_string());
    let _period = use_signal(|| "1m".to_string());
    let mut descs = use_signal(|| "".to_string());

    let mut models = use_signal(Vec::new);
    let mut selected_model = use_signal(|| "".to_string());
    let mut prompt_role = use_signal(|| "user".to_string());
    let mut forecast_header =
        use_signal(|| "Forecast the price in two hours using this information.".to_string());
    let mut dur_in_sec = use_signal(|| "".to_string());
    let mut forecast_result = use_signal(|| "".to_string());

    rsx! {
        div { class: "forecast-container",
            style: "width: 100%;",
            textarea {
                class: "form-control",
                style: "width: 100%;",
                rows: "8",
                value: "{forecast_header}",
                oninput: move |e| forecast_header.set(e.value()),
            }
        }
        div { class: "descs-container",
            style: "width: 100%;",
            textarea {
                class: "form-control",
                style: "width: 100%;",
                rows: "8",
                value: "{descs}",
                oninput: move |e| descs.set(e.value()),
            }
        }
        div { class: "model-container",
            style: "display: flex; align-items: center; margin-bottom: 10px;",
            button {
                class: "btn btn-primary",
                onclick: move |_| {
                    spawn_local(async move {
                        let model_names = client().ollama.list_models().await;
                        models.set(model_names);
                    });
                },
                "Fetch models"
            }
            select {
                class: "form-control",
                value: "{selected_model}",
                oninput: move |e| selected_model.set(e.value()),
                option { value: "", "Select a model" }
                for model in models().iter() {
                    option {
                        value: "{model}",
                        "{model}"
                    }
                }
            }
        }
        div { class: "role-container",
            style: "display: flex; align-items: center; margin-bottom: 10px;",
            select {
                class: "form-control",
                value: "{prompt_role}",
                oninput: move |e| prompt_role.set(e.value()),
                option { value: "user", "User" }
                option { value: "assistant", "Assistant" }
                option { value: "system", "System" }
            }
        }
        div { class: "ollama-container",
            style: "display: flex; align-items: center; margin-bottom: 10px;",
            button {
                class: "btn btn-primary",
                onclick: move |_| {
                    dur_in_sec.set("".to_string());
                    forecast_result.set("Loading...".to_string());
                    spawn_local(async move {
                        let start_time = js_sys::Date::now();
                        let response = client().ollama.chat(&ChatRequest {
                            model_name: selected_model().clone(),
                            messages: vec![
                                Message {
                                    role: prompt_role().clone(),
                                    content: forecast_header().clone() + "\n---\n" + &descs(),
                                }
                            ],
                        }).await;
                        let end_time = js_sys::Date::now();
                        let duration_ms = end_time - start_time;
                        dur_in_sec.set(format!("{:0.2} seconds", duration_ms / 1000.0));
                        forecast_result.set(response);
                    });
                },
                "Forecast"
            }
        }
        div { class: "duration-container",
            style: "display: flex; align-items: center; margin-bottom: 10px;",
            "{dur_in_sec}"
        }
        div { class: "response-container",
            style: "display: flex; align-items: center; margin-bottom: 10px;",
            Markdown {
                src: forecast_result
            }
        }
    }
}

#[component]
pub fn view() -> Element {
    let _client = use_signal(crate::server_api::get_client);

    rsx! {
        div { class: "stats-container",
            style: "display: flex; flex-direction: column; width: 100%;",
            
            // チャート表示コンポーネント
            div { class: "charts-section",
                charts_view {}
            }
            
            // 統計分析コンポーネント
            div { class: "stats-analysis-section",
                stats_analysis_view {}
            }
            
            // 予測機能コンポーネント
            div { class: "forecast-section",
                forecast_view {}
            }
        }
    }
}

fn parse_duration(s: &str) -> anyhow::Result<Duration> {
    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: i64 = num_str.parse()?;

    match unit {
        "s" => Ok(Duration::seconds(num)),
        "m" => Ok(Duration::minutes(num)),
        "h" => Ok(Duration::hours(num)),
        "d" => Ok(Duration::days(num)),
        _ => Err(anyhow::anyhow!("Invalid duration unit: {}", unit)),
    }
}
