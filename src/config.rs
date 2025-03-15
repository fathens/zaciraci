use crate::Result;
use anyhow::anyhow;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

static CONFIG_STORE: Lazy<Arc<Mutex<HashMap<String, String>>>> = Lazy::new(|| {
    Arc::new(Mutex::new(HashMap::new()))
});

pub fn get(name: &str) -> Result<String> {
    // まずハッシュマップから値を取得しようとする
    if let Some(value) = get_from_store(name) {
        if value.is_empty() {
            return Err(anyhow!("{} is empty", name));
        }
        return Ok(value);
    }

    // ハッシュマップになければ環境変数から取得
    match std::env::var(name) {
        Ok(val) => {
            if val.is_empty() {
                Err(anyhow!("{} is empty", name))
            } else {
                Ok(val)
            }
        }
        Err(e) => Err(anyhow!("{}: {}", e, name)),
    }
}

#[allow(dead_code)]
// This function is not used in the code, but it is needed for tests
pub fn set(name: &str, value: &str) {
    if let Ok(mut store) = CONFIG_STORE.lock() {
        store.insert(name.to_string(), value.to_string());
    }
}

fn get_from_store(name: &str) -> Option<String> {
    if let Ok(store) = CONFIG_STORE.lock() {
        store.get(name).cloned()
    } else {
        None
    }
}
