use once_cell::sync::Lazy;
use std::time::Duration;

// ── ConfigResolve trait: type-specific config resolution ──

pub(crate) trait ConfigResolve: Sized {
    type Default;
    fn resolve(key: &str, default: Self::Default) -> Self;
}

impl ConfigResolve for bool {
    type Default = bool;
    fn resolve(key: &str, default: bool) -> Self {
        crate::config::store::get(key)
            .ok()
            .and_then(|v| v.to_lowercase().parse::<bool>().ok())
            .unwrap_or(default)
    }
}

impl ConfigResolve for String {
    type Default = &'static str;
    fn resolve(key: &str, default: &'static str) -> Self {
        crate::config::store::get(key).unwrap_or_else(|_| default.to_string())
    }
}

macro_rules! impl_config_resolve_numeric {
    ($($ty:ty),*) => {
        $(impl ConfigResolve for $ty {
            type Default = $ty;
            fn resolve(key: &str, default: $ty) -> Self {
                crate::config::store::get(key)
                    .ok()
                    .and_then(|v| v.parse::<$ty>().ok())
                    .unwrap_or(default)
            }
        })*
    }
}

impl_config_resolve_numeric!(u16, u32, u64, u128, usize, i64, f64);

impl ConfigResolve for Duration {
    type Default = Duration;
    fn resolve(key: &str, default: Duration) -> Self {
        crate::config::store::get(key)
            .ok()
            .and_then(|v| humantime::parse_duration(&v).ok())
            .unwrap_or(default)
    }
}

impl ConfigResolve for anyhow::Result<String> {
    type Default = ();
    fn resolve(key: &str, _default: ()) -> Self {
        crate::config::store::get(key)
            .map_err(|_| anyhow::anyhow!("required config key not found: {}", key))
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

    /// Days of price history for predictions
    fn trade_price_history_days() -> u32 {
        key: "TRADE_PRICE_HISTORY_DAYS",
        default: 30
    }

    /// Days of volatility data
    fn trade_volatility_days() -> u32 {
        key: "TRADE_VOLATILITY_DAYS",
        default: 7
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
    fn rpc_max_attempts() -> u32 {
        key: "RPC_MAX_ATTEMPTS",
        default: 10
    }

    // ── cron ──

    /// Number of historical pool info records to keep
    fn pool_info_retention_count() -> u32 {
        key: "POOL_INFO_RETENTION_COUNT",
        default: 10
    }

    /// Retention period for token rate records in days
    fn token_rates_retention_days() -> u32 {
        key: "TOKEN_RATES_RETENTION_DAYS",
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

    /// Retention days for portfolio holding snapshots
    fn portfolio_holdings_retention_days() -> u16 {
        key: "PORTFOLIO_HOLDINGS_RETENTION_DAYS",
        default: 90
    }

    // ── prediction ──

    /// Retention days for evaluated prediction records
    fn prediction_record_retention_days() -> i64 {
        key: "PREDICTION_RECORD_RETENTION_DAYS",
        default: 30
    }

    /// Retention days for unevaluated predictions
    fn prediction_unevaluated_retention_days() -> i64 {
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

static TYPED: Lazy<ConfigResolver> = Lazy::new(|| ConfigResolver);

/// Returns a reference to the global typed config resolver.
///
/// Each accessor resolves the value at call time through the priority chain
/// (CONFIG_STORE > DB_STORE > env > defaults).
pub fn typed() -> &'static ConfigResolver {
    &TYPED
}

#[cfg(test)]
mod tests;
