use dioxus::prelude::*;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::File;
use js_sys;
use std::rc::Rc;

#[component]
pub fn ImageUpload(
    on_file_selected: EventHandler<Vec<u8>>,
) -> Element {
    let file_input_id = "file-upload-input";
    
    let file_change_handler = move |_| {
        let window = web_sys::window().expect("グローバルwindowオブジェクトがありません");
        let document = window.document().expect("現在のwindowにdocumentがありません");
        
        let input = document
            .get_element_by_id(file_input_id)
            .unwrap()
            .dyn_into::<web_sys::HtmlInputElement>()
            .unwrap();
            
        let files = input.files();
        if let Some(files) = files {
            if let Some(js_file) = files.get(0) {
                let file_obj = js_file.dyn_into::<File>().unwrap();
                let reader = Rc::new(web_sys::FileReader::new().unwrap());
                let reader_clone = reader.clone();
                
                let onloadend = Closure::wrap(Box::new(move |_: web_sys::ProgressEvent| {
                    if let Ok(result) = reader_clone.result() {
                        if let Ok(array_buffer) = result.dyn_into::<js_sys::ArrayBuffer>() {
                            let bytes = js_sys::Uint8Array::new(&array_buffer).to_vec();
                            on_file_selected.call(bytes);
                        }
                    }
                }) as Box<dyn FnMut(_)>);

                reader.set_onloadend(Some(onloadend.as_ref().unchecked_ref()));
                onloadend.forget(); // メモリリークを防ぐためにforget()を呼び出す
                reader.read_as_array_buffer(&file_obj).unwrap();
            }
        }
    };

    rsx! {
        div {
            class: "input-group",
            input {
                id: "{file_input_id}",
                class: "form-control",
                type: "file",
                accept: "image/*",
                onchange: file_change_handler,
            }
        }
    }
}
