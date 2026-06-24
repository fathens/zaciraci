use super::*;
use crate::types::TokenPrice;
use bigdecimal::BigDecimal;
use std::str::FromStr;

fn price(s: &str) -> TokenPrice {
    TokenPrice::from_near_per_token(BigDecimal::from_str(s).expect("valid price"))
}

#[test]
fn triggers_when_below_stop_price() {
    // entry=1.0, threshold=10% → trigger when current < 0.9
    assert!(should_trigger_stop_loss(&price("1.0"), &price("0.5"), 0.10));
    assert!(should_trigger_stop_loss(
        &price("1.0"),
        &price("0.89"),
        0.10
    ));
}

#[test]
fn does_not_trigger_at_or_above_stop_price() {
    // 1.0 - 0.10 is not exactly 0.9 in f64; use a value clearly above the
    // f64-rounded stop_price to avoid asserting on the precision artifact.
    assert!(!should_trigger_stop_loss(
        &price("1.0"),
        &price("0.91"),
        0.10
    ));
    assert!(!should_trigger_stop_loss(
        &price("1.0"),
        &price("1.0"),
        0.10
    ));
    assert!(!should_trigger_stop_loss(
        &price("1.0"),
        &price("1.5"),
        0.10
    ));
}

#[test]
fn returns_false_for_zero_entry() {
    // bootstrap-like entry=0 must not trigger or panic
    assert!(!should_trigger_stop_loss(
        &TokenPrice::zero(),
        &price("0.5"),
        0.10
    ));
}

#[test]
fn returns_false_for_invalid_threshold() {
    let entry = price("1.0");
    let current = price("0.5");
    assert!(!should_trigger_stop_loss(&entry, &current, 0.0));
    assert!(!should_trigger_stop_loss(&entry, &current, 1.0));
    assert!(!should_trigger_stop_loss(&entry, &current, -0.1));
    assert!(!should_trigger_stop_loss(&entry, &current, 1.5));
    assert!(!should_trigger_stop_loss(&entry, &current, f64::NAN));
    assert!(!should_trigger_stop_loss(&entry, &current, f64::INFINITY));
}

#[test]
fn handles_typical_thresholds() {
    // 5%: stop_price ≈ 0.95
    assert!(should_trigger_stop_loss(
        &price("1.0"),
        &price("0.94"),
        0.05
    ));
    assert!(!should_trigger_stop_loss(
        &price("1.0"),
        &price("0.96"),
        0.05
    ));
    // 30% (the upper of the documented clamp range): stop_price ≈ 0.70
    assert!(should_trigger_stop_loss(
        &price("1.0"),
        &price("0.69"),
        0.30
    ));
    // safely above the f64-rounded stop_price
    assert!(!should_trigger_stop_loss(
        &price("1.0"),
        &price("0.71"),
        0.30
    ));
}

#[test]
fn handles_large_entry_prices() {
    // entry = 1000 NEAR/token (high-priced asset like wrapped BTC)
    // threshold = 10% → trigger at current < 900
    assert!(should_trigger_stop_loss(
        &price("1000"),
        &price("899"),
        0.10
    ));
    assert!(!should_trigger_stop_loss(
        &price("1000"),
        &price("950"),
        0.10
    ));
}

#[test]
fn handles_small_entry_prices() {
    // entry = 0.0001 NEAR/token (memecoin-scale price)
    // threshold = 20% → trigger at current < 0.00008
    assert!(should_trigger_stop_loss(
        &price("0.0001"),
        &price("0.00007"),
        0.20
    ));
    assert!(!should_trigger_stop_loss(
        &price("0.0001"),
        &price("0.00009"),
        0.20
    ));
}

#[test]
fn uses_exact_threshold_at_clean_f64_values() {
    // 0.5 is exactly representable in f64, so 1 - 0.5 = 0.5 and
    // 1.0 × 0.5 = 0.5 exactly: boundary semantics are unambiguous.
    assert!(!should_trigger_stop_loss(&price("1.0"), &price("0.5"), 0.5));
    assert!(should_trigger_stop_loss(
        &price("1.0"),
        &price("0.499"),
        0.5
    ));
}
