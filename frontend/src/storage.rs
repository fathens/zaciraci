use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;

#[component]
pub fn view() -> Element {
    let client = use_signal(crate::server_api::get_client);

    let mut storage_deposit_min_result = use_signal(|| "None".to_string());
    let on_storage_deposit_min = move |_| {
        spawn_local(async move {
            let text = client().storage.deposit_min().await;
            storage_deposit_min_result.set(text);
        });
    };

    rsx! {
        div { class: "storage-view",
            h2 { "Storage Management" }
            div { class: "storage_deposit_min-container",
                style: "display: flex; align-items: center;",
                button {
                    onclick: on_storage_deposit_min,
                    class: if storage_deposit_min_result() == "None" { "active" } else { "" },
                    "Get Storage Deposit Min"
                }
                p { class: "result", ": {storage_deposit_min_result}" }
            }
        }
    }
}
