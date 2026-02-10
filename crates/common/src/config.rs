use crate::Result;
use anyhow::anyhow;
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

// TOML configuration structure
#[derive(Debug, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub network: NetworkConfig,
    #[serde(default)]
    pub wallet: WalletConfig,
    #[serde(default)]
    pub rpc: RpcConfig,
    #[serde(default)]
    pub external_services: ExternalServicesConfig,
    #[serde(default)]
    pub trade: TradeConfig,
    #[serde(default)]
    pub cron: CronConfig,
    #[serde(default)]
    pub harvest: HarvestConfig,
    #[serde(default)]
    pub arbitrage: ArbitrageConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
}

#[derive(Debug, Deserialize)]
pub struct NetworkConfig {
    #[serde(default = "default_use_mainnet")]
    pub use_mainnet: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct WalletConfig {
    #[serde(default)]
    pub root_account_id: String,
    #[serde(default)]
    pub root_mnemonic: String,
    #[serde(default = "default_hdpath")]
    pub root_hdpath: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct RpcConfig {
    #[serde(default)]
    pub endpoints: Vec<RpcEndpoint>,
    #[serde(default)]
    pub settings: RpcSettings,
}

#[derive(Debug, Deserialize, serde::Serialize, Clone)]
pub struct RpcEndpoint {
    pub url: String,
    #[serde(default = "default_weight")]
    pub weight: u32,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

#[derive(Debug, Deserialize)]
pub struct RpcSettings {
    #[serde(default = "default_failure_reset_seconds")]
    pub failure_reset_seconds: u64,
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
}

#[derive(Debug, Deserialize, Default)]
pub struct ExternalServicesConfig {}

#[derive(Debug, Deserialize)]
pub struct TradeConfig {
    #[serde(default = "default_trade_enabled")]
    pub enabled: bool,
    #[serde(default = "default_initial_investment")]
    pub initial_investment: u32,
    #[serde(default = "default_top_tokens")]
    pub top_tokens: u32,
    #[serde(default = "default_evaluation_days")]
    pub evaluation_days: u32,
    #[serde(default = "default_account_reserve")]
    pub account_reserve: u32,
    #[serde(default = "default_cron_schedule")]
    pub cron_schedule: String,
    #[serde(default = "default_prediction_max_retries")]
    pub prediction_max_retries: u32,
    #[serde(default = "default_prediction_retry_delay_seconds")]
    pub prediction_retry_delay_seconds: u64,
    #[serde(default = "default_price_history_days")]
    pub price_history_days: u32,
    #[serde(default = "default_volatility_days")]
    pub volatility_days: u32,
    #[serde(default = "default_unwrap_on_stop")]
    pub unwrap_on_stop: bool,
    #[serde(default = "default_prediction_concurrency")]
    pub prediction_concurrency: u32,
    #[serde(default = "default_min_pool_liquidity")]
    pub min_pool_liquidity: u32,
}

#[derive(Debug, Deserialize)]
pub struct CronConfig {
    #[serde(default = "default_pool_info_retention_count")]
    pub pool_info_retention_count: u32,
    #[serde(default = "default_token_rates_retention_days")]
    pub token_rates_retention_days: u32,
}

#[derive(Debug, Deserialize)]
pub struct HarvestConfig {
    #[serde(default)]
    pub account_id: String,
    #[serde(default = "default_harvest_min_amount")]
    pub min_amount: u32,
    #[serde(default = "default_harvest_reserve_amount")]
    pub reserve_amount: u32,
    #[serde(default = "default_harvest_interval_seconds")]
    pub interval_seconds: u64,
}

impl Default for HarvestConfig {
    fn default() -> Self {
        Self {
            account_id: String::new(),
            min_amount: default_harvest_min_amount(),
            reserve_amount: default_harvest_reserve_amount(),
            interval_seconds: default_harvest_interval_seconds(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ArbitrageConfig {
    #[serde(default)]
    pub needed: bool,
    #[serde(default = "default_token_not_found_wait")]
    pub token_not_found_wait: String,
    #[serde(default = "default_other_error_wait")]
    pub other_error_wait: String,
    #[serde(default = "default_preview_not_found_wait")]
    pub preview_not_found_wait: String,
}

#[derive(Debug, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_rust_log_format")]
    pub rust_log_format: String,
}

// Default values
fn default_use_mainnet() -> bool {
    true
}
fn default_hdpath() -> String {
    "m/44'/397'/0'".to_string()
}
fn default_weight() -> u32 {
    10
}
fn default_max_retries() -> u32 {
    3
}
fn default_failure_reset_seconds() -> u64 {
    300
}
fn default_max_attempts() -> u32 {
    10
}
fn default_trade_enabled() -> bool {
    false
}
fn default_initial_investment() -> u32 {
    100
}
fn default_top_tokens() -> u32 {
    10
}
fn default_evaluation_days() -> u32 {
    10
}
fn default_account_reserve() -> u32 {
    10
}
fn default_cron_schedule() -> String {
    "0 0 0 * * *".to_string()
}
fn default_prediction_max_retries() -> u32 {
    2
}
fn default_prediction_retry_delay_seconds() -> u64 {
    5
}
fn default_price_history_days() -> u32 {
    30
}
fn default_volatility_days() -> u32 {
    7
}
fn default_unwrap_on_stop() -> bool {
    false
}
fn default_prediction_concurrency() -> u32 {
    8 // DB接続プール(16)の半分
}
fn default_min_pool_liquidity() -> u32 {
    100 // 100 NEAR
}
fn default_pool_info_retention_count() -> u32 {
    10
}
fn default_token_rates_retention_days() -> u32 {
    365
}
fn default_harvest_min_amount() -> u32 {
    10
}
fn default_harvest_reserve_amount() -> u32 {
    1
}
fn default_harvest_interval_seconds() -> u64 {
    86400 // 24時間
}
fn default_token_not_found_wait() -> String {
    "1s".to_string()
}
fn default_other_error_wait() -> String {
    "5s".to_string()
}
fn default_preview_not_found_wait() -> String {
    "2s".to_string()
}
fn default_rust_log_format() -> String {
    "json".to_string()
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            use_mainnet: default_use_mainnet(),
        }
    }
}

impl Default for RpcSettings {
    fn default() -> Self {
        Self {
            failure_reset_seconds: default_failure_reset_seconds(),
            max_attempts: default_max_attempts(),
        }
    }
}

impl Default for TradeConfig {
    fn default() -> Self {
        Self {
            enabled: default_trade_enabled(),
            initial_investment: default_initial_investment(),
            top_tokens: default_top_tokens(),
            evaluation_days: default_evaluation_days(),
            account_reserve: default_account_reserve(),
            cron_schedule: default_cron_schedule(),
            prediction_max_retries: default_prediction_max_retries(),
            prediction_retry_delay_seconds: default_prediction_retry_delay_seconds(),
            price_history_days: default_price_history_days(),
            volatility_days: default_volatility_days(),
            unwrap_on_stop: default_unwrap_on_stop(),
            prediction_concurrency: default_prediction_concurrency(),
            min_pool_liquidity: default_min_pool_liquidity(),
        }
    }
}

impl Default for CronConfig {
    fn default() -> Self {
        Self {
            pool_info_retention_count: default_pool_info_retention_count(),
            token_rates_retention_days: default_token_rates_retention_days(),
        }
    }
}

impl Default for ArbitrageConfig {
    fn default() -> Self {
        Self {
            needed: false,
            token_not_found_wait: default_token_not_found_wait(),
            other_error_wait: default_other_error_wait(),
            preview_not_found_wait: default_preview_not_found_wait(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            rust_log_format: default_rust_log_format(),
        }
    }
}

static CONFIG: Lazy<Config> = Lazy::new(|| {
    load_config().unwrap_or_else(|e| {
        eprintln!(
            "Warning: Failed to load config files: {}. Using defaults.",
            e
        );
        Config::default()
    })
});

static CONFIG_STORE: Lazy<Arc<Mutex<HashMap<String, String>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

static DB_STORE: Lazy<Arc<Mutex<HashMap<String, String>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

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

    // Priority 3: Environment variables (for backward compatibility)
    if let Ok(val) = std::env::var(name)
        && !val.is_empty()
    {
        return Ok(val);
    }

    // Priority 4: TOML config
    let toml_value = match name {
        "USE_MAINNET" => Some(CONFIG.network.use_mainnet.to_string()),
        "ROOT_ACCOUNT_ID" => {
            if !CONFIG.wallet.root_account_id.is_empty() {
                Some(CONFIG.wallet.root_account_id.clone())
            } else {
                None
            }
        }
        "ROOT_MNEMONIC" => {
            if !CONFIG.wallet.root_mnemonic.is_empty() {
                Some(CONFIG.wallet.root_mnemonic.clone())
            } else {
                None
            }
        }
        "ROOT_HDPATH" => Some(CONFIG.wallet.root_hdpath.clone()),
        "TRADE_INITIAL_INVESTMENT" => Some(CONFIG.trade.initial_investment.to_string()),
        "TRADE_TOP_TOKENS" => Some(CONFIG.trade.top_tokens.to_string()),
        "TRADE_EVALUATION_DAYS" => Some(CONFIG.trade.evaluation_days.to_string()),
        "TRADE_ACCOUNT_RESERVE" => Some(CONFIG.trade.account_reserve.to_string()),
        "TRADE_CRON_SCHEDULE" => Some(CONFIG.trade.cron_schedule.clone()),
        "TRADE_PREDICTION_MAX_RETRIES" => Some(CONFIG.trade.prediction_max_retries.to_string()),
        "TRADE_PREDICTION_RETRY_DELAY_SECONDS" => {
            Some(CONFIG.trade.prediction_retry_delay_seconds.to_string())
        }
        "TRADE_PRICE_HISTORY_DAYS" => Some(CONFIG.trade.price_history_days.to_string()),
        "TRADE_VOLATILITY_DAYS" => Some(CONFIG.trade.volatility_days.to_string()),
        "TRADE_UNWRAP_ON_STOP" => Some(CONFIG.trade.unwrap_on_stop.to_string()),
        "TRADE_MIN_POOL_LIQUIDITY" => Some(CONFIG.trade.min_pool_liquidity.to_string()),
        "POOL_INFO_RETENTION_COUNT" => Some(CONFIG.cron.pool_info_retention_count.to_string()),
        "TOKEN_RATES_RETENTION_DAYS" => Some(CONFIG.cron.token_rates_retention_days.to_string()),
        "HARVEST_ACCOUNT_ID" => {
            if !CONFIG.harvest.account_id.is_empty() {
                Some(CONFIG.harvest.account_id.clone())
            } else {
                None
            }
        }
        "HARVEST_MIN_AMOUNT" => Some(CONFIG.harvest.min_amount.to_string()),
        "HARVEST_RESERVE_AMOUNT" => Some(CONFIG.harvest.reserve_amount.to_string()),
        "HARVEST_INTERVAL_SECONDS" => Some(CONFIG.harvest.interval_seconds.to_string()),
        "ARBITRAGE_NEEDED" => Some(CONFIG.arbitrage.needed.to_string()),
        "ARBITRAGE_TOKEN_NOT_FOUND_WAIT" => Some(CONFIG.arbitrage.token_not_found_wait.clone()),
        "ARBITRAGE_OTHER_ERROR_WAIT" => Some(CONFIG.arbitrage.other_error_wait.clone()),
        "ARBITRAGE_PREVIEW_NOT_FOUND_WAIT" => Some(CONFIG.arbitrage.preview_not_found_wait.clone()),
        "TRADE_PREDICTION_CONCURRENCY" => Some(CONFIG.trade.prediction_concurrency.to_string()),
        "RPC_ENDPOINTS" => {
            let json = serde_json::to_string(&CONFIG.rpc.endpoints).ok();
            if json.as_deref() == Some("[]") {
                None
            } else {
                json
            }
        }
        "RPC_FAILURE_RESET_SECONDS" => Some(CONFIG.rpc.settings.failure_reset_seconds.to_string()),
        "RPC_MAX_ATTEMPTS" => Some(CONFIG.rpc.settings.max_attempts.to_string()),
        "PORTFOLIO_REBALANCE_THRESHOLD" => Some("0.1".to_string()),
        "LIQUIDITY_VOLUME_WEIGHT" => Some("0.6".to_string()),
        "LIQUIDITY_POOL_WEIGHT" => Some("0.4".to_string()),
        "LIQUIDITY_ERROR_DEFAULT_SCORE" => Some("0.3".to_string()),
        "CRON_MAX_SLEEP_SECONDS" => Some("60".to_string()),
        "CRON_LOG_THRESHOLD_SECONDS" => Some("300".to_string()),
        "HARVEST_BALANCE_MULTIPLIER" => Some("128".to_string()),
        "RUST_LOG_FORMAT" => Some(CONFIG.logging.rust_log_format.clone()),
        _ => None,
    };

    if let Some(value) = toml_value
        && !value.is_empty()
    {
        return Ok(value);
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

fn get_from_store(name: &str) -> Option<String> {
    if let Ok(store) = CONFIG_STORE.lock() {
        store.get(name).cloned()
    } else {
        None
    }
}

fn get_from_db_store(name: &str) -> Option<String> {
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

/// Load configuration from TOML files with priority:
/// 1. config/config.local.toml (git-ignored, for local overrides)
/// 2. config/config.toml (git-managed template)
/// 3. Default values
fn load_config() -> Result<Config> {
    let mut config = Config::default();

    // Load base config from config.toml
    let base_path = "config/config.toml";
    if Path::new(base_path).exists() {
        let content = fs::read_to_string(base_path)?;
        config = toml::from_str(&content)?;
    }

    // Override with local config if exists
    let local_path = "config/config.local.toml";
    if Path::new(local_path).exists() {
        let content = fs::read_to_string(local_path)?;
        let local_config: Config = toml::from_str(&content)?;
        merge_config(&mut config, local_config);
    }

    Ok(config)
}

/// Merge local config into base config (local values override base values)
fn merge_config(base: &mut Config, local: Config) {
    // Network
    if local.network.use_mainnet != default_use_mainnet() {
        base.network.use_mainnet = local.network.use_mainnet;
    }

    // Wallet
    if !local.wallet.root_account_id.is_empty() {
        base.wallet.root_account_id = local.wallet.root_account_id;
    }
    if !local.wallet.root_mnemonic.is_empty() {
        base.wallet.root_mnemonic = local.wallet.root_mnemonic;
    }
    if local.wallet.root_hdpath != default_hdpath() {
        base.wallet.root_hdpath = local.wallet.root_hdpath;
    }

    // RPC
    if !local.rpc.endpoints.is_empty() {
        base.rpc.endpoints = local.rpc.endpoints;
    }
    if local.rpc.settings.failure_reset_seconds != default_failure_reset_seconds() {
        base.rpc.settings.failure_reset_seconds = local.rpc.settings.failure_reset_seconds;
    }
    if local.rpc.settings.max_attempts != default_max_attempts() {
        base.rpc.settings.max_attempts = local.rpc.settings.max_attempts;
    }

    // Trade
    if local.trade.initial_investment != default_initial_investment() {
        base.trade.initial_investment = local.trade.initial_investment;
    }
    if local.trade.top_tokens != default_top_tokens() {
        base.trade.top_tokens = local.trade.top_tokens;
    }
    if local.trade.evaluation_days != default_evaluation_days() {
        base.trade.evaluation_days = local.trade.evaluation_days;
    }
    if local.trade.account_reserve != default_account_reserve() {
        base.trade.account_reserve = local.trade.account_reserve;
    }
    if local.trade.cron_schedule != default_cron_schedule() {
        base.trade.cron_schedule = local.trade.cron_schedule;
    }
    if local.trade.prediction_max_retries != default_prediction_max_retries() {
        base.trade.prediction_max_retries = local.trade.prediction_max_retries;
    }
    if local.trade.prediction_retry_delay_seconds != default_prediction_retry_delay_seconds() {
        base.trade.prediction_retry_delay_seconds = local.trade.prediction_retry_delay_seconds;
    }
    if local.trade.price_history_days != default_price_history_days() {
        base.trade.price_history_days = local.trade.price_history_days;
    }
    if local.trade.volatility_days != default_volatility_days() {
        base.trade.volatility_days = local.trade.volatility_days;
    }
    if local.trade.unwrap_on_stop != default_unwrap_on_stop() {
        base.trade.unwrap_on_stop = local.trade.unwrap_on_stop;
    }
    if local.trade.prediction_concurrency != default_prediction_concurrency() {
        base.trade.prediction_concurrency = local.trade.prediction_concurrency;
    }
    if local.trade.min_pool_liquidity != default_min_pool_liquidity() {
        base.trade.min_pool_liquidity = local.trade.min_pool_liquidity;
    }

    // Cron
    if local.cron.pool_info_retention_count != default_pool_info_retention_count() {
        base.cron.pool_info_retention_count = local.cron.pool_info_retention_count;
    }
    if local.cron.token_rates_retention_days != default_token_rates_retention_days() {
        base.cron.token_rates_retention_days = local.cron.token_rates_retention_days;
    }

    // Harvest
    if !local.harvest.account_id.is_empty() {
        base.harvest.account_id = local.harvest.account_id;
    }
    if local.harvest.min_amount != default_harvest_min_amount() {
        base.harvest.min_amount = local.harvest.min_amount;
    }
    if local.harvest.reserve_amount != default_harvest_reserve_amount() {
        base.harvest.reserve_amount = local.harvest.reserve_amount;
    }
    if local.harvest.interval_seconds != default_harvest_interval_seconds() {
        base.harvest.interval_seconds = local.harvest.interval_seconds;
    }

    // Arbitrage
    if local.arbitrage.needed {
        base.arbitrage.needed = local.arbitrage.needed;
    }
    if local.arbitrage.token_not_found_wait != default_token_not_found_wait() {
        base.arbitrage.token_not_found_wait = local.arbitrage.token_not_found_wait;
    }
    if local.arbitrage.other_error_wait != default_other_error_wait() {
        base.arbitrage.other_error_wait = local.arbitrage.other_error_wait;
    }
    if local.arbitrage.preview_not_found_wait != default_preview_not_found_wait() {
        base.arbitrage.preview_not_found_wait = local.arbitrage.preview_not_found_wait;
    }

    // Logging
    if local.logging.rust_log_format != default_rust_log_format() {
        base.logging.rust_log_format = local.logging.rust_log_format;
    }
}

/// Get TOML-based configuration
pub fn config() -> &'static Config {
    &CONFIG
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_config_store_priority() {
        // CONFIG_STOREの値が最優先
        const TEST_KEY: &str = "RUST_LOG_FORMAT";
        unsafe {
            std::env::set_var(TEST_KEY, "env-value");
        }
        set(TEST_KEY, "store-value");
        let result = get(TEST_KEY).unwrap();
        assert_eq!(result, "store-value");

        // Cleanup
        if let Ok(mut store) = CONFIG_STORE.lock() {
            store.remove(TEST_KEY);
        }
        unsafe {
            std::env::remove_var(TEST_KEY);
        }
    }

    #[test]
    #[serial]
    fn test_boolean_config() {
        unsafe {
            std::env::remove_var("USE_MAINNET");
        }
        let result = get("USE_MAINNET").unwrap();
        assert_eq!(result, "true");
    }

    #[test]
    #[serial]
    fn test_numeric_config() {
        unsafe {
            std::env::remove_var("TRADE_TOP_TOKENS");
        }
        let result = get("TRADE_TOP_TOKENS").unwrap();
        assert_eq!(result, "10");
    }

    #[test]
    #[serial]
    fn test_priority_order() {
        // 優先順位の完全検証: CONFIG_STORE > DB > 環境変数 > TOML > デフォルト
        const TEST_KEY: &str = "TRADE_TOP_TOKENS";

        // Step 1: TOML/デフォルトのみ (最低優先度)
        unsafe {
            std::env::remove_var(TEST_KEY);
        }
        if let Ok(mut store) = CONFIG_STORE.lock() {
            store.remove(TEST_KEY);
        }
        if let Ok(mut store) = DB_STORE.lock() {
            store.remove(TEST_KEY);
        }
        let result = get(TEST_KEY).unwrap();
        assert_eq!(result, "10"); // config.toml または default

        // Step 2: 環境変数追加 (TOML より優先)
        unsafe {
            std::env::set_var(TEST_KEY, "99");
        }
        let result = get(TEST_KEY).unwrap();
        assert_eq!(result, "99");

        // Step 3: DB_STORE 追加 (環境変数より優先)
        if let Ok(mut store) = DB_STORE.lock() {
            store.insert(TEST_KEY.to_string(), "77".to_string());
        }
        let result = get(TEST_KEY).unwrap();
        assert_eq!(result, "77");

        // Step 4: CONFIG_STORE 追加 (DB_STORE より優先)
        set(TEST_KEY, "42");
        let result = get(TEST_KEY).unwrap();
        assert_eq!(result, "42");

        // Cleanup
        if let Ok(mut store) = CONFIG_STORE.lock() {
            store.remove(TEST_KEY);
        }
        if let Ok(mut store) = DB_STORE.lock() {
            store.remove(TEST_KEY);
        }
        unsafe {
            std::env::remove_var(TEST_KEY);
        }
    }

    #[test]
    #[serial]
    fn test_trade_min_pool_liquidity_default() {
        unsafe {
            std::env::remove_var("TRADE_MIN_POOL_LIQUIDITY");
        }
        if let Ok(mut store) = CONFIG_STORE.lock() {
            store.remove("TRADE_MIN_POOL_LIQUIDITY");
        }
        let result = get("TRADE_MIN_POOL_LIQUIDITY").unwrap();
        assert_eq!(result, "100");
    }

    #[test]
    #[serial]
    fn test_trade_min_pool_liquidity_from_env() {
        unsafe {
            std::env::set_var("TRADE_MIN_POOL_LIQUIDITY", "200");
        }
        let result = get("TRADE_MIN_POOL_LIQUIDITY").unwrap();
        assert_eq!(result, "200");
        unsafe {
            std::env::remove_var("TRADE_MIN_POOL_LIQUIDITY");
        }
    }

    /// Step 1: 新規キーのデフォルト値テスト
    #[test]
    #[serial]
    fn test_new_config_keys_defaults() {
        // 環境変数と CONFIG_STORE をクリア
        let keys_and_defaults = [
            ("TRADE_PREDICTION_CONCURRENCY", "8"),
            ("RPC_FAILURE_RESET_SECONDS", "300"),
            ("RPC_MAX_ATTEMPTS", "10"),
            ("PORTFOLIO_REBALANCE_THRESHOLD", "0.1"),
            ("LIQUIDITY_VOLUME_WEIGHT", "0.6"),
            ("LIQUIDITY_POOL_WEIGHT", "0.4"),
            ("LIQUIDITY_ERROR_DEFAULT_SCORE", "0.3"),
            ("CRON_MAX_SLEEP_SECONDS", "60"),
            ("CRON_LOG_THRESHOLD_SECONDS", "300"),
            ("HARVEST_BALANCE_MULTIPLIER", "128"),
        ];

        for (key, expected) in &keys_and_defaults {
            unsafe {
                std::env::remove_var(key);
            }
            if let Ok(mut store) = CONFIG_STORE.lock() {
                store.remove(*key);
            }
            let result = get(key).unwrap();
            assert_eq!(result, *expected, "key={key}");
        }
    }

    #[test]
    #[serial]
    fn test_rpc_endpoints_json() {
        unsafe {
            std::env::remove_var("RPC_ENDPOINTS");
        }
        if let Ok(mut store) = CONFIG_STORE.lock() {
            store.remove("RPC_ENDPOINTS");
        }
        // RPC_ENDPOINTS はデフォルトで空配列なので None→Err になるか、
        // TOML に設定があれば JSON 文字列が返る
        let result = get("RPC_ENDPOINTS");
        if let Ok(json_str) = result {
            // 有効な JSON であること
            let parsed: std::result::Result<Vec<RpcEndpoint>, _> = serde_json::from_str(&json_str);
            assert!(parsed.is_ok(), "RPC_ENDPOINTS should be valid JSON array");
        }
        // 空配列の場合は Err が返って OK
    }

    #[test]
    #[serial]
    fn test_new_config_keys_config_store_override() {
        // CONFIG_STORE で上書きした場合に新規キーも優先されることを確認
        set("PORTFOLIO_REBALANCE_THRESHOLD", "0.05");
        let result = get("PORTFOLIO_REBALANCE_THRESHOLD").unwrap();
        assert_eq!(result, "0.05");

        set("HARVEST_BALANCE_MULTIPLIER", "256");
        let result = get("HARVEST_BALANCE_MULTIPLIER").unwrap();
        assert_eq!(result, "256");

        // Cleanup
        if let Ok(mut store) = CONFIG_STORE.lock() {
            store.remove("PORTFOLIO_REBALANCE_THRESHOLD");
            store.remove("HARVEST_BALANCE_MULTIPLIER");
        }
    }

    #[test]
    #[serial]
    fn test_db_store_overrides_env() {
        // DB_STORE が環境変数より優先されること
        const TEST_KEY: &str = "TRADE_TOP_TOKENS";
        unsafe {
            std::env::set_var(TEST_KEY, "env_val");
        }
        if let Ok(mut store) = CONFIG_STORE.lock() {
            store.remove(TEST_KEY);
        }

        load_db_config(HashMap::from([(
            TEST_KEY.to_string(),
            "db_val".to_string(),
        )]));
        let result = get(TEST_KEY).unwrap();
        assert_eq!(result, "db_val");

        // Cleanup
        if let Ok(mut store) = DB_STORE.lock() {
            store.clear();
        }
        unsafe {
            std::env::remove_var(TEST_KEY);
        }
    }

    #[test]
    #[serial]
    fn test_config_store_overrides_db_store() {
        // CONFIG_STORE が DB_STORE より優先されること
        const TEST_KEY: &str = "TRADE_TOP_TOKENS";
        load_db_config(HashMap::from([(
            TEST_KEY.to_string(),
            "db_val".to_string(),
        )]));
        set(TEST_KEY, "store_val");

        let result = get(TEST_KEY).unwrap();
        assert_eq!(result, "store_val");

        // Cleanup
        if let Ok(mut store) = CONFIG_STORE.lock() {
            store.remove(TEST_KEY);
        }
        if let Ok(mut store) = DB_STORE.lock() {
            store.clear();
        }
    }

    #[test]
    #[serial]
    fn test_load_db_config_replaces_previous() {
        // load_db_config を再度呼ぶと前の値が置き換えられること
        load_db_config(HashMap::from([
            ("KEY_A".to_string(), "val_a".to_string()),
            ("KEY_B".to_string(), "val_b".to_string()),
        ]));
        assert_eq!(get("KEY_A").unwrap(), "val_a");
        assert_eq!(get("KEY_B").unwrap(), "val_b");

        // 再ロード: KEY_A は更新、KEY_B は消える
        load_db_config(HashMap::from([(
            "KEY_A".to_string(),
            "new_val_a".to_string(),
        )]));
        assert_eq!(get("KEY_A").unwrap(), "new_val_a");
        assert!(get("KEY_B").is_err());

        // Cleanup
        if let Ok(mut store) = DB_STORE.lock() {
            store.clear();
        }
    }

    #[test]
    #[serial]
    fn test_db_store_empty_falls_through() {
        // DB_STORE が空の場合は環境変数にフォールスルー
        const TEST_KEY: &str = "TRADE_TOP_TOKENS";
        if let Ok(mut store) = CONFIG_STORE.lock() {
            store.remove(TEST_KEY);
        }
        if let Ok(mut store) = DB_STORE.lock() {
            store.clear();
        }
        unsafe {
            std::env::set_var(TEST_KEY, "env_val");
        }

        let result = get(TEST_KEY).unwrap();
        assert_eq!(result, "env_val");

        // Cleanup
        unsafe {
            std::env::remove_var(TEST_KEY);
        }
    }
}
