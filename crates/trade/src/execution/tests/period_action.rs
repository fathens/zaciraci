use bigdecimal::BigDecimal;
use chrono::{Duration, NaiveDate, NaiveDateTime};
use common::types::{NearValue, YoctoAmount};
use persistence::evaluation_period::EvaluationPeriod;

use crate::execution::{PeriodAction, determine_period_action, drawdown_triggers_breaker};

fn fixed_now() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 4, 16)
        .expect("valid date")
        .and_hms_opt(0, 0, 0)
        .expect("valid time")
}

fn period_at(start: NaiveDateTime, period_id: &str) -> EvaluationPeriod {
    EvaluationPeriod {
        id: 1,
        period_id: period_id.to_string(),
        start_time: start,
        initial_value: YoctoAmount::zero(),
        selected_tokens: None,
        created_at: start,
    }
}

#[test]
fn bootstrap_when_no_period_exists() {
    let action = determine_period_action(None, fixed_now(), 10);
    assert!(matches!(action, PeriodAction::Bootstrap));
}

#[test]
fn continue_when_within_period() {
    let now = fixed_now();
    let start = now - Duration::days(3);
    let period = period_at(start, "p1");
    let action = determine_period_action(Some(period), now, 10);
    match action {
        PeriodAction::Continue {
            period_id,
            days_elapsed,
            ..
        } => {
            assert_eq!(period_id, "p1");
            assert_eq!(days_elapsed, 3);
        }
        other => panic!("expected Continue, got {other:?}"),
    }
}

#[test]
fn end_and_start_new_when_period_elapsed() {
    let now = fixed_now();
    let start = now - Duration::days(10);
    let period = period_at(start, "p2");
    let action = determine_period_action(Some(period), now, 10);
    match action {
        PeriodAction::EndAndStartNew {
            period_id,
            days_elapsed,
            ..
        } => {
            assert_eq!(period_id, "p2");
            assert_eq!(days_elapsed, 10);
        }
        other => panic!("expected EndAndStartNew, got {other:?}"),
    }
}

#[test]
fn end_and_start_new_at_boundary_day() {
    // exactly evaluation_period_days days elapsed → end period
    let now = fixed_now();
    let start = now - Duration::days(7);
    let period = period_at(start, "p3");
    let action = determine_period_action(Some(period), now, 7);
    assert!(matches!(action, PeriodAction::EndAndStartNew { .. }));
}

#[test]
fn continue_one_second_before_boundary() {
    // 6 days, 23 hours, 59 seconds → still Continue (num_days truncates)
    let now = fixed_now();
    let start = now - Duration::days(6) - Duration::hours(23) - Duration::seconds(59);
    let period = period_at(start, "p4");
    let action = determine_period_action(Some(period), now, 7);
    assert!(matches!(action, PeriodAction::Continue { .. }));
}

fn near(v: i64) -> NearValue {
    NearValue::from_near(BigDecimal::from(v))
}

fn near_decimal(s: &str) -> NearValue {
    NearValue::from_near(s.parse::<BigDecimal>().expect("valid decimal"))
}

#[test]
fn dd_breaker_triggers_when_below_threshold() {
    // initial=100, threshold=15% → trigger when current < 85
    assert!(drawdown_triggers_breaker(
        &near(100),
        &near_decimal("84.99"),
        0.15
    ));
    assert!(drawdown_triggers_breaker(&near(100), &near(50), 0.15));
}

#[test]
fn dd_breaker_does_not_trigger_at_or_above_threshold() {
    // current == initial * (1 - threshold) → strict less-than comparison, no trigger
    assert!(!drawdown_triggers_breaker(&near(100), &near(85), 0.15));
    assert!(!drawdown_triggers_breaker(&near(100), &near(95), 0.15));
    assert!(!drawdown_triggers_breaker(&near(100), &near(150), 0.15));
}

#[test]
fn dd_breaker_returns_false_for_zero_initial() {
    // defensive: bootstrap-like initial=0 must not trigger or panic
    assert!(!drawdown_triggers_breaker(
        &NearValue::zero(),
        &near(50),
        0.15
    ));
}

#[test]
fn dd_breaker_returns_false_for_invalid_threshold() {
    // threshold <= 0 / >= 1 / NaN → defensive false
    assert!(!drawdown_triggers_breaker(&near(100), &near(0), 0.0));
    assert!(!drawdown_triggers_breaker(&near(100), &near(0), 1.0));
    assert!(!drawdown_triggers_breaker(&near(100), &near(0), -0.1));
    assert!(!drawdown_triggers_breaker(&near(100), &near(0), 1.5));
    assert!(!drawdown_triggers_breaker(&near(100), &near(0), f64::NAN));
}

#[test]
fn dd_breaker_handles_extreme_thresholds_within_range() {
    // 1%: trigger only at < 99
    assert!(drawdown_triggers_breaker(&near(100), &near(98), 0.01));
    assert!(!drawdown_triggers_breaker(&near(100), &near(99), 0.01));
    // 99%: trigger only at < 1; use a value clearly above the f64-rounded
    // dd_limit (= 100 × (1.0 - 0.99) which is not exactly 1.0 due to f64
    // representation) to avoid asserting on the precision artifact itself.
    assert!(drawdown_triggers_breaker(
        &near(100),
        &near_decimal("0.5"),
        0.99
    ));
    assert!(!drawdown_triggers_breaker(&near(100), &near(2), 0.99));
}

#[test]
fn dd_breaker_uses_exact_threshold_at_clean_f64_values() {
    // 0.5 is exactly representable in f64, so 1 - 0.5 = 0.5 exactly,
    // and 100 × 0.5 = 50 exactly: boundary semantics are unambiguous.
    assert!(!drawdown_triggers_breaker(&near(100), &near(50), 0.5));
    assert!(drawdown_triggers_breaker(
        &near(100),
        &near_decimal("49.999"),
        0.5
    ));
}
