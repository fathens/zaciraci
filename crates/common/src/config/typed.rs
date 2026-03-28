use std::sync::LazyLock;
use std::time::Duration;

// ── ConfigValueType enum ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigValueType {
    Bool,
    U16,
    U32,
    U64,
    U128,
    I64,
    F64,
    String,
    Duration,
}

impl ConfigValueType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::U128 => "u128",
            Self::I64 => "i64",
            Self::F64 => "f64",
            Self::String => "string",
            Self::Duration => "duration",
        }
    }
}

impl std::fmt::Display for ConfigValueType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── KeyDefinition / ResolvedKeyInfo ──

pub struct KeyDefinition {
    pub key: &'static str,
    pub description: &'static str,
    pub value_type: ConfigValueType,
    pub default_value: &'static str,
}

pub struct ResolvedKeyInfo {
    pub key: std::string::String,
    pub description: std::string::String,
    pub value_type: ConfigValueType,
    pub resolved_value: std::string::String,
}

// ── ConfigResolve trait: type-specific config resolution ──

pub(crate) trait ConfigResolve: Sized {
    type Default;
    const VALUE_TYPE: ConfigValueType;
    fn resolve(key: &str, default: Self::Default) -> Self;
    fn resolve_without_db(key: &str, default: Self::Default) -> Self;
    fn display_string(value: Self) -> std::string::String;
}

impl ConfigResolve for bool {
    type Default = bool;
    const VALUE_TYPE: ConfigValueType = ConfigValueType::Bool;
    fn resolve(key: &str, default: bool) -> Self {
        crate::config::store::get(key)
            .ok()
            .and_then(|v| v.to_lowercase().parse::<bool>().ok())
            .unwrap_or(default)
    }
    fn resolve_without_db(key: &str, default: bool) -> Self {
        crate::config::store::get_excluding_db(key)
            .ok()
            .and_then(|v| v.to_lowercase().parse::<bool>().ok())
            .unwrap_or(default)
    }
    fn display_string(value: Self) -> std::string::String {
        value.to_string()
    }
}

impl ConfigResolve for String {
    type Default = &'static str;
    const VALUE_TYPE: ConfigValueType = ConfigValueType::String;
    fn resolve(key: &str, default: &'static str) -> Self {
        crate::config::store::get(key).unwrap_or_else(|_| default.to_string())
    }
    fn resolve_without_db(key: &str, default: &'static str) -> Self {
        crate::config::store::get_excluding_db(key).unwrap_or_else(|_| default.to_string())
    }
    fn display_string(value: Self) -> std::string::String {
        value
    }
}

macro_rules! impl_config_resolve_numeric {
    ($ty:ty, $variant:ident) => {
        impl ConfigResolve for $ty {
            type Default = $ty;
            const VALUE_TYPE: ConfigValueType = ConfigValueType::$variant;
            fn resolve(key: &str, default: $ty) -> Self {
                crate::config::store::get(key)
                    .ok()
                    .and_then(|v| v.parse::<$ty>().ok())
                    .unwrap_or(default)
            }
            fn resolve_without_db(key: &str, default: $ty) -> Self {
                crate::config::store::get_excluding_db(key)
                    .ok()
                    .and_then(|v| v.parse::<$ty>().ok())
                    .unwrap_or(default)
            }
            fn display_string(value: Self) -> std::string::String {
                value.to_string()
            }
        }
    };
}

impl_config_resolve_numeric!(u16, U16);
impl_config_resolve_numeric!(u32, U32);
impl_config_resolve_numeric!(u64, U64);
impl_config_resolve_numeric!(u128, U128);
impl_config_resolve_numeric!(usize, U64);
impl_config_resolve_numeric!(i64, I64);
impl_config_resolve_numeric!(f64, F64);

impl ConfigResolve for Duration {
    type Default = Duration;
    const VALUE_TYPE: ConfigValueType = ConfigValueType::Duration;
    fn resolve(key: &str, default: Duration) -> Self {
        crate::config::store::get(key)
            .ok()
            .and_then(|v| humantime::parse_duration(&v).ok())
            .unwrap_or(default)
    }
    fn resolve_without_db(key: &str, default: Duration) -> Self {
        crate::config::store::get_excluding_db(key)
            .ok()
            .and_then(|v| humantime::parse_duration(&v).ok())
            .unwrap_or(default)
    }
    fn display_string(value: Self) -> std::string::String {
        humantime::format_duration(value).to_string()
    }
}

impl ConfigResolve for anyhow::Result<String> {
    type Default = ();
    const VALUE_TYPE: ConfigValueType = ConfigValueType::String;
    fn resolve(key: &str, _default: ()) -> Self {
        crate::config::store::get(key)
            .map_err(|_| anyhow::anyhow!("required config key not found: {}", key))
    }
    fn resolve_without_db(key: &str, _default: ()) -> Self {
        crate::config::store::get_excluding_db(key)
            .map_err(|_| anyhow::anyhow!("required config key not found: {}", key))
    }
    fn display_string(value: Self) -> std::string::String {
        value.unwrap_or_else(|_| "(未設定)".to_string())
    }
}

// ── MockStore trait: maps types to Clone-able mock storage ──

pub trait MockStore: Sized {
    /// The type stored in MockConfig fields (must be Clone).
    /// For most types this is Self. For Result<String> it is String.
    type Storage: Clone;

    /// Convert stored value to the actual return type.
    fn from_storage(s: &Self::Storage) -> Self;
}

impl MockStore for bool {
    type Storage = bool;
    fn from_storage(s: &bool) -> Self {
        *s
    }
}

impl MockStore for String {
    type Storage = String;
    fn from_storage(s: &String) -> Self {
        s.clone()
    }
}

macro_rules! impl_mock_store_copy {
    ($($ty:ty),*) => {
        $(impl MockStore for $ty {
            type Storage = $ty;
            fn from_storage(s: &$ty) -> Self { *s }
        })*
    }
}

impl_mock_store_copy!(u16, u32, u64, u128, usize, i64, f64);

impl MockStore for Duration {
    type Storage = Duration;
    fn from_storage(s: &Duration) -> Self {
        *s
    }
}

/// For Result<String>, MockConfig stores just a String.
/// Setting `mock.database_url = Some("postgres://...")` will return `Ok(...)`.
impl MockStore for anyhow::Result<String> {
    type Storage = String;
    fn from_storage(s: &String) -> Self {
        Ok(s.clone())
    }
}

// ── Main macro ──

/// Declarative macro that generates:
/// - `ConfigAccess` trait with typed accessor methods
/// - `ConfigResolver` struct that resolves values via `config::get()` priority chain
/// - `MockConfig` struct for test isolation (wraps real resolver, overrides per-field)
/// - `KEY_DEFINITIONS` const with static metadata for all config keys
/// - `resolve_all_without_db()` function for runtime key resolution excluding DB
macro_rules! define_typed_config {
    (
        $(
            $(#[doc = $doc:expr])*
            fn $method:ident() -> $ty:ty {
                key: $key:expr,
                default: $default:expr
            }
        )*
    ) => {
        pub trait ConfigAccess: Send + Sync {
            $(
                $(#[doc = $doc])*
                fn $method(&self) -> $ty;
            )*
        }

        #[derive(Clone, Copy)]
        pub struct ConfigResolver;

        impl ConfigAccess for ConfigResolver {
            $(
                fn $method(&self) -> $ty {
                    <$ty as ConfigResolve>::resolve($key, $default)
                }
            )*
        }

        #[doc(hidden)]
        pub struct MockConfig {
            base: ConfigResolver,
            $( pub $method: Option<<$ty as MockStore>::Storage>, )*
        }

        impl Default for MockConfig {
            fn default() -> Self {
                Self::new()
            }
        }

        impl MockConfig {
            pub fn new() -> Self {
                Self {
                    base: ConfigResolver,
                    $( $method: None, )*
                }
            }
        }

        impl ConfigAccess for MockConfig {
            $(
                fn $method(&self) -> $ty {
                    match &self.$method {
                        Some(v) => <$ty as MockStore>::from_storage(v),
                        None => self.base.$method(),
                    }
                }
            )*
        }

        pub const KEY_DEFINITIONS: &[KeyDefinition] = &[
            $(
                KeyDefinition {
                    key: $key,
                    description: concat!($($doc, "\n",)*),
                    value_type: <$ty as ConfigResolve>::VALUE_TYPE,
                    default_value: stringify!($default),
                },
            )*
        ];

        pub fn resolve_all_without_db() -> Vec<ResolvedKeyInfo> {
            vec![
                $(
                    {
                        let value = <$ty as ConfigResolve>::resolve_without_db($key, $default);
                        ResolvedKeyInfo {
                            key: $key.to_string(),
                            description: concat!($($doc, "\n",)*).trim().to_string(),
                            value_type: <$ty as ConfigResolve>::VALUE_TYPE,
                            resolved_value: <$ty as ConfigResolve>::display_string(value),
                        }
                    },
                )*
            ]
        }
    };
}

define_typed_config! {
    // ── trade ──

    /// Whether trading is enabled
    fn trade_enabled() -> bool {
        key: "TRADE_ENABLED",
        default: false
    }

    /// Initial investment amount in NEAR
    fn trade_initial_investment() -> u32 {
        key: "TRADE_INITIAL_INVESTMENT",
        default: 100
    }

    /// Number of top tokens to track
    fn trade_top_tokens() -> u32 {
        key: "TRADE_TOP_TOKENS",
        default: 10
    }

    /// Evaluation period in days
    fn trade_evaluation_days() -> u32 {
        key: "TRADE_EVALUATION_DAYS",
        default: 10
    }

    /// Account reserve in NEAR
    fn trade_account_reserve() -> u32 {
        key: "TRADE_ACCOUNT_RESERVE",
        default: 10
    }

    /// Cron schedule for trade execution
    fn trade_cron_schedule() -> String {
        key: "TRADE_CRON_SCHEDULE",
        default: "0 0 0 * * *"
    }

    /// Cron schedule for rate recording
    fn record_rates_cron_schedule() -> String {
        key: "RECORD_RATES_CRON_SCHEDULE",
        default: "0 */15 * * * *"
    }

    /// Max retries for prediction fetch
    fn trade_prediction_max_retries() -> u32 {
        key: "TRADE_PREDICTION_MAX_RETRIES",
        default: 2
    }

    /// Delay between prediction retries in seconds
    fn trade_prediction_retry_delay_seconds() -> u64 {
        key: "TRADE_PREDICTION_RETRY_DELAY_SECONDS",
        default: 5
    }

    /// Days of price history for predictions and volatility calculation
    fn trade_price_history_days() -> u32 {
        key: "TRADE_PRICE_HISTORY_DAYS",
        default: 30
    }

    /// Whether to unwrap wrap.near on stop
    fn trade_unwrap_on_stop() -> bool {
        key: "TRADE_UNWRAP_ON_STOP",
        default: false
    }

    /// Parallel prediction tasks
    fn trade_prediction_concurrency() -> u32 {
        key: "TRADE_PREDICTION_CONCURRENCY",
        default: 4
    }

    /// Number of tokens to process per prediction chunk.
    /// Controls peak memory: each chunk loads chunk_size * ~2335 rows of price history.
    /// Recommended range: 5–50. Smaller values reduce peak memory but increase DB round-trips.
    fn trade_prediction_chunk_size() -> u32 {
        key: "TRADE_PREDICTION_CHUNK_SIZE",
        default: 20
    }

    /// Number of threads for model training pool.
    /// Controls peak memory: each thread can hold one augurs model buffer (~200 MB).
    /// Independent of TRADE_PREDICTION_CONCURRENCY.
    /// Recommended range: 1–8. Higher values increase peak memory proportionally.
    fn trade_prediction_model_threads() -> u32 {
        key: "TRADE_PREDICTION_MODEL_THREADS",
        default: 3
    }

    /// Minimum pool liquidity in NEAR
    fn trade_min_pool_liquidity() -> u32 {
        key: "TRADE_MIN_POOL_LIQUIDITY",
        default: 100
    }

    /// Parallel token cache update tasks
    fn trade_token_cache_concurrency() -> u32 {
        key: "TRADE_TOKEN_CACHE_CONCURRENCY",
        default: 8
    }

    /// Base backoff interval in minutes for failed decimals RPC fetches
    fn trade_token_cache_backoff_base_minutes() -> u64 {
        key: "TRADE_TOKEN_CACHE_BACKOFF_BASE_MINUTES",
        default: 15
    }

    /// Maximum backoff interval in minutes for failed decimals RPC fetches
    fn trade_token_cache_max_backoff_minutes() -> u64 {
        key: "TRADE_TOKEN_CACHE_MAX_BACKOFF_MINUTES",
        default: 1440
    }

    /// Minimum per-token prediction confidence to include in portfolio.
    /// Tokens below this threshold are excluded from trading.
    fn trade_min_token_confidence() -> f64 {
        key: "TRADE_MIN_TOKEN_CONFIDENCE",
        default: 0.3
    }

    // ── arbitrage ──

    /// Whether arbitrage engine is enabled
    fn arbitrage_needed() -> bool {
        key: "ARBITRAGE_NEEDED",
        default: false
    }

    /// Wait duration when token not found
    fn arbitrage_token_not_found_wait() -> Duration {
        key: "ARBITRAGE_TOKEN_NOT_FOUND_WAIT",
        default: Duration::from_secs(1)
    }

    /// Wait duration on other errors
    fn arbitrage_other_error_wait() -> Duration {
        key: "ARBITRAGE_OTHER_ERROR_WAIT",
        default: Duration::from_secs(5)
    }

    /// Wait duration when preview not found
    fn arbitrage_preview_not_found_wait() -> Duration {
        key: "ARBITRAGE_PREVIEW_NOT_FOUND_WAIT",
        default: Duration::from_secs(2)
    }

    // ── harvest ──

    /// Harvest destination account ID (required)
    fn harvest_account_id() -> anyhow::Result<String> {
        key: "HARVEST_ACCOUNT_ID",
        default: ()
    }

    /// Minimum NEAR to trigger harvest
    fn harvest_min_amount() -> u32 {
        key: "HARVEST_MIN_AMOUNT",
        default: 10
    }

    /// NEAR to keep in account when harvesting
    fn harvest_reserve_amount() -> u32 {
        key: "HARVEST_RESERVE_AMOUNT",
        default: 1
    }

    /// Interval between harvests in seconds
    fn harvest_interval_seconds() -> u64 {
        key: "HARVEST_INTERVAL_SECONDS",
        default: 86400
    }

    /// Multiplier for harvest balance calculation
    fn harvest_balance_multiplier() -> u128 {
        key: "HARVEST_BALANCE_MULTIPLIER",
        default: 128
    }

    // ── rpc ──

    /// Max RPC retry attempts
    fn rpc_max_attempts() -> u16 {
        key: "RPC_MAX_ATTEMPTS",
        default: 128
    }

    // ── cron ──

    /// Retention period for pool info records in days
    fn pool_info_retention_days() -> u32 {
        key: "POOL_INFO_RETENTION_DAYS",
        default: 30
    }

    /// Retention period for token rate records in days
    fn token_rates_retention_days() -> u32 {
        key: "TOKEN_RATES_RETENTION_DAYS",
        default: 90
    }

    /// Retention period for evaluation period records in days
    /// (ON DELETE CASCADE also removes related trade_transactions and portfolio_holdings)
    fn evaluation_periods_retention_days() -> u32 {
        key: "EVALUATION_PERIODS_RETENTION_DAYS",
        default: 365
    }

    /// Retention period for config store history records in days
    fn config_store_history_retention_days() -> u32 {
        key: "CONFIG_STORE_HISTORY_RETENTION_DAYS",
        default: 365
    }

    /// Max sleep duration in cron loop in seconds
    fn cron_max_sleep_seconds() -> u64 {
        key: "CRON_MAX_SLEEP_SECONDS",
        default: 60
    }

    /// Log threshold for long waits in seconds
    fn cron_log_threshold_seconds() -> u64 {
        key: "CRON_LOG_THRESHOLD_SECONDS",
        default: 300
    }

    /// Cron schedule for database maintenance (REINDEX)
    fn db_maintenance_cron_schedule() -> String {
        key: "DB_MAINTENANCE_CRON_SCHEDULE",
        default: "0 0 4 * * 7"
    }

    // ── wallet / logging: moved to StartupConfig ──

    // ── portfolio/liquidity ──

    /// Portfolio rebalance trigger threshold
    fn portfolio_rebalance_threshold() -> f64 {
        key: "PORTFOLIO_REBALANCE_THRESHOLD",
        default: 0.1
    }

    /// Weight for volume-based liquidity score
    fn liquidity_volume_weight() -> f64 {
        key: "LIQUIDITY_VOLUME_WEIGHT",
        default: 0.6
    }

    /// Weight for pool-based liquidity score
    fn liquidity_pool_weight() -> f64 {
        key: "LIQUIDITY_POOL_WEIGHT",
        default: 0.4
    }

    /// Default liquidity score on error
    fn liquidity_error_default_score() -> f64 {
        key: "LIQUIDITY_ERROR_DEFAULT_SCORE",
        default: 0.3
    }

    // ── prediction ──

    /// Retention days for evaluated prediction records
    fn prediction_record_retention_days() -> u32 {
        key: "PREDICTION_RECORD_RETENTION_DAYS",
        default: 30
    }

    /// Retention days for unevaluated predictions
    fn prediction_unevaluated_retention_days() -> u32 {
        key: "PREDICTION_UNEVALUATED_RETENTION_DAYS",
        default: 20
    }

    /// Time tolerance for prediction evaluation in minutes
    fn prediction_eval_tolerance_minutes() -> i64 {
        key: "PREDICTION_EVAL_TOLERANCE_MINUTES",
        default: 30
    }

    /// Window size for accuracy calculation
    fn prediction_accuracy_window() -> i64 {
        key: "PREDICTION_ACCURACY_WINDOW",
        default: 20
    }

    /// Min samples needed for accuracy evaluation
    fn prediction_accuracy_min_samples() -> usize {
        key: "PREDICTION_ACCURACY_MIN_SAMPLES",
        default: 5
    }

    /// MAPE threshold for excellent predictions
    fn prediction_mape_excellent() -> f64 {
        key: "PREDICTION_MAPE_EXCELLENT",
        default: 3.0
    }

    /// MAPE threshold for poor predictions
    fn prediction_mape_poor() -> f64 {
        key: "PREDICTION_MAPE_POOR",
        default: 15.0
    }

    // ── persistence: database_url, pg_pool_size, instance_id moved to StartupConfig ──
}

static TYPED: LazyLock<ConfigResolver> = LazyLock::new(|| ConfigResolver);

/// Returns a reference to the global typed config resolver.
///
/// Each accessor resolves the value at call time through the priority chain
/// (CONFIG_STORE > DB_STORE > env > defaults).
pub fn typed() -> &'static ConfigResolver {
    &TYPED
}

#[cfg(test)]
mod tests;
