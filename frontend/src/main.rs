mod api_underlying;
mod basic;
mod chart;
mod chronos_api;
mod errors;
mod image_upload;
mod ollama;
mod pools;
mod predict;
mod prediction_config;
mod prediction_utils;
mod server_api;
mod services;
mod stats;
mod storage;
mod tokens;

use dioxus::prelude::*;

pub use server_api::get_client;
pub use zaciraci_common::config;

fn main() {
    // ロガーを初期化
    dioxus_logger::init(dioxus_logger::tracing::Level::DEBUG).expect("failed to init logger");

    // アプリを起動
    dioxus::launch(app);
}

#[component]
fn app() -> Element {
    let mut current_view = use_signal(|| "basic".to_string());

    rsx! {
        div { class: "container",
            header { class: "header",
                h1 { "Zaciraci" }
                nav { class: "nav",
                    button {
                        onclick: move |_| current_view.set("basic".to_string()),
                        class: if current_view() == "basic" { "active" } else { "" },
                        "Basic"
                    }
                    button {
                        onclick: move |_| current_view.set("pools".to_string()),
                        class: if current_view() == "pools" { "active" } else { "" },
                        "Pools"
                    }
                    button {
                        onclick: move |_| current_view.set("storage".to_string()),
                        class: if current_view() == "storage" { "active" } else { "" },
                        "Storage"
                    }
                    button {
                        onclick: move |_| current_view.set("ollama".to_string()),
                        class: if current_view() == "ollama" { "active" } else { "" },
                        "Ollama"
                    }
                    button {
                        onclick: move |_| current_view.set("stats".to_string()),
                        class: if current_view() == "stats" { "active" } else { "" },
                        "Stats"
                    }
                    button {
                        onclick: move |_| current_view.set("predict".to_string()),
                        class: if current_view() == "predict" { "active" } else { "" },
                        "predict"
                    }
                    button {
                        onclick: move |_| current_view.set("tokens".to_string()),
                        class: if current_view() == "tokens" { "active" } else { "" },
                        "Tokens"
                    }
                }
            }
            main { class: "main",
                {match current_view().as_str() {
                    "basic" => rsx! { basic::view {} },
                    "pools" => rsx! { pools::view {} },
                    "storage" => rsx! { storage::view {} },
                    "ollama" => rsx! { ollama::view {} },
                    "stats" => rsx! { stats::view {} },
                    "predict" => rsx! { predict::view {} },
                    "tokens" => rsx! { tokens::view {} },
                    _ => rsx! { basic::view {} },
                }}
            }
            footer { class: "footer",
                p { " 2025 Zaciraci" }
            }
        }
    }
}
