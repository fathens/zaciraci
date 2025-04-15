use chrono::{Duration, Utc};
use dioxus::prelude::*;
use dioxus_markdown::Markdown;
use wasm_bindgen_futures::spawn_local;
use zaciraci_common::{
    ollama::{ChatRequest, Message},
    stats::DescribesRequest,
};

#[component]
pub fn view() -> Element {
    let client = use_signal(crate::server_api::get_client);

    let mut quote = use_signal(|| "wrap.near".to_string());
    let mut base = use_signal(|| "mark.gra-fun.near".to_string());
    let now = Utc::now();
    let one_hour_ago = now - Duration::hours(1);
    let mut start_date = use_signal(|| one_hour_ago.format("%Y-%m-%dT%H:%M:%S").to_string());
    let mut end_date = use_signal(|| now.format("%Y-%m-%dT%H:%M:%S").to_string());
    let mut period = use_signal(|| "1m".to_string());
    let mut descs = use_signal(|| "".to_string());

    let mut models = use_signal(Vec::new);
    let mut selected_model = use_signal(|| "".to_string());
    let mut prompt_role = use_signal(|| "user".to_string());
    let mut forecast_header =
        use_signal(|| "Forecast the price in two hours using this information.".to_string());
    let mut dur_in_sec = use_signal(|| "".to_string());
    let mut forecast_result = use_signal(|| "".to_string());

    rsx! {
        div { class: "stats-view",
            h2 { "Stats" }
            div { class: "quote-container",
                style: "display: flex; align-items: center;",
                input {
                    class: "form-control",
                    value: "{quote}",
                    oninput: move |e| quote.set(e.value()),
                }
            }
            div { class: "base-container",
                style: "display: flex; align-items: center;",
                input {
                    class: "form-control",
                    value: "{base}",
                    oninput: move |e| base.set(e.value()),
                }
            }
            div { class: "date-container",
                style: "display: flex; gap: 10px; align-items: center;",
                "Start Date:"
                input {
                    class: "form-control",
                    type: "datetime-local",
                    value: "{start_date}",
                    oninput: move |e| start_date.set(e.value()),
                }
                "End Date:"
                input {
                    class: "form-control",
                    type: "datetime-local",
                    value: "{end_date}",
                    oninput: move |e| end_date.set(e.value()),
                }
            }
            div { class: "period-container",
                style: "display: flex; align-items: center;",
                input {
                    class: "form-control",
                    value: "{period}",
                    oninput: move |e| period.set(e.value()),
                }
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
        }
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
