use crate::Result;
use anyhow::anyhow;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

static CONFIG_STORE: Lazy<Arc<Mutex<HashMap<String, String>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

pub(crate) static DB_STORE: Lazy<Arc<Mutex<HashMap<String, String>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

/// Resolve a configuration value through the priority chain:
/// CONFIG_STORE > DB_STORE > env > Err
#[doc(hidden)]
pub fn get(name: &str) -> Result<String> {
    // Priority 1: CONFIG_STORE (runtime overrides)
    if let Some(value) = get_from_store(name) {
        if value.is_empty() {
            return Err(anyhow!("{} is empty", name));
        }
        return Ok(value);
    }

    // Priority 2: DB_STORE (database config)
    if let Some(value) = get_from_db_store(name) {
        if value.is_empty() {
            return Err(anyhow!("{} is empty", name));
        }
        return Ok(value);
    }

    get_from_env(name)
}

/// Resolve a configuration value excluding DB_STORE:
/// CONFIG_STORE > env > Err
///
/// DB に値がない場合の「実効値」を取得するために使用する。
#[doc(hidden)]
pub fn get_excluding_db(name: &str) -> Result<String> {
    // Priority 1: CONFIG_STORE (runtime overrides)
    if let Some(value) = get_from_store(name) {
        if value.is_empty() {
            return Err(anyhow!("{} is empty", name));
        }
        return Ok(value);
    }

    // (DB_STORE をスキップ)

    get_from_env(name)
}

fn get_from_env(name: &str) -> Result<String> {
    if let Ok(val) = std::env::var(name)
        && !val.is_empty()
    {
        return Ok(val);
    }

    Err(anyhow!("Configuration key not found: {}", name))
}

/// テスト用: 設定値を上書きする
///
/// 注: `#[cfg(test)]` にすると他クレート(backend等)のテストから参照できないため
/// `#[doc(hidden)]` で公開している
#[doc(hidden)]
pub fn set(name: &str, value: &str) {
    if let Ok(mut store) = CONFIG_STORE.lock() {
        store.insert(name.to_string(), value.to_string());
    }
}

/// テスト用: 設定値を CONFIG_STORE から削除する
#[doc(hidden)]
pub fn remove(name: &str) {
    if let Ok(mut store) = CONFIG_STORE.lock() {
        store.remove(name);
    }
}

/// テスト用: CONFIG_STORE に値をセットし、Drop 時に自動で元に戻す RAII ガード。
///
/// テストが途中で panic しても確実にクリーンアップされる。
#[doc(hidden)]
pub struct ConfigGuard {
    key: String,
    previous: Option<String>,
}

impl ConfigGuard {
    pub fn new(key: &str, value: &str) -> Self {
        let previous = get_from_store(key);
        set(key, value);
        Self {
            key: key.to_string(),
            previous,
        }
    }
}

impl Drop for ConfigGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(prev) => set(&self.key, prev),
            None => remove(&self.key),
        }
    }
}

/// テスト用: DB_STORE 全体を保存し、Drop 時に復元する RAII ガード。
#[doc(hidden)]
pub struct DbStoreGuard {
    previous: HashMap<String, String>,
}

impl Default for DbStoreGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl DbStoreGuard {
    pub fn new() -> Self {
        let previous = if let Ok(store) = DB_STORE.lock() {
            store.clone()
        } else {
            HashMap::new()
        };
        Self { previous }
    }
}

impl Drop for DbStoreGuard {
    fn drop(&mut self) {
        if let Ok(mut store) = DB_STORE.lock() {
            store.clear();
            store.extend(std::mem::take(&mut self.previous));
        }
    }
}

/// テスト用: 環境変数を設定し、Drop 時に元に戻す RAII ガード。
#[doc(hidden)]
pub struct EnvGuard {
    key: String,
    previous: Option<String>,
}

impl EnvGuard {
    /// 環境変数を `value` に設定する。Drop 時に元の値に復元される。
    pub fn set(key: &str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        // SAFETY: テスト専用。#[serial] で排他制御されている前提。
        unsafe {
            std::env::set_var(key, value);
        }
        Self {
            key: key.to_string(),
            previous,
        }
    }

    /// 環境変数を削除する。Drop 時に元の値に復元される。
    pub fn remove(key: &str) -> Self {
        let previous = std::env::var(key).ok();
        // SAFETY: テスト専用。#[serial] で排他制御されている前提。
        unsafe {
            std::env::remove_var(key);
        }
        Self {
            key: key.to_string(),
            previous,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        // SAFETY: テスト専用。#[serial] で排他制御されている前提。
        unsafe {
            match &self.previous {
                Some(prev) => std::env::set_var(&self.key, prev),
                None => std::env::remove_var(&self.key),
            }
        }
    }
}

pub(crate) fn get_from_store(name: &str) -> Option<String> {
    if let Ok(store) = CONFIG_STORE.lock() {
        store.get(name).cloned()
    } else {
        None
    }
}

pub(crate) fn get_from_db_store(name: &str) -> Option<String> {
    if let Ok(store) = DB_STORE.lock() {
        store.get(name).cloned()
    } else {
        None
    }
}

/// DB から取得した設定を DB_STORE にロードする
///
/// 既存の DB_STORE を全て置き換える（リロード動作）。
pub fn load_db_config(configs: HashMap<String, String>) {
    if let Ok(mut store) = DB_STORE.lock() {
        store.clear();
        store.extend(configs);
    }
}
