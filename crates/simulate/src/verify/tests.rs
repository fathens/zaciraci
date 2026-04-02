use super::*;
use crate::cli::{OutputFormat, VerifyArgs};
use bigdecimal::BigDecimal;
use common::types::TokenSmallestUnits;

fn make_tx(from: &str, to: &str, estimated: u128, actual: Option<u128>) -> TradeTransaction {
    TradeTransaction {
        tx_id: format!("tx_{}", rand_id()),
        trade_batch_id: "batch_1".to_string(),
        from_token: from.to_string(),
        from_amount: TokenSmallestUnits::from(BigDecimal::from(1_000_000u64)),
        to_token: to.to_string(),
        to_amount: TokenSmallestUnits::from(BigDecimal::from(estimated)),
        timestamp: chrono::DateTime::from_timestamp(1700000000, 0)
            .unwrap()
            .naive_utc(),
        evaluation_period_id: "eval_test".to_string(),
        actual_to_amount: actual.map(BigDecimal::from),
    }
}

fn rand_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[test]
fn empty_transactions() {
    let result = analyze(&[]);
    assert_eq!(result.total_trades, 0);
    assert_eq!(result.trades_with_actual, 0);
    assert_eq!(result.trades_without_actual, 0);
    assert_eq!(result.trades_skipped, 0);
    assert_eq!(result.mean_error_pct, 0.0);
}

#[test]
fn all_without_actual() {
    let txs = vec![
        make_tx("wrap.near", "usdt.near", 1000, None),
        make_tx("wrap.near", "usdc.near", 2000, None),
    ];
    let result = analyze(&txs);
    assert_eq!(result.total_trades, 2);
    assert_eq!(result.trades_with_actual, 0);
    assert_eq!(result.trades_without_actual, 2);
    assert_eq!(result.mean_error_pct, 0.0);
}

#[test]
fn exact_match() {
    let txs = vec![make_tx("wrap.near", "usdt.near", 1000, Some(1000))];
    let result = analyze(&txs);
    assert_eq!(result.trades_with_actual, 1);
    assert!((result.mean_error_pct).abs() < 1e-10);
    assert!((result.median_error_pct).abs() < 1e-10);
    assert!((result.std_dev_pct).abs() < 1e-10);
}

#[test]
fn positive_slippage() {
    // actual > estimated = favorable execution
    let txs = vec![make_tx("wrap.near", "usdt.near", 1000, Some(1050))];
    let result = analyze(&txs);
    assert!((result.mean_error_pct - 5.0).abs() < 1e-10);
}

#[test]
fn negative_slippage() {
    // actual < estimated = unfavorable execution
    let txs = vec![make_tx("wrap.near", "usdt.near", 1000, Some(950))];
    let result = analyze(&txs);
    assert!((result.mean_error_pct - (-5.0)).abs() < 1e-10);
}

#[test]
fn mixed_slippage() {
    let txs = vec![
        make_tx("wrap.near", "usdt.near", 1000, Some(1100)), // +10%
        make_tx("wrap.near", "usdt.near", 1000, Some(900)),  // -10%
    ];
    let result = analyze(&txs);
    // mean should be ~0%
    assert!(result.mean_error_pct.abs() < 1e-10);
    assert_eq!(result.trades_with_actual, 2);
    // sample variance (Bessel's correction): sum((e-mean)^2) / (n-1) = 200/1 = 200
    // std_dev = sqrt(200) ≈ 14.1421
    assert!((result.std_dev_pct - 200.0_f64.sqrt()).abs() < 1e-10);
}

#[test]
fn by_token_pair_grouping() {
    let txs = vec![
        make_tx("wrap.near", "usdt.near", 1000, Some(990)),
        make_tx("wrap.near", "usdt.near", 1000, Some(980)),
        make_tx("wrap.near", "usdc.near", 2000, Some(1900)),
    ];
    let result = analyze(&txs);
    assert_eq!(result.by_token_pair.len(), 2);

    let usdt_stats = &result.by_token_pair["wrap.near -> usdt.near"];
    assert_eq!(usdt_stats.count, 2);

    let usdc_stats = &result.by_token_pair["wrap.near -> usdc.near"];
    assert_eq!(usdc_stats.count, 1);
    assert!((usdc_stats.mean_error_pct - (-5.0)).abs() < 1e-10);
}

#[test]
fn zero_estimated_skipped() {
    let txs = vec![make_tx("wrap.near", "usdt.near", 0, Some(100))];
    let result = analyze(&txs);
    // zero estimated amount should be skipped (not cause division by zero)
    assert_eq!(result.trades_with_actual, 0);
    assert_eq!(result.trades_skipped, 1);
    assert_eq!(result.trades_without_actual, 0);
    // total_trades = trades_with_actual + trades_without_actual + trades_skipped
    assert_eq!(result.total_trades, 1);
    assert_eq!(
        result.total_trades,
        result.trades_with_actual + result.trades_without_actual + result.trades_skipped
    );
}

#[test]
fn median_odd_count() {
    let txs = vec![
        make_tx("a", "b", 100, Some(110)), // +10%
        make_tx("a", "b", 100, Some(90)),  // -10%
        make_tx("a", "b", 100, Some(105)), // +5%
    ];
    let result = analyze(&txs);
    // sorted: -10, +5, +10 → median = +5
    assert!((result.median_error_pct - 5.0).abs() < 1e-10);
}

#[test]
fn p95_single_trade() {
    let txs = vec![make_tx("a", "b", 1000, Some(970))]; // -3%
    let result = analyze(&txs);
    assert!((result.p95_error_pct - 3.0).abs() < 1e-10);
    assert!((result.max_error_pct - 3.0).abs() < 1e-10);
}

#[test]
fn median_even_count() {
    let txs = vec![
        make_tx("a", "b", 100, Some(110)), // +10%
        make_tx("a", "b", 100, Some(90)),  // -10%
        make_tx("a", "b", 100, Some(105)), // +5%
        make_tx("a", "b", 100, Some(97)),  // -3%
    ];
    let result = analyze(&txs);
    // sorted: -10, -3, +5, +10 → median = (-3 + 5) / 2 = +1
    assert!((result.median_error_pct - 1.0).abs() < 1e-10);
}

#[test]
fn mixed_with_and_without_actual() {
    let txs = vec![
        make_tx("a", "b", 1000, Some(990)),  // has actual
        make_tx("a", "b", 1000, None),       // no actual
        make_tx("a", "b", 1000, Some(1010)), // has actual
    ];
    let result = analyze(&txs);
    assert_eq!(result.total_trades, 3);
    assert_eq!(result.trades_with_actual, 2);
    assert_eq!(result.trades_without_actual, 1);
}

#[test]
fn single_trade_std_dev_zero() {
    let txs = vec![make_tx("a", "b", 1000, Some(1050))]; // +5%
    let result = analyze(&txs);
    assert_eq!(result.trades_with_actual, 1);
    // n=1: sample variance is 0.0 (no Bessel's correction possible)
    assert!((result.std_dev_pct).abs() < 1e-10);
}

// --- parse_date_range ---

fn make_verify_args(start: &str, end: &str) -> VerifyArgs {
    VerifyArgs {
        start_date: start.to_string(),
        end_date: end.to_string(),
        format: OutputFormat::Text,
    }
}

#[test]
fn parse_date_range_valid() {
    let args = make_verify_args("2025-01-01", "2025-06-30");
    let (start, end) = parse_date_range(&args).unwrap();
    assert!(start < end);
}

#[test]
fn parse_date_range_start_equals_end() {
    let args = make_verify_args("2025-03-01", "2025-03-01");
    let err = parse_date_range(&args).unwrap_err();
    assert!(
        err.to_string()
            .contains("start-date must be before end-date")
    );
}

#[test]
fn parse_date_range_start_after_end() {
    let args = make_verify_args("2025-06-01", "2025-01-01");
    let err = parse_date_range(&args).unwrap_err();
    assert!(
        err.to_string()
            .contains("start-date must be before end-date")
    );
}
