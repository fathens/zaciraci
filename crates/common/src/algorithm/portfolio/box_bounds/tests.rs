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
    };
    assert!(bounds.validate().is_ok());
}

#[test]
fn validate_rejects_sum_lower_above_one() {
    let bounds = BoxBounds {
        lower: vec![0.6, 0.6],
        upper: vec![0.8, 0.8],
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
