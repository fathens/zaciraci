use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;
use zaciraci_common::stats::DescribesRequest;
use chrono::{Duration, Utc};

#[component]
pub fn view() -> Element {
    let client = use_signal(|| crate::server_api::get_client());
    
    let mut quote = use_signal(|| "wrap.near".to_string());
    let mut base = use_signal(|| "usdt.tether-token.near".to_string());
    let now = Utc::now();
    let one_hour_ago = now - Duration::hours(1);
    let mut start_date = use_signal(|| one_hour_ago.format("%Y-%m-%dT%H:%M:%S").to_string());
    let mut end_date = use_signal(|| now.format("%Y-%m-%dT%H:%M:%S").to_string());
    let mut period = use_signal(|| "10s".to_string());
    
    let mut result = use_signal(|| "".to_string());
    
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
                    result.set("Loading...".to_string());
                    spawn_local(async move {
                        let request = DescribesRequest {
                            quote_token: quote().clone(),
                            base_token: base().clone(),
                            start: start_date().parse().unwrap(),
                            end: end_date().parse().unwrap(),
                            period: parse_duration(&period()).unwrap(),
                        };
                        let text = client().stats.describes(&request).await;
                        result.set(text);
                    });
                },
                "Get Describes"
            }
            p { class: "result", "{result}" }
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