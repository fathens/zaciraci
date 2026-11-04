use super::*;

#[test]
fn cap_at_unit_when_portfolio_calmer_than_target() {
    // σ_target=1.5%/day, σ_portfolio=1.0%/day → raw 1.5, clamped to 1.0
    let cap = compute_vol_target_cap(0.015, 0.010);
    assert_eq!(cap, 1.0);
}

#[test]
fn cap_proportional_when_portfolio_above_target() {
    // σ_target=1.5%/day, σ_portfolio=3.0%/day → raw 0.5
    let cap = compute_vol_target_cap(0.015, 0.030);
    assert!((cap - 0.5).abs() < 1e-12, "got {cap}");
}

#[test]
fn cap_floors_at_lower_bound() {
    // σ_target=1.5%/day, σ_portfolio=100%/day → raw 0.015, clamped up to 0.1
    let cap = compute_vol_target_cap(0.015, 1.0);
    assert_eq!(cap, 0.1);
}

#[test]
fn cap_at_unit_when_target_exactly_matches_portfolio() {
    let cap = compute_vol_target_cap(0.015, 0.015);
    assert!((cap - 1.0).abs() < 1e-12, "got {cap}");
}

#[test]
fn cap_handles_near_zero_portfolio_sigma() {
    // σ_portfolio ≈ 0 → division floored at SIGMA_FLOOR, then clamped to upper
    let cap = compute_vol_target_cap(0.015, 1e-15);
    assert_eq!(cap, 1.0);
}

#[test]
fn cap_returns_unit_for_non_finite_inputs() {
    assert_eq!(compute_vol_target_cap(f64::NAN, 0.020), 1.0);
    assert_eq!(compute_vol_target_cap(0.015, f64::NAN), 1.0);
    assert_eq!(compute_vol_target_cap(f64::INFINITY, 0.020), 1.0);
    assert_eq!(compute_vol_target_cap(0.015, f64::INFINITY), 1.0);
    assert_eq!(compute_vol_target_cap(f64::NEG_INFINITY, 0.020), 1.0);
}

#[test]
fn cap_returns_unit_for_non_positive_target() {
    assert_eq!(compute_vol_target_cap(0.0, 0.020), 1.0);
    assert_eq!(compute_vol_target_cap(-0.01, 0.020), 1.0);
}

#[test]
fn cap_always_in_unit_interval() {
    for &sigma_target in &[0.001, 0.005, 0.015, 0.05, 0.1] {
        for &sigma_portfolio in &[0.001, 0.005, 0.01, 0.03, 0.1, 1.0] {
            let cap = compute_vol_target_cap(sigma_target, sigma_portfolio);
            assert!(
                (AGGREGATE_CAP_LOWER..=AGGREGATE_CAP_UPPER).contains(&cap),
                "σ_t={sigma_target} σ_p={sigma_portfolio} → cap={cap} out of bounds"
            );
        }
    }
}

#[test]
fn cap_monotonically_decreases_with_portfolio_sigma() {
    // For a fixed target, increasing σ_portfolio must not increase cap
    // (until it saturates at the lower bound).
    let target = 0.015;
    let sigmas = [0.005, 0.01, 0.02, 0.05, 0.1];
    let caps: Vec<f64> = sigmas
        .iter()
        .map(|&s| compute_vol_target_cap(target, s))
        .collect();
    for w in caps.windows(2) {
        assert!(
            w[0] >= w[1] - 1e-12,
            "cap should be monotonically decreasing: {} → {}",
            w[0],
            w[1]
        );
    }
}
