use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;

#[component]
pub fn view() -> Element {
    let client = use_signal(|| crate::server_api::get_client());
    
    let mut healthcheck_result = use_signal(|| "None".to_string());
    let mut balance_result = use_signal(|| "None".to_string());
    let mut transfer_result = use_signal(|| "None".to_string());
    let mut receiver = use_signal(|| "".to_string());
    let mut amount = use_signal(|| "".to_string());

    let on_healthcheck = move |_| {
        spawn_local(async move {
            let text = client().basic.healthcheck().await;
            healthcheck_result.set(text);
        });
    };

    let on_native_token_balance = move |_| {
        spawn_local(async move {
            let text = client().basic.native_token_balance().await;
            balance_result.set(text);
        });
    };

    let on_native_token_transfer = move |_| {
        spawn_local(async move {
            let text = client().basic.native_token_transfer(&receiver(), &amount()).await;
            transfer_result.set(text);
        });
    };

    rsx! {
        div { class: "basic-view",
            h2 { "Basic Information" }
            div { class: "healthcheck-container",
                style: "display: flex; align-items: center;",
                button {
                    onclick: on_healthcheck,
                    "Healthcheck"
                }
                p { class: "result", ": {healthcheck_result}" }
            }
            div { class: "balance-container",
                style: "display: flex; align-items: center;",
                button {
                    onclick: on_native_token_balance,
                    "Native Token Balance"
                }
                p { class: "result", ": {balance_result}" }
            }
            div { class: "transfer-container",
                style: "display: flex; align-items: center;",
                p { "Receiver: " }
                input {
                    type: "text", name: "receiver", value: "{receiver}",
                    oninput: move |e| receiver.set(e.value())
                }
                p { "Amount: " }
                input {
                    type: "text", name: "amount", value: "{amount}",
                    oninput: move |e| amount.set(e.value())
                }
                button {
                    onclick: on_native_token_transfer,
                    "Native Token Transfer"
                }
                p { class: "result", ": {transfer_result}" }
            }
        }
    }
}
