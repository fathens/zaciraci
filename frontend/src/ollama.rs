use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;
use serde_json;

#[component]
pub fn view() -> Element {
    let client = use_signal(|| crate::server_api::get_client());
    
    let mut port = use_signal(|| "11434".to_string());
    let mut models = use_signal(|| Vec::new());
    let mut selected_model = use_signal(|| "".to_string());

    rsx! {
        div { class: "ollama-view",
            div { class: "input-group",
                input {
                    class: "form-control",
                    type: "number",
                    min: "1",
                    max: "65535",
                    placeholder: "11434",
                    value: "{port}",
                    oninput: move |e| port.set(e.value()),
                }
                button {
                    class: "btn btn-primary",
                    onclick: move |_| {
                        spawn_local(async move {
                            let model_names = client().ollama_list_models(port().parse().unwrap()).await;
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