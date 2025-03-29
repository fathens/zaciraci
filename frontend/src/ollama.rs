use crate::image_upload::ImageUpload;
use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;
use zaciraci_common::ollama::{ChatRequest, GenerateRequest, Image, Message};
use js_sys::Date;

#[component]
pub fn view() -> Element {
    let client = use_signal(|| crate::server_api::get_client());

    let mut models = use_signal(|| Vec::new());
    let mut selected_model = use_signal(|| "".to_string());
    let mut prompt_role = use_signal(|| "user".to_string());
    let mut prompt = use_signal(|| "".to_string());
    let mut image_data = use_signal(|| None);
    let mut res_msg = use_signal(|| "".to_string());
    let mut dur_in_sec = use_signal(|| "".to_string());

    rsx! {
        div { class: "ollama-view",
            h2 { "Ollama Management" }
            div { class: "models-container",
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
            div { class: "chat-container",
                style: "display: flex; flex-direction: column; margin-bottom: 10px;",
                textarea {
                    class: "form-control",
                    rows: "4",
                    value: "{prompt_role}",
                    oninput: move |e| prompt_role.set(e.value()),
                }
                textarea {
                    class: "form-control",
                    rows: "8",
                    value: "{prompt}",
                    oninput: move |e| prompt.set(e.value()),
                }
                button {
                    class: "btn btn-primary",
                    onclick: move |_| {
                        res_msg.set("generating...".to_string());
                        spawn_local(async move {
                            let start_time = Date::now();
                            let response = client().ollama.chat(&ChatRequest {
                                model_name: selected_model().clone(),
                                messages: vec![
                                    Message {
                                        role: prompt_role().clone(),
                                        content: prompt().clone(),
                                    }
                                ],
                            }).await;
                            let end_time = Date::now();
                            let duration_ms = end_time - start_time;
                            dur_in_sec.set(format!("{:0.2} seconds", duration_ms / 1000.0));
                            res_msg.set(response);
                        });
                    },
                    "Chat"
                }
            }
            div { class: "generate-container",
                style: "display: flex; align-items: center; margin-bottom: 10px;",
                ImageUpload {
                    on_file_selected: move |data| {
                        image_data.set(Some(data));
                    }
                }
                button {
                    class: "btn btn-primary",
                    onclick: move |_| {
                        res_msg.set("generating...".to_string());
                        spawn_local(async move {
                            let mut images = Vec::new();
                            if let Some(image_data) = image_data() {
                                let image = Image::from_bytes(&image_data);
                                images.push(image);
                            }
                            let start_time = Date::now();
                            let response = client().ollama.generate(&GenerateRequest {
                                model_name: selected_model().clone(),
                                prompt: prompt().clone(),
                                images: images,
                            }).await;
                            let end_time = Date::now();
                            let duration_ms = end_time - start_time;
                            dur_in_sec.set(format!("{:0.2} seconds", duration_ms / 1000.0));
                            res_msg.set(response);
                        });
                    },
                    "Generate"
                }
            }
            div { class: "duration-container",
                style: "display: flex; align-items: center; margin-bottom: 10px;",
                "{dur_in_sec}"
            }
            div { class: "response-container",
                style: "display: flex; align-items: center; margin-bottom: 10px;",
                "{res_msg}"
            }
        }
    }
}
