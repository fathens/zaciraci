mod basic;
mod server_api;

use dioxus::prelude::*;
use dioxus_logger;

pub use zaciraci_common::config;
pub use server_api::get_client;

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
                }
            }
            main { class: "main",
                {match current_view().as_str() {
                    "basic" => rsx! { basic::view {} },
                    "pools" => rsx! { pools_view {} },
                    "storage" => rsx! { storage_view {} },
                    _ => rsx! { basic::view {} },
                }}
            }
            footer { class: "footer",
                p { " 2025 Zaciraci" }
            }
        }
    }
}

fn pools_view() -> Element {
    rsx! {
        div { class: "pools-view",
            h2 { "Pools Management" }
            p { "プールの一覧と詳細情報を表示します" }
        }
    }
}

fn storage_view() -> Element {
    rsx! {
        div { class: "storage-view",
            h2 { "Storage Management" }
            p { "ストレージの使用状況や詳細情報を表示します" }
        }
    }
}
