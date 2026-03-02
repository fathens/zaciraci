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
fn test_harvest_account_id_required() {
    let _env = EnvGuard::remove("HARVEST_ACCOUNT_ID");
    crate::config::store::remove("HARVEST_ACCOUNT_ID");
    assert!(typed().harvest_account_id().is_err());
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

// ── ConfigValueType tests ──

#[test]
fn test_config_value_type_as_str() {
    assert_eq!(ConfigValueType::Bool.as_str(), "bool");
    assert_eq!(ConfigValueType::U16.as_str(), "u16");
    assert_eq!(ConfigValueType::U32.as_str(), "u32");
    assert_eq!(ConfigValueType::U64.as_str(), "u64");
    assert_eq!(ConfigValueType::U128.as_str(), "u128");
    assert_eq!(ConfigValueType::I64.as_str(), "i64");
    assert_eq!(ConfigValueType::F64.as_str(), "f64");
    assert_eq!(ConfigValueType::String.as_str(), "string");
    assert_eq!(ConfigValueType::RequiredString.as_str(), "string(required)");
    assert_eq!(ConfigValueType::Duration.as_str(), "duration");
}

#[test]
fn test_config_value_type_display() {
    assert_eq!(format!("{}", ConfigValueType::Bool), "bool");
    assert_eq!(format!("{}", ConfigValueType::String), "string");
    assert_eq!(
        format!("{}", ConfigValueType::RequiredString),
        "string(required)"
    );
    assert_eq!(format!("{}", ConfigValueType::Duration), "duration");
}

// ── display_string tests ──

#[test]
fn test_display_string_bool() {
    assert_eq!(<bool as ConfigResolve>::display_string(true), "true");
    assert_eq!(<bool as ConfigResolve>::display_string(false), "false");
}

#[test]
fn test_display_string_u32() {
    assert_eq!(<u32 as ConfigResolve>::display_string(42), "42");
}

#[test]
fn test_display_string_string() {
    assert_eq!(
        <String as ConfigResolve>::display_string("hello".to_string()),
        "hello"
    );
}

#[test]
fn test_display_string_duration() {
    let d = Duration::from_secs(5);
    let s = <Duration as ConfigResolve>::display_string(d);
    assert_eq!(s, "5s");
}

#[test]
fn test_display_string_result_ok() {
    let v: anyhow::Result<String> = Ok("value".to_string());
    assert_eq!(
        <anyhow::Result<String> as ConfigResolve>::display_string(v),
        "value"
    );
}

#[test]
fn test_display_string_result_err() {
    let v: anyhow::Result<String> = Err(anyhow::anyhow!("not found"));
    assert_eq!(
        <anyhow::Result<String> as ConfigResolve>::display_string(v),
        "(未設定)"
    );
}

// ── KEY_DEFINITIONS tests ──

#[test]
fn test_key_definitions_count() {
    // define_typed_config! に定義されたキーの数と一致すること
    assert_eq!(KEY_DEFINITIONS.len(), 42);
}

#[test]
fn test_key_definitions_fields_non_empty() {
    for def in KEY_DEFINITIONS {
        assert!(!def.key.is_empty(), "key should not be empty");
        assert!(
            !def.description.trim().is_empty(),
            "description should not be empty for key: {}",
            def.key
        );
        assert!(
            !def.type_name.is_empty(),
            "type_name should not be empty for key: {}",
            def.key
        );
        assert!(
            !def.default_value.is_empty(),
            "default_value should not be empty for key: {}",
            def.key
        );
    }
}

#[test]
fn test_key_definitions_no_duplicate_keys() {
    let mut keys: Vec<&str> = KEY_DEFINITIONS.iter().map(|d| d.key).collect();
    let original_len = keys.len();
    keys.sort();
    keys.dedup();
    assert_eq!(
        keys.len(),
        original_len,
        "KEY_DEFINITIONS should not have duplicate keys"
    );
}

#[test]
fn test_key_definitions_trade_enabled_metadata() {
    let def = KEY_DEFINITIONS
        .iter()
        .find(|d| d.key == "TRADE_ENABLED")
        .expect("TRADE_ENABLED should exist in KEY_DEFINITIONS");
    assert_eq!(def.type_name, "bool");
    assert_eq!(def.default_value, "false");
    assert!(def.description.contains("trading"));
}

// ── resolve_all_without_db tests ──

#[test]
#[serial]
fn test_resolve_all_without_db_count() {
    let resolved = resolve_all_without_db();
    assert_eq!(resolved.len(), KEY_DEFINITIONS.len());
}

#[test]
#[serial]
fn test_resolve_all_without_db_fields_non_empty() {
    let resolved = resolve_all_without_db();
    for info in &resolved {
        assert!(!info.key.is_empty(), "key should not be empty");
        assert!(
            !info.description.is_empty(),
            "description should not be empty for key: {}",
            info.key
        );
        assert!(
            !info.type_name.is_empty(),
            "type_name should not be empty for key: {}",
            info.key
        );
        // resolved_value can be "(未設定)" for required keys
    }
}

#[test]
#[serial]
fn test_resolve_all_without_db_trade_enabled() {
    let _env = EnvGuard::remove("TRADE_ENABLED");
    crate::config::store::remove("TRADE_ENABLED");

    let resolved = resolve_all_without_db();
    let info = resolved
        .iter()
        .find(|r| r.key == "TRADE_ENABLED")
        .expect("TRADE_ENABLED should exist in resolved list");
    assert_eq!(info.type_name, "bool");
    assert_eq!(info.resolved_value, "false");
}

#[test]
#[serial]
fn test_resolve_all_without_db_excludes_db() {
    use crate::config::store::DbStoreGuard;
    use std::collections::HashMap;

    let _db_guard = DbStoreGuard::new();
    let _env = EnvGuard::remove("TRADE_ENABLED");
    crate::config::store::remove("TRADE_ENABLED");

    // DB に true をセットしても resolve_all_without_db はスキップする
    crate::config::store::load_db_config(HashMap::from([(
        "TRADE_ENABLED".to_string(),
        "true".to_string(),
    )]));

    let resolved = resolve_all_without_db();
    let info = resolved
        .iter()
        .find(|r| r.key == "TRADE_ENABLED")
        .expect("TRADE_ENABLED should exist");
    // DB の値は無視され、デフォルトの false が返る
    assert_eq!(info.resolved_value, "false");
}

#[test]
#[serial]
fn test_resolve_all_without_db_harvest_account_id_unset() {
    let _env = EnvGuard::remove("HARVEST_ACCOUNT_ID");
    crate::config::store::remove("HARVEST_ACCOUNT_ID");

    let resolved = resolve_all_without_db();
    let info = resolved
        .iter()
        .find(|r| r.key == "HARVEST_ACCOUNT_ID")
        .expect("HARVEST_ACCOUNT_ID should exist");
    assert_eq!(info.type_name, "string(required)");
    assert_eq!(info.resolved_value, "(未設定)");
}
