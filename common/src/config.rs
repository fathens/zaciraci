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

#[derive(Debug, Deserialize, Clone)]
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

#[derive(Debug, Deserialize)]
pub struct ExternalServicesConfig {
    #[serde(default = "default_chronos_url")]
    pub chronos_url: String,
    #[serde(default = "default_ollama_base_url")]
    pub ollama_base_url: String,
    #[serde(default = "default_ollama_model")]
    pub ollama_model: String,
}

#[derive(Debug, Deserialize)]
pub struct TradeConfig {
    #[serde(default = "default_initial_investment")]
    pub initial_investment: u32,
    #[serde(default = "default_top_tokens")]
    pub top_tokens: u32,
    #[serde(default = "default_evaluation_days")]
    pub evaluation_days: u32,
    #[serde(default = "default_cron_schedule")]
    pub cron_schedule: String,
}

#[derive(Debug, Deserialize)]
pub struct CronConfig {
    #[serde(default = "default_record_rates_initial_value")]
    pub record_rates_initial_value: u32,
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
}

impl Default for HarvestConfig {
    fn default() -> Self {
        Self {
            account_id: String::new(),
            min_amount: default_harvest_min_amount(),
            reserve_amount: default_harvest_reserve_amount(),
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
fn default_chronos_url() -> String {
    "http://localhost:8000".to_string()
}
fn default_ollama_base_url() -> String {
    "http://localhost:11434".to_string()
}
fn default_ollama_model() -> String {
    "llama2".to_string()
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
fn default_cron_schedule() -> String {
    "0 0 0 * * *".to_string()
}
fn default_record_rates_initial_value() -> u32 {
    100
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

impl Default for ExternalServicesConfig {
    fn default() -> Self {
        Self {
            chronos_url: default_chronos_url(),
            ollama_base_url: default_ollama_base_url(),
            ollama_model: default_ollama_model(),
        }
    }
}

impl Default for TradeConfig {
    fn default() -> Self {
        Self {
            initial_investment: default_initial_investment(),
            top_tokens: default_top_tokens(),
            evaluation_days: default_evaluation_days(),
            cron_schedule: default_cron_schedule(),
        }
    }
}

impl Default for CronConfig {
    fn default() -> Self {
        Self {
            record_rates_initial_value: default_record_rates_initial_value(),
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

pub fn get(name: &str) -> Result<String> {
    // Priority 1: CONFIG_STORE (runtime overrides)
    if let Some(value) = get_from_store(name) {
        if value.is_empty() {
            return Err(anyhow!("{} is empty", name));
        }
        return Ok(value);
    }

    // Priority 2: Environment variables (for backward compatibility)
    if let Ok(val) = std::env::var(name)
        && !val.is_empty()
    {
        return Ok(val);
    }

    // Priority 3: TOML config
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
        "CHRONOS_URL" => Some(CONFIG.external_services.chronos_url.clone()),
        "OLLAMA_BASE_URL" => Some(CONFIG.external_services.ollama_base_url.clone()),
        "OLLAMA_MODEL" => Some(CONFIG.external_services.ollama_model.clone()),
        "TRADE_INITIAL_INVESTMENT" => Some(CONFIG.trade.initial_investment.to_string()),
        "TRADE_TOP_TOKENS" => Some(CONFIG.trade.top_tokens.to_string()),
        "TRADE_EVALUATION_DAYS" => Some(CONFIG.trade.evaluation_days.to_string()),
        "TRADE_CRON_SCHEDULE" => Some(CONFIG.trade.cron_schedule.clone()),
        "RECORD_RATES_INITIAL_VALUE" => Some(CONFIG.cron.record_rates_initial_value.to_string()),
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
        "ARBITRAGE_NEEDED" => Some(CONFIG.arbitrage.needed.to_string()),
        "ARBITRAGE_TOKEN_NOT_FOUND_WAIT" => Some(CONFIG.arbitrage.token_not_found_wait.clone()),
        "ARBITRAGE_OTHER_ERROR_WAIT" => Some(CONFIG.arbitrage.other_error_wait.clone()),
        "ARBITRAGE_PREVIEW_NOT_FOUND_WAIT" => Some(CONFIG.arbitrage.preview_not_found_wait.clone()),
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

#[allow(dead_code)] // This function is not used in the code, but it is needed for tests
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

    // External services
    if local.external_services.chronos_url != default_chronos_url() {
        base.external_services.chronos_url = local.external_services.chronos_url;
    }
    if local.external_services.ollama_base_url != default_ollama_base_url() {
        base.external_services.ollama_base_url = local.external_services.ollama_base_url;
    }
    if local.external_services.ollama_model != default_ollama_model() {
        base.external_services.ollama_model = local.external_services.ollama_model;
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
    if local.trade.cron_schedule != default_cron_schedule() {
        base.trade.cron_schedule = local.trade.cron_schedule;
    }

    // Cron
    if local.cron.record_rates_initial_value != default_record_rates_initial_value() {
        base.cron.record_rates_initial_value = local.cron.record_rates_initial_value;
    }
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

    #[test]
    fn test_toml_default_values() {
        // 環境変数が設定されていない場合はTOMLのデフォルト値が使われる
        unsafe {
            std::env::remove_var("OLLAMA_BASE_URL");
        }
        let result = get("OLLAMA_BASE_URL").unwrap();
        assert_eq!(result, "http://localhost:11434");
    }

    #[test]
    fn test_backward_compatibility_with_env_vars() {
        // 環境変数が設定されている場合は環境変数の値が使われる
        unsafe {
            std::env::set_var("OLLAMA_MODEL", "test-model");
        }
        let result = get("OLLAMA_MODEL").unwrap();
        assert_eq!(result, "test-model");
        unsafe {
            std::env::remove_var("OLLAMA_MODEL");
        }
    }

    #[test]
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
    fn test_boolean_config() {
        unsafe {
            std::env::remove_var("USE_MAINNET");
        }
        let result = get("USE_MAINNET").unwrap();
        assert_eq!(result, "true");
    }

    #[test]
    fn test_numeric_config() {
        unsafe {
            std::env::remove_var("TRADE_TOP_TOKENS");
        }
        let result = get("TRADE_TOP_TOKENS").unwrap();
        assert_eq!(result, "10");
    }

    #[test]
    fn test_priority_order() {
        // 優先順位の完全検証: CONFIG_STORE > 環境変数 > TOML > デフォルト
        const TEST_KEY: &str = "OLLAMA_BASE_URL";

        // Step 1: TOML/デフォルトのみ (最低優先度)
        unsafe {
            std::env::remove_var(TEST_KEY);
        }
        let result = get(TEST_KEY).unwrap();
        assert_eq!(result, "http://localhost:11434"); // config.toml または default

        // Step 2: 環境変数追加 (TOML より優先)
        unsafe {
            std::env::set_var(TEST_KEY, "http://env-url:1111");
        }
        let result = get(TEST_KEY).unwrap();
        assert_eq!(result, "http://env-url:1111");

        // Step 3: CONFIG_STORE 追加 (環境変数より優先)
        set(TEST_KEY, "http://store-url:2222");
        let result = get(TEST_KEY).unwrap();
        assert_eq!(result, "http://store-url:2222");

        // Cleanup
        if let Ok(mut store) = CONFIG_STORE.lock() {
            store.remove(TEST_KEY);
        }
        unsafe {
            std::env::remove_var(TEST_KEY);
        }
    }
}
