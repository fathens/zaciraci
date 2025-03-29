use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;

#[component]
pub fn view() -> Element {
    let client = use_signal(|| crate::server_api::get_client());
    
    let mut get_all_pools_result = use_signal(|| "None".to_string());
    let on_get_all_pools = move |_| {
        spawn_local(async move {
            let text = client().pools.get_all_pools().await;
            get_all_pools_result.set(text);
        });
    };

    rsx! {
        div { class: "pools-view",
            h2 { "Pools Management" }
            div { class: "get_all_pools-container",
                style: "display: flex; align-items: center;",
                button {
                    onclick: on_get_all_pools,
                    class: if get_all_pools_result() == "None" { "active" } else { "" },
                    "Get All Pools"
                }
                p { class: "result", ": {get_all_pools_result}" }
            }
        }
    }
}
