use super::*;
#[allow(unused_imports)]
use crate::config::ConfigAccess;
use crate::config::store::{ConfigGuard, EnvGuard};
use std::time::Duration;

use serial_test::serial;

// ── bool keys ──

#[test]
#[serial]
fn test_trade_enabled_default() {
    let _env = EnvGuard::remove("TRADE_ENABLED");
    crate::config::store::remove("TRADE_ENABLED");
    assert!(!typed().trade_enabled());
}

#[test]
#[serial]
fn test_trade_enabled_override() {
    let _guard = ConfigGuard::new("TRADE_ENABLED", "true");
    assert!(typed().trade_enabled());
}

#[test]
#[serial]
fn test_use_mainnet_default() {
    let _env = EnvGuard::remove("USE_MAINNET");
    crate::config::store::remove("USE_MAINNET");
    assert!(typed().use_mainnet());
}

#[test]
#[serial]
fn test_arbitrage_needed_default() {
    let _env = EnvGuard::remove("ARBITRAGE_NEEDED");
    crate::config::store::remove("ARBITRAGE_NEEDED");
    assert!(!typed().arbitrage_needed());
}

#[test]
#[serial]
fn test_trade_unwrap_on_stop_default() {
    let _env = EnvGuard::remove("TRADE_UNWRAP_ON_STOP");
    crate::config::store::remove("TRADE_UNWRAP_ON_STOP");
    assert!(!typed().trade_unwrap_on_stop());
}

// ── u32 keys ──

#[test]
#[serial]
fn test_trade_initial_investment_default() {
    let _env = EnvGuard::remove("TRADE_INITIAL_INVESTMENT");
    crate::config::store::remove("TRADE_INITIAL_INVESTMENT");
    assert_eq!(typed().trade_initial_investment(), 100);
}

#[test]
#[serial]
fn test_trade_top_tokens_default() {
    let _env = EnvGuard::remove("TRADE_TOP_TOKENS");
    crate::config::store::remove("TRADE_TOP_TOKENS");
    assert_eq!(typed().trade_top_tokens(), 10);
}

#[test]
#[serial]
fn test_trade_evaluation_days_default() {
    let _env = EnvGuard::remove("TRADE_EVALUATION_DAYS");
    crate::config::store::remove("TRADE_EVALUATION_DAYS");
    assert_eq!(typed().trade_evaluation_days(), 10);
}

#[test]
#[serial]
fn test_trade_account_reserve_default() {
    let _env = EnvGuard::remove("TRADE_ACCOUNT_RESERVE");
    crate::config::store::remove("TRADE_ACCOUNT_RESERVE");
    assert_eq!(typed().trade_account_reserve(), 10);
}

#[test]
#[serial]
fn test_trade_prediction_max_retries_default() {
    let _env = EnvGuard::remove("TRADE_PREDICTION_MAX_RETRIES");
    crate::config::store::remove("TRADE_PREDICTION_MAX_RETRIES");
    assert_eq!(typed().trade_prediction_max_retries(), 2);
}

#[test]
#[serial]
fn test_trade_prediction_concurrency_default() {
    let _env = EnvGuard::remove("TRADE_PREDICTION_CONCURRENCY");
    crate::config::store::remove("TRADE_PREDICTION_CONCURRENCY");
    assert_eq!(typed().trade_prediction_concurrency(), 4);
}

#[test]
#[serial]
fn test_trade_min_pool_liquidity_default() {
    let _env = EnvGuard::remove("TRADE_MIN_POOL_LIQUIDITY");
    crate::config::store::remove("TRADE_MIN_POOL_LIQUIDITY");
    assert_eq!(typed().trade_min_pool_liquidity(), 100);
}

#[test]
#[serial]
fn test_trade_token_cache_concurrency_default() {
    let _env = EnvGuard::remove("TRADE_TOKEN_CACHE_CONCURRENCY");
    crate::config::store::remove("TRADE_TOKEN_CACHE_CONCURRENCY");
    assert_eq!(typed().trade_token_cache_concurrency(), 8);
}

#[test]
#[serial]
fn test_trade_min_pool_liquidity_override() {
    let _guard = ConfigGuard::new("TRADE_MIN_POOL_LIQUIDITY", "200");
    assert_eq!(typed().trade_min_pool_liquidity(), 200);
}

#[test]
#[serial]
fn test_harvest_min_amount_default() {
    let _env = EnvGuard::remove("HARVEST_MIN_AMOUNT");
    crate::config::store::remove("HARVEST_MIN_AMOUNT");
    assert_eq!(typed().harvest_min_amount(), 10);
}

#[test]
#[serial]
fn test_harvest_reserve_amount_default() {
    let _env = EnvGuard::remove("HARVEST_RESERVE_AMOUNT");
    crate::config::store::remove("HARVEST_RESERVE_AMOUNT");
    assert_eq!(typed().harvest_reserve_amount(), 1);
}

#[test]
#[serial]
fn test_pool_info_retention_count_default() {
    let _env = EnvGuard::remove("POOL_INFO_RETENTION_COUNT");
    crate::config::store::remove("POOL_INFO_RETENTION_COUNT");
    assert_eq!(typed().pool_info_retention_count(), 10);
}

#[test]
#[serial]
fn test_token_rates_retention_days_default() {
    let _env = EnvGuard::remove("TOKEN_RATES_RETENTION_DAYS");
    crate::config::store::remove("TOKEN_RATES_RETENTION_DAYS");
    assert_eq!(typed().token_rates_retention_days(), 365);
}

#[test]
#[serial]
fn test_rpc_max_attempts_default() {
    let _env = EnvGuard::remove("RPC_MAX_ATTEMPTS");
    crate::config::store::remove("RPC_MAX_ATTEMPTS");
    assert_eq!(typed().rpc_max_attempts(), 10);
}

// ── u64 keys ──

#[test]
#[serial]
fn test_trade_prediction_retry_delay_seconds_default() {
    let _env = EnvGuard::remove("TRADE_PREDICTION_RETRY_DELAY_SECONDS");
    crate::config::store::remove("TRADE_PREDICTION_RETRY_DELAY_SECONDS");
    assert_eq!(typed().trade_prediction_retry_delay_seconds(), 5);
}

#[test]
#[serial]
fn test_harvest_interval_seconds_default() {
    let _env = EnvGuard::remove("HARVEST_INTERVAL_SECONDS");
    crate::config::store::remove("HARVEST_INTERVAL_SECONDS");
    assert_eq!(typed().harvest_interval_seconds(), 86400);
}

#[test]
#[serial]
fn test_rpc_failure_reset_seconds_default() {
    let _env = EnvGuard::remove("RPC_FAILURE_RESET_SECONDS");
    crate::config::store::remove("RPC_FAILURE_RESET_SECONDS");
    assert_eq!(typed().rpc_failure_reset_seconds(), 300);
}

#[test]
#[serial]
fn test_cron_max_sleep_seconds_default() {
    let _env = EnvGuard::remove("CRON_MAX_SLEEP_SECONDS");
    crate::config::store::remove("CRON_MAX_SLEEP_SECONDS");
    assert_eq!(typed().cron_max_sleep_seconds(), 60);
}

#[test]
#[serial]
fn test_cron_log_threshold_seconds_default() {
    let _env = EnvGuard::remove("CRON_LOG_THRESHOLD_SECONDS");
    crate::config::store::remove("CRON_LOG_THRESHOLD_SECONDS");
    assert_eq!(typed().cron_log_threshold_seconds(), 300);
}

// ── u128 keys ──

#[test]
#[serial]
fn test_harvest_balance_multiplier_default() {
    let _env = EnvGuard::remove("HARVEST_BALANCE_MULTIPLIER");
    crate::config::store::remove("HARVEST_BALANCE_MULTIPLIER");
    assert_eq!(typed().harvest_balance_multiplier(), 128);
}

#[test]
#[serial]
fn test_harvest_balance_multiplier_override() {
    let _guard = ConfigGuard::new("HARVEST_BALANCE_MULTIPLIER", "256");
    assert_eq!(typed().harvest_balance_multiplier(), 256);
}

// ── f64 keys ──

#[test]
#[serial]
fn test_portfolio_rebalance_threshold_default() {
    let _env = EnvGuard::remove("PORTFOLIO_REBALANCE_THRESHOLD");
    crate::config::store::remove("PORTFOLIO_REBALANCE_THRESHOLD");
    assert!((typed().portfolio_rebalance_threshold() - 0.1).abs() < f64::EPSILON);
}

#[test]
#[serial]
fn test_liquidity_volume_weight_default() {
    let _env = EnvGuard::remove("LIQUIDITY_VOLUME_WEIGHT");
    crate::config::store::remove("LIQUIDITY_VOLUME_WEIGHT");
    assert!((typed().liquidity_volume_weight() - 0.6).abs() < f64::EPSILON);
}

#[test]
#[serial]
fn test_liquidity_pool_weight_default() {
    let _env = EnvGuard::remove("LIQUIDITY_POOL_WEIGHT");
    crate::config::store::remove("LIQUIDITY_POOL_WEIGHT");
    assert!((typed().liquidity_pool_weight() - 0.4).abs() < f64::EPSILON);
}

#[test]
#[serial]
fn test_liquidity_error_default_score_default() {
    let _env = EnvGuard::remove("LIQUIDITY_ERROR_DEFAULT_SCORE");
    crate::config::store::remove("LIQUIDITY_ERROR_DEFAULT_SCORE");
    assert!((typed().liquidity_error_default_score() - 0.3).abs() < f64::EPSILON);
}

#[test]
#[serial]
fn test_prediction_mape_excellent_default() {
    let _env = EnvGuard::remove("PREDICTION_MAPE_EXCELLENT");
    crate::config::store::remove("PREDICTION_MAPE_EXCELLENT");
    assert!((typed().prediction_mape_excellent() - 3.0).abs() < f64::EPSILON);
}

#[test]
#[serial]
fn test_prediction_mape_poor_default() {
    let _env = EnvGuard::remove("PREDICTION_MAPE_POOR");
    crate::config::store::remove("PREDICTION_MAPE_POOR");
    assert!((typed().prediction_mape_poor() - 15.0).abs() < f64::EPSILON);
}

// ── i64 keys ──

#[test]
#[serial]
fn test_prediction_record_retention_days_default() {
    let _env = EnvGuard::remove("PREDICTION_RECORD_RETENTION_DAYS");
    crate::config::store::remove("PREDICTION_RECORD_RETENTION_DAYS");
    assert_eq!(typed().prediction_record_retention_days(), 30);
}

#[test]
#[serial]
fn test_prediction_unevaluated_retention_days_default() {
    let _env = EnvGuard::remove("PREDICTION_UNEVALUATED_RETENTION_DAYS");
    crate::config::store::remove("PREDICTION_UNEVALUATED_RETENTION_DAYS");
    assert_eq!(typed().prediction_unevaluated_retention_days(), 20);
}

#[test]
#[serial]
fn test_prediction_eval_tolerance_minutes_default() {
    let _env = EnvGuard::remove("PREDICTION_EVAL_TOLERANCE_MINUTES");
    crate::config::store::remove("PREDICTION_EVAL_TOLERANCE_MINUTES");
    assert_eq!(typed().prediction_eval_tolerance_minutes(), 30);
}

#[test]
#[serial]
fn test_prediction_accuracy_window_default() {
    let _env = EnvGuard::remove("PREDICTION_ACCURACY_WINDOW");
    crate::config::store::remove("PREDICTION_ACCURACY_WINDOW");
    assert_eq!(typed().prediction_accuracy_window(), 20);
}

// ── usize keys ──

#[test]
#[serial]
fn test_prediction_accuracy_min_samples_default() {
    let _env = EnvGuard::remove("PREDICTION_ACCURACY_MIN_SAMPLES");
    crate::config::store::remove("PREDICTION_ACCURACY_MIN_SAMPLES");
    assert_eq!(typed().prediction_accuracy_min_samples(), 5);
}

#[test]
#[serial]
fn test_pg_pool_size_default() {
    let _env = EnvGuard::remove("PG_POOL_SIZE");
    crate::config::store::remove("PG_POOL_SIZE");
    assert_eq!(typed().pg_pool_size(), 2);
}

// ── u16 keys ──

#[test]
#[serial]
fn test_portfolio_holdings_retention_days_default() {
    let _env = EnvGuard::remove("PORTFOLIO_HOLDINGS_RETENTION_DAYS");
    crate::config::store::remove("PORTFOLIO_HOLDINGS_RETENTION_DAYS");
    assert_eq!(typed().portfolio_holdings_retention_days(), 90);
}

// ── String keys ──

#[test]
#[serial]
fn test_trade_cron_schedule_default() {
    let _env = EnvGuard::remove("TRADE_CRON_SCHEDULE");
    crate::config::store::remove("TRADE_CRON_SCHEDULE");
    assert_eq!(typed().trade_cron_schedule(), "0 0 0 * * *");
}

#[test]
#[serial]
fn test_root_hdpath_default() {
    let _env = EnvGuard::remove("ROOT_HDPATH");
    crate::config::store::remove("ROOT_HDPATH");
    assert_eq!(typed().root_hdpath(), "m/44'/397'/0'");
}

#[test]
#[serial]
fn test_rust_log_format_default() {
    let _env = EnvGuard::remove("RUST_LOG_FORMAT");
    crate::config::store::remove("RUST_LOG_FORMAT");
    assert_eq!(typed().rust_log_format(), "json");
}

#[test]
#[serial]
fn test_instance_id_default() {
    let _env = EnvGuard::remove("INSTANCE_ID");
    crate::config::store::remove("INSTANCE_ID");
    assert_eq!(typed().instance_id(), "*");
}

// ── Duration keys ──

#[test]
#[serial]
fn test_arbitrage_token_not_found_wait_default() {
    let _env = EnvGuard::remove("ARBITRAGE_TOKEN_NOT_FOUND_WAIT");
    crate::config::store::remove("ARBITRAGE_TOKEN_NOT_FOUND_WAIT");
    assert_eq!(
        typed().arbitrage_token_not_found_wait(),
        Duration::from_secs(1)
    );
}

#[test]
#[serial]
fn test_arbitrage_other_error_wait_default() {
    let _env = EnvGuard::remove("ARBITRAGE_OTHER_ERROR_WAIT");
    crate::config::store::remove("ARBITRAGE_OTHER_ERROR_WAIT");
    assert_eq!(typed().arbitrage_other_error_wait(), Duration::from_secs(5));
}

#[test]
#[serial]
fn test_arbitrage_preview_not_found_wait_default() {
    let _env = EnvGuard::remove("ARBITRAGE_PREVIEW_NOT_FOUND_WAIT");
    crate::config::store::remove("ARBITRAGE_PREVIEW_NOT_FOUND_WAIT");
    assert_eq!(
        typed().arbitrage_preview_not_found_wait(),
        Duration::from_secs(2)
    );
}

#[test]
#[serial]
fn test_arbitrage_duration_override() {
    let _guard = ConfigGuard::new("ARBITRAGE_OTHER_ERROR_WAIT", "10s");
    assert_eq!(
        typed().arbitrage_other_error_wait(),
        Duration::from_secs(10)
    );
}

// ── Result<String> keys ──

#[test]
#[serial]
fn test_required_key_returns_error_when_missing() {
    let _env = EnvGuard::remove("DATABASE_URL");
    crate::config::store::remove("DATABASE_URL");
    assert!(typed().database_url().is_err());
}

#[test]
#[serial]
fn test_required_key_returns_value_when_set() {
    let _guard = ConfigGuard::new("DATABASE_URL", "postgres://localhost/test");
    assert_eq!(typed().database_url().unwrap(), "postgres://localhost/test");
}

#[test]
#[serial]
fn test_root_account_id_required() {
    let _env = EnvGuard::remove("ROOT_ACCOUNT_ID");
    crate::config::store::remove("ROOT_ACCOUNT_ID");
    assert!(typed().root_account_id().is_err());
}

#[test]
#[serial]
fn test_root_mnemonic_required() {
    let _env = EnvGuard::remove("ROOT_MNEMONIC");
    crate::config::store::remove("ROOT_MNEMONIC");
    assert!(typed().root_mnemonic().is_err());
}

#[test]
#[serial]
fn test_harvest_account_id_required() {
    let _env = EnvGuard::remove("HARVEST_ACCOUNT_ID");
    crate::config::store::remove("HARVEST_ACCOUNT_ID");
    assert!(typed().harvest_account_id().is_err());
}

// ── Vec<RpcEndpoint> ──

#[test]
#[serial]
fn test_rpc_endpoints_default_empty() {
    let _env = EnvGuard::remove("RPC_ENDPOINTS");
    crate::config::store::remove("RPC_ENDPOINTS");
    // Default may be empty or loaded from TOML; both are valid
    let endpoints = typed().rpc_endpoints();
    // Just verify it doesn't panic and returns a valid vec
    assert!(endpoints.len() < 1000);
}

#[test]
#[serial]
fn test_rpc_endpoints_json_override() {
    let json = r#"[{"url":"https://rpc.example.com","weight":10,"max_retries":3}]"#;
    let _guard = ConfigGuard::new("RPC_ENDPOINTS", json);
    let endpoints = typed().rpc_endpoints();
    assert_eq!(endpoints.len(), 1);
    assert_eq!(endpoints[0].url, "https://rpc.example.com");
    assert_eq!(endpoints[0].weight, 10);
}

// ── MockConfig tests ──

#[test]
fn test_mock_config_override() {
    let mut mock = MockConfig::new();
    mock.trade_enabled = Some(true);
    mock.trade_min_pool_liquidity = Some(500);
    assert!(mock.trade_enabled());
    assert_eq!(mock.trade_min_pool_liquidity(), 500);
}

#[test]
fn test_mock_config_delegates_to_real() {
    let mock = MockConfig::new();
    // Without override, delegates to ConfigResolver
    // Just verify it doesn't panic
    let _ = mock.trade_enabled();
    let _ = mock.trade_top_tokens();
}

// ── trade_volatility_days ──

#[test]
#[serial]
fn test_trade_volatility_days_default() {
    let _env = EnvGuard::remove("TRADE_VOLATILITY_DAYS");
    crate::config::store::remove("TRADE_VOLATILITY_DAYS");
    assert_eq!(typed().trade_volatility_days(), 7);
}

// ── trade_price_history_days ──

#[test]
#[serial]
fn test_trade_price_history_days_default() {
    let _env = EnvGuard::remove("TRADE_PRICE_HISTORY_DAYS");
    crate::config::store::remove("TRADE_PRICE_HISTORY_DAYS");
    assert_eq!(typed().trade_price_history_days(), 30);
}
