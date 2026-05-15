use super::*;
use std::str::FromStr;

fn tok(s: &str) -> TokenOutAccount {
    TokenOutAccount::from_str(s).expect("valid account id")
}

#[test]
fn uniform_sets_upper_to_max_position() {
    let bounds = BoxBounds::uniform(3, 0.6);
    assert_eq!(bounds.len(), 3);
    for i in 0..3 {
        assert_eq!(bounds.lower(i), 0.0);
        assert_eq!(bounds.upper(i), 0.6);
    }
}

#[test]
fn uniform_validates_when_feasible() {
    let bounds = BoxBounds::uniform(3, 0.6);
    assert!(bounds.validate().is_ok());
}

#[test]
fn uniform_rejects_max_position_too_small() {
    let bounds = BoxBounds::uniform(3, 0.1);
    assert!(matches!(
        bounds.validate(),
        Err(BoxBoundsError::UpperInfeasible { .. })
    ));
}

#[test]
fn with_held_sell_only_clamps_upper_to_current_weight() {
    let tokens = vec![tok("a.near"), tok("b.near"), tok("c.near")];
    let current_weights = vec![0.3, 0.0, 0.0];
    let held: BTreeSet<_> = [tok("a.near")].into_iter().collect();
    let bounds =
        BoxBounds::with_held_sell_only(&tokens, &current_weights, &held, 0.6, 0.0).unwrap();
    assert_eq!(bounds.upper(0), 0.3);
    assert_eq!(bounds.upper(1), 0.6);
    assert_eq!(bounds.upper(2), 0.6);
    for i in 0..3 {
        assert_eq!(bounds.lower(i), 0.0);
    }
}

#[test]
fn with_held_above_max_position_keeps_current_weight() {
    let tokens = vec![tok("a.near"), tok("b.near")];
    let current_weights = vec![0.7, 0.0];
    let held: BTreeSet<_> = [tok("a.near")].into_iter().collect();
    let bounds =
        BoxBounds::with_held_sell_only(&tokens, &current_weights, &held, 0.6, 0.0).unwrap();
    assert_eq!(bounds.upper(0), 0.7);
    assert_eq!(bounds.upper(1), 0.6);
}

#[test]
fn with_held_applies_sell_only_epsilon() {
    let tokens = vec![tok("a.near"), tok("b.near"), tok("c.near")];
    let current_weights = vec![0.3, 0.0, 0.0];
    let held: BTreeSet<_> = [tok("a.near")].into_iter().collect();
    let bounds =
        BoxBounds::with_held_sell_only(&tokens, &current_weights, &held, 0.6, 1e-6).unwrap();
    assert!((bounds.upper(0) - 0.300_001).abs() < 1e-12);
    assert_eq!(bounds.upper(1), 0.6);
    assert_eq!(bounds.upper(2), 0.6);
}

#[test]
fn with_held_clamps_epsilon_to_one() {
    let tokens = vec![tok("a.near"), tok("b.near")];
    let current_weights = vec![1.0, 0.0];
    let held: BTreeSet<_> = [tok("a.near")].into_iter().collect();
    let bounds =
        BoxBounds::with_held_sell_only(&tokens, &current_weights, &held, 0.6, 1e-6).unwrap();
    assert_eq!(bounds.upper(0), 1.0);
}

#[test]
fn with_held_rejects_length_mismatch() {
    let tokens = vec![tok("a.near"), tok("b.near")];
    let current_weights = vec![0.3];
    let held: BTreeSet<_> = BTreeSet::new();
    let result = BoxBounds::with_held_sell_only(&tokens, &current_weights, &held, 0.6, 0.0);
    assert!(matches!(
        result,
        Err(BoxBoundsError::LengthMismatch {
            tokens: 2,
            current_weights: 1
        })
    ));
}

#[test]
fn validate_rejects_infeasible_sum_upper() {
    let tokens = vec![tok("a.near"), tok("b.near")];
    let current_weights = vec![0.1, 0.1];
    let held: BTreeSet<_> = [tok("a.near"), tok("b.near")].into_iter().collect();
    let result = BoxBounds::with_held_sell_only(&tokens, &current_weights, &held, 0.6, 0.0);
    assert!(matches!(
        result,
        Err(BoxBoundsError::UpperInfeasible { .. })
    ));
}

#[test]
fn validate_rejects_inverted_bounds() {
    let bounds = BoxBounds {
        lower: vec![0.5],
        upper: vec![0.3],
        aggregate_cap: BoxBoundsCap::Equality,
    };
    assert!(matches!(
        bounds.validate(),
        Err(BoxBoundsError::Inverted(0, _, _))
    ));
}

#[test]
fn validate_rejects_negative_bounds() {
    let bounds = BoxBounds {
        lower: vec![0.0, -0.1],
        upper: vec![0.6, 0.5],
        aggregate_cap: BoxBoundsCap::Equality,
    };
    assert!(matches!(
        bounds.validate(),
        Err(BoxBoundsError::Negative(1))
    ));
}

#[test]
fn validate_rejects_non_finite_bounds() {
    let bounds = BoxBounds {
        lower: vec![0.0],
        upper: vec![f64::NAN],
        aggregate_cap: BoxBoundsCap::Equality,
    };
    assert!(matches!(
        bounds.validate(),
        Err(BoxBoundsError::NonFinite(0))
    ));
}

#[test]
fn validate_accepts_sum_upper_exactly_one() {
    let bounds = BoxBounds {
        lower: vec![0.0, 0.0],
        upper: vec![0.5, 0.5],
        aggregate_cap: BoxBoundsCap::Equality,
    };
    assert!(bounds.validate().is_ok());
}

#[test]
fn validate_rejects_sum_lower_above_one() {
    let bounds = BoxBounds {
        lower: vec![0.6, 0.6],
        upper: vec![0.8, 0.8],
        aggregate_cap: BoxBoundsCap::Equality,
    };
    assert!(matches!(
        bounds.validate(),
        Err(BoxBoundsError::LowerInfeasible { .. })
    ));
}

#[test]
fn lower_upper_slice_match_accessors() {
    let bounds = BoxBounds::uniform(3, 0.6);
    assert_eq!(bounds.lower_slice(), &[0.0, 0.0, 0.0]);
    assert_eq!(bounds.upper_slice(), &[0.6, 0.6, 0.6]);
}

#[test]
fn empty_bounds() {
    let bounds = BoxBounds::uniform(0, 0.6);
    assert!(bounds.is_empty());
    assert_eq!(bounds.len(), 0);
}

#[test]
fn effective_uppers_returns_uppers_when_sum_ge_one() {
    let bounds = BoxBounds::uniform(3, 0.6);
    let eff = bounds.effective_uppers();
    assert_eq!(eff, vec![0.6, 0.6, 0.6]);
}

#[test]
fn effective_uppers_matches_legacy_uniform_max_position() {
    // Uniform feasible bounds (n*m >= 1.0): legacy used max_position directly.
    let n = 5;
    let max_position = 0.4;
    let bounds = BoxBounds::uniform(n, max_position);
    let eff = bounds.effective_uppers();
    let legacy = vec![max_position; n];
    assert_eq!(eff.len(), legacy.len());
    for (e, l) in eff.iter().zip(legacy.iter()) {
        assert!((e - l).abs() < 1e-15);
    }
}

#[test]
fn effective_uppers_scales_to_one_over_n_when_sum_lt_one() {
    // Uniform infeasible bounds (n*m < 1.0): legacy fell back to 1/n.
    let n = 5;
    let max_position = 0.1; // 5 * 0.1 = 0.5 < 1.0
    let bounds = BoxBounds::uniform(n, max_position);
    let eff = bounds.effective_uppers();
    let expected = 1.0 / n as f64;
    for e in eff.iter() {
        assert!(
            (e - expected).abs() < 1e-15,
            "got {}, expected {}",
            e,
            expected
        );
    }
}

#[test]
fn subset_extracts_indexed_bounds() {
    let bounds = BoxBounds::from_uppers(vec![0.2, 0.3, 0.4, 0.5]);
    let sub = bounds.subset(&[0, 2]);
    assert_eq!(sub.len(), 2);
    assert_eq!(sub.upper(0), 0.2);
    assert_eq!(sub.upper(1), 0.4);
    assert_eq!(sub.lower(0), 0.0);
    assert_eq!(sub.lower(1), 0.0);
}

#[test]
fn subset_can_reorder() {
    let bounds = BoxBounds::from_uppers(vec![0.1, 0.2, 0.3]);
    let sub = bounds.subset(&[2, 0]);
    assert_eq!(sub.upper(0), 0.3);
    assert_eq!(sub.upper(1), 0.1);
}

#[test]
fn effective_uppers_non_uniform_scaling_sums_to_one() {
    let bounds = BoxBounds {
        lower: vec![0.0; 3],
        upper: vec![0.2, 0.3, 0.4], // sum = 0.9 < 1.0
        aggregate_cap: BoxBoundsCap::Equality,
    };
    let eff = bounds.effective_uppers();
    let sum: f64 = eff.iter().sum();
    assert!((sum - 1.0).abs() < 1e-15);
    // Proportional scaling: each scaled by 1/0.9
    assert!((eff[0] - 0.2 / 0.9).abs() < 1e-15);
    assert!((eff[1] - 0.3 / 0.9).abs() < 1e-15);
    assert!((eff[2] - 0.4 / 0.9).abs() < 1e-15);
}
