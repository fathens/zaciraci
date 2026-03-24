use super::*;

#[test]
fn test_unprotected_returns_zero() {
    let result = calculate_min_out(1_000_000, &SlippagePolicy::Unprotected).unwrap();
    assert_eq!(result, 0);
}

#[test]
fn test_basic_expected_return() {
    // 5% expected return → slippage_budget = 5% → protection = 95%
    let policy = SlippagePolicy::FromExpectedReturn(ExpectedReturn::new(0.05));
    let result = calculate_min_out(10_000, &policy).unwrap();
    // 10_000 * 9500 / 10_000 = 9500
    assert_eq!(result, 9500);
}

#[test]
fn test_min_slippage_budget_clamp() {
    // 0.1% expected return → clamped to MIN_SLIPPAGE_BUDGET (0.5%) → protection = 99.5%
    let policy = SlippagePolicy::FromExpectedReturn(ExpectedReturn::new(0.001));
    let result = calculate_min_out(10_000, &policy).unwrap();
    // 10_000 * 9950 / 10_000 = 9950
    assert_eq!(result, 9950);
}

#[test]
fn test_max_slippage_budget_clamp() {
    // 50% expected return → clamped to MAX_SLIPPAGE_BUDGET (15%) → protection = 85%
    let policy = SlippagePolicy::FromExpectedReturn(ExpectedReturn::new(0.50));
    let result = calculate_min_out(10_000, &policy).unwrap();
    // 10_000 * 8500 / 10_000 = 8500
    assert_eq!(result, 8500);
}

#[test]
fn test_negative_expected_return_uses_abs() {
    // -5% → abs → 5% → same as positive 5%
    let policy = SlippagePolicy::FromExpectedReturn(ExpectedReturn::new(-0.05));
    let result = calculate_min_out(10_000, &policy).unwrap();
    assert_eq!(result, 9500);
}

#[test]
fn test_zero_estimated_output() {
    let policy = SlippagePolicy::FromExpectedReturn(ExpectedReturn::new(0.05));
    let result = calculate_min_out(0, &policy).unwrap();
    assert_eq!(result, 0);
}

#[test]
fn test_yocto_near_scale() {
    // 100 NEAR = 10^26 yoctoNEAR, 3% expected return
    let estimated = 100 * 10u128.pow(24);
    let policy = SlippagePolicy::FromExpectedReturn(ExpectedReturn::new(0.03));
    let result = calculate_min_out(estimated, &policy).unwrap();
    // 10^26 * 9700 / 10000 = 9.7 * 10^25
    let expected_min = estimated * 9700 / 10_000;
    assert_eq!(result, expected_min);
}

#[test]
fn test_large_value_no_overflow() {
    // 10^29 (100,000 NEAR) — should not overflow since 10^29 * 10^4 = 10^33 < u128::MAX
    let estimated = 10u128.pow(29);
    let policy = SlippagePolicy::FromExpectedReturn(ExpectedReturn::new(0.05));
    let result = calculate_min_out(estimated, &policy).unwrap();
    assert_eq!(result, estimated * 9500 / 10_000);
}

#[test]
fn test_extreme_value_overflow_returns_error() {
    // u128::MAX should overflow when multiplied by protection_bps
    let policy = SlippagePolicy::FromExpectedReturn(ExpectedReturn::new(0.05));
    let result = calculate_min_out(u128::MAX, &policy);
    assert!(result.is_err());
}

#[test]
fn test_display_from_expected_return() {
    let policy = SlippagePolicy::FromExpectedReturn(ExpectedReturn::new(0.05));
    assert_eq!(format!("{}", policy), "FromExpectedReturn(0.0500)");
}

#[test]
fn test_display_unprotected() {
    assert_eq!(format!("{}", SlippagePolicy::Unprotected), "Unprotected");
}

#[test]
fn test_exact_min_boundary() {
    // Exactly at MIN_SLIPPAGE_BUDGET (0.5%)
    let policy = SlippagePolicy::FromExpectedReturn(ExpectedReturn::new(0.005));
    let result = calculate_min_out(10_000, &policy).unwrap();
    assert_eq!(result, 9950);
}

#[test]
fn test_exact_max_boundary() {
    // Exactly at MAX_SLIPPAGE_BUDGET (15%)
    let policy = SlippagePolicy::FromExpectedReturn(ExpectedReturn::new(0.15));
    let result = calculate_min_out(10_000, &policy).unwrap();
    assert_eq!(result, 8500);
}

#[test]
fn test_bps_truncation() {
    // 10_001 * 9500 / 10_000 = 9500.95 → truncated to 9500
    let policy = SlippagePolicy::FromExpectedReturn(ExpectedReturn::new(0.05));
    let result = calculate_min_out(10_001, &policy).unwrap();
    assert_eq!(result, 9500); // integer truncation
}
