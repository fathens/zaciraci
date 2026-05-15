use chrono::{Duration, NaiveDate, NaiveDateTime};
use common::types::YoctoAmount;
use persistence::evaluation_period::EvaluationPeriod;

use crate::execution::{PeriodAction, determine_period_action};

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
