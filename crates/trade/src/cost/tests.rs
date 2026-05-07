use super::*;
use std::str::FromStr;

const ONE_NEAR_YOCTO: u128 = 1_000_000_000_000_000_000_000_000;

#[test]
fn test_to_cost_deduction_zero_position_returns_zero_position_error() {
    let breakdown = TradeCostBreakdown {
        variable_ratio: 0.005,
        fixed_cost: YoctoValue::from_yocto_u128(1_000_000_000_000_000_000_000),
    };
    let zero = YoctoValue::zero();
    assert_eq!(
        breakdown.to_cost_deduction(&zero),
        Err(CostError::ZeroPosition)
    );
}

#[test]
fn test_to_cost_deduction_finite_value_invariant() {
    let breakdown = TradeCostBreakdown {
        variable_ratio: 0.01,
        fixed_cost: YoctoValue::from_yocto_u128(1_000_000_000_000_000_000_000),
    };
    let assumed = YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO);
    let deduction = breakdown
        .to_cost_deduction(&assumed)
        .expect("zero position is the only failure path here");
    let value = deduction.as_f64();
    assert!(value.is_finite() && value >= 0.0);
    assert!((value - 0.011).abs() < 1e-6, "expected 0.011, got {value}");
}

#[test]
fn test_to_cost_deduction_rejects_non_finite_variable_ratio() {
    let breakdown = TradeCostBreakdown {
        variable_ratio: f64::NAN,
        fixed_cost: YoctoValue::from_yocto_u128(1_000_000_000_000_000_000_000),
    };
    let assumed = YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO);
    assert_eq!(
        breakdown.to_cost_deduction(&assumed),
        Err(CostError::NonFiniteRatio)
    );
}

#[test]
fn test_cost_deduction_rejects_non_finite() {
    assert!(CostDeduction::new(f64::NAN).is_none());
    assert!(CostDeduction::new(f64::INFINITY).is_none());
    assert!(CostDeduction::new(f64::NEG_INFINITY).is_none());
}

#[test]
fn test_cost_deduction_rejects_negative() {
    assert!(CostDeduction::new(-0.001).is_none());
}

#[test]
fn test_cost_deduction_accepts_zero_and_positive() {
    assert_eq!(
        CostDeduction::new(0.0).map(CostDeduction::as_f64),
        Some(0.0)
    );
    assert_eq!(
        CostDeduction::new(0.5).map(CostDeduction::as_f64),
        Some(0.5)
    );
}

#[test]
fn test_to_cost_deduction_combines_variable_and_fixed() {
    let breakdown = TradeCostBreakdown {
        variable_ratio: 0.01,
        // 固定費 0.001 NEAR
        fixed_cost: YoctoValue::from_yocto_u128(1_000_000_000_000_000_000_000),
    };
    // assumed = 1 NEAR → fixed_ratio = 0.001 / 1.0 = 0.001
    let assumed = YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO);
    let deduction = breakdown
        .to_cost_deduction(&assumed)
        .expect("zero position is the only failure path here")
        .as_f64();
    assert!(
        (deduction - 0.011).abs() < 1e-6,
        "expected 0.011, got {deduction}"
    );
}

#[test]
fn test_to_cost_deduction_larger_position_reduces_fixed_ratio() {
    let breakdown = TradeCostBreakdown {
        variable_ratio: 0.005,
        fixed_cost: YoctoValue::from_yocto_u128(1_000_000_000_000_000_000_000),
    };
    let small = YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO);
    let large = YoctoValue::from_yocto_u128(100 * ONE_NEAR_YOCTO);
    let small_d = breakdown
        .to_cost_deduction(&small)
        .expect("non-zero position")
        .as_f64();
    let large_d = breakdown
        .to_cost_deduction(&large)
        .expect("non-zero position")
        .as_f64();
    assert!(
        large_d < small_d,
        "larger position should yield smaller deduction"
    );
}

#[test]
fn test_to_cost_deduction_only_variable_when_fixed_zero() {
    let breakdown = TradeCostBreakdown {
        variable_ratio: 0.01,
        fixed_cost: YoctoValue::zero(),
    };
    let assumed = YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO);
    let deduction = breakdown
        .to_cost_deduction(&assumed)
        .expect("non-zero position")
        .as_f64();
    assert!(
        (deduction - 0.01).abs() < 1e-9,
        "expected 0.01, got {deduction}"
    );
}

#[test]
fn test_compute_loss_ratio_basic() {
    let input = NearValue::from_near(BigDecimal::from_str("1.0").unwrap());
    let output = NearValue::from_near(BigDecimal::from_str("0.99").unwrap());
    let loss = compute_loss_ratio(&input, &output);
    assert!((loss - 0.01).abs() < 1e-9, "expected ~0.01, got {loss}");
}

#[test]
fn test_compute_loss_ratio_zero_input_returns_zero() {
    let input = NearValue::zero();
    let output = NearValue::from_near(BigDecimal::from_str("1.0").unwrap());
    assert_eq!(compute_loss_ratio(&input, &output), 0.0);
}

#[test]
fn test_compute_loss_ratio_clamps_negative_to_zero() {
    // 数値誤差で output > input になっても 0 にクランプ
    let input = NearValue::from_near(BigDecimal::from_str("1.0").unwrap());
    let output = NearValue::from_near(BigDecimal::from_str("1.001").unwrap());
    assert_eq!(compute_loss_ratio(&input, &output), 0.0);
}

#[test]
fn test_compute_loss_ratio_full_loss() {
    // output = 0 → loss = 100%
    let input = NearValue::from_near(BigDecimal::from_str("1.0").unwrap());
    let output = NearValue::zero();
    assert!((compute_loss_ratio(&input, &output) - 1.0).abs() < 1e-12);
}

#[test]
fn test_clamp_storage_min_under_cap_passthrough() {
    // 通常の運用値（0.1 NEAR）はクランプされずそのまま返る
    let normal = YoctoValue::from_yocto_u128(100_000_000_000_000_000_000_000);
    let clamped = clamp_storage_min(&normal);
    assert_eq!(clamped, 100_000_000_000_000_000_000_000);
    assert!(clamped < STORAGE_MIN_SANE_CAP);
}

#[test]
fn test_clamp_storage_min_at_cap_returns_cap() {
    let at_cap = YoctoValue::from_yocto_u128(STORAGE_MIN_SANE_CAP);
    assert_eq!(clamp_storage_min(&at_cap), STORAGE_MIN_SANE_CAP);
}

#[test]
fn test_clamp_storage_min_above_cap_clamped() {
    // 100 NEAR — cap (10 NEAR) を超える
    let above = YoctoValue::from_yocto_u128(100 * 10u128.pow(24));
    assert_eq!(clamp_storage_min(&above), STORAGE_MIN_SANE_CAP);
}

#[test]
fn test_clamp_storage_min_u128_max_clamped_to_cap() {
    // RPC が u128::MAX を返す敵対的シナリオでも cap で吸収される
    let hostile = YoctoValue::from_yocto_u128(u128::MAX);
    assert_eq!(clamp_storage_min(&hostile), STORAGE_MIN_SANE_CAP);
}

#[test]
fn test_clamp_storage_min_overflow_u128_clamped_to_cap() {
    // BigDecimal が u128 に収まらない値（u128::MAX + 1）でも cap にクランプ
    let huge = YoctoValue::from_yocto(BigDecimal::from(u128::MAX) + BigDecimal::from(1));
    assert_eq!(clamp_storage_min(&huge), STORAGE_MIN_SANE_CAP);
}

#[test]
fn test_clamp_storage_min_saturating_mul_safe_with_max_token_count() {
    // クランプされた値 × 最大 token 数（MAX_NEW_TOKEN_COUNT = 16）が
    // u128 範囲内に収まることを確認（overflow せず saturate もしない）。
    let hostile = YoctoValue::from_yocto_u128(u128::MAX);
    let clamped = clamp_storage_min(&hostile);
    let mul = clamped.checked_mul(MAX_NEW_TOKEN_COUNT as u128);
    assert!(
        mul.is_some(),
        "STORAGE_MIN_SANE_CAP × MAX_NEW_TOKEN_COUNT must not overflow u128"
    );
}
