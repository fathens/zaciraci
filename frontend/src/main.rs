use chrono;
use dioxus::prelude::*;
use dioxus_logger;
use zaciraci_common::types::{Transaction, TransactionStatus};
use reqwest;

pub use zaciraci_common::config;

fn main() {
    // ロガーを初期化
    dioxus_logger::init(dioxus_logger::tracing::Level::INFO).expect("failed to init logger");
    
    // アプリを起動
    dioxus::launch(app);
}

fn app() -> Element {
    let mut current_view = use_signal(|| "basic".to_string());

    // サンプルデータ（実際の実装ではAPIから取得）
    let _transactions = vec![
        Transaction {
            id: "tx1".to_string(),
            amount: "123.45".to_string(),
            timestamp: chrono::Utc::now(),
            status: TransactionStatus::Completed,
        },
        Transaction {
            id: "tx2".to_string(),
            amount: "67.89".to_string(),
            timestamp: chrono::Utc::now(),
            status: TransactionStatus::Pending,
        },
    ];

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
                    "basic" => rsx! { basic_view {} },
                    "pools" => rsx! { pools_view {} },
                    "storage" => rsx! { storage_view {} },
                    _ => rsx! { basic_view {} },
                }}
            }
            footer { class: "footer",
                p { " 2025 Zaciraci" }
            }
        }
    }
}

fn basic_view() -> Element {
    let client = use_signal(|| None::<reqwest::Client>);
    
    let healthcheck_result = use_signal(|| String::new());
    let balance_result = use_signal(|| String::new());
    let transfer_result = use_signal(|| String::new());
    
    // 初期化時にクライアントを接続
    use_effect(move || {
        log::info!("Connecting to client...");
        let mut client_clone = client.clone();
        
        wasm_bindgen_futures::spawn_local(async move {
            let new_one =  reqwest::Client::new() ;
            client_clone.set(Some(new_one));
            log::info!("Client connected successfully");
        });
        
        // クリーンアップ関数
        // Dioxus 0.6では戻り値が()である必要がある
        ()
    });
    
    // データを取得
    use_effect(move || {
        let client_clone = client.clone();
        let _healthcheck_result_clone = healthcheck_result.clone();
        let _balance_result_clone = balance_result.clone();
        let _transfer_result_clone = transfer_result.clone();
        
        wasm_bindgen_futures::spawn_local(async move {
            if let Some(_) = &client_clone() {
                log::info!("Fetching data...");
            }
        });
        
        // クリーンアップ関数
        ()
    });

    rsx! {
        div { class: "basic-view",
            h2 { "Basic Information" }
            p { class: "result", "Healthcheck: {healthcheck_result()}" }
            p { class: "result", "Native Token Balance: {balance_result()}" }
            p { class: "result", "Native Token Transfer: {transfer_result()}" }
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
