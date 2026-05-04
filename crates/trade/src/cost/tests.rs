use super::*;
use std::str::FromStr;

const ONE_NEAR_YOCTO: u128 = 1_000_000_000_000_000_000_000_000;

#[test]
fn test_to_return_deduction_zero_position_returns_infinity() {
    let breakdown = TradeCostBreakdown {
        variable_ratio: 0.005,
        fixed_cost: YoctoValue::from_yocto_u128(1_000_000_000_000_000_000_000),
    };
    let zero = YoctoValue::zero();
    assert_eq!(breakdown.to_return_deduction(&zero), f64::INFINITY);
}

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
fn test_to_return_deduction_combines_variable_and_fixed() {
    let breakdown = TradeCostBreakdown {
        variable_ratio: 0.01,
        // 固定費 0.001 NEAR
        fixed_cost: YoctoValue::from_yocto_u128(1_000_000_000_000_000_000_000),
    };
    // assumed = 1 NEAR → fixed_ratio = 0.001 / 1.0 = 0.001
    let assumed = YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO);
    let deduction = breakdown.to_return_deduction(&assumed);
    assert!(
        (deduction - 0.011).abs() < 1e-6,
        "expected 0.011, got {deduction}"
    );
}

#[test]
fn test_to_return_deduction_larger_position_reduces_fixed_ratio() {
    let breakdown = TradeCostBreakdown {
        variable_ratio: 0.005,
        fixed_cost: YoctoValue::from_yocto_u128(1_000_000_000_000_000_000_000),
    };
    let small = YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO);
    let large = YoctoValue::from_yocto_u128(100 * ONE_NEAR_YOCTO);
    let small_d = breakdown.to_return_deduction(&small);
    let large_d = breakdown.to_return_deduction(&large);
    assert!(
        large_d < small_d,
        "larger position should yield smaller deduction"
    );
}

#[test]
fn test_to_return_deduction_only_variable_when_fixed_zero() {
    let breakdown = TradeCostBreakdown {
        variable_ratio: 0.01,
        fixed_cost: YoctoValue::zero(),
    };
    let assumed = YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO);
    let deduction = breakdown.to_return_deduction(&assumed);
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
