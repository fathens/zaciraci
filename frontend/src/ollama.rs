use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;

#[component]
pub fn view() -> Element {
    let client = use_signal(|| crate::server_api::get_client());
    
    let mut models = use_signal(|| Vec::new());
    let mut selected_model = use_signal(|| "".to_string());

    rsx! {
        div { class: "ollama-view",
            div { class: "input-group",
                button {
                    class: "btn btn-primary",
                    onclick: move |_| {
                        spawn_local(async move {
                            let model_names = client().ollama_list_models().await;
                            models.set(model_names);
                        });
                    },
                    "Fetch models"
                }
            }
            div { class: "input-group",
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
        }
    }
}