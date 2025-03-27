use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;

#[component]
pub fn view() -> Element {
    rsx! {
        div { class: "ollama-view", "Hello, world!" }
    }
}