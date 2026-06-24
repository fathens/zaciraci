use super::*;

/// A healthy pool: full-size effective rate equals the marginal rate, so the
/// impact is zero.
#[test]
fn no_impact_when_rates_match() {
    // full 1000 in → 2000 out (rate 2.0); ref 1 in → 2 out (rate 2.0).
    let impact = price_impact_ratio(1000, 2000, 1, 2);
    assert!(impact.abs() < 1e-9, "expected ~0, got {impact}");
}

/// A thin pool: the full trade gets a far worse effective rate than the
/// marginal probe.
#[test]
fn detects_partial_impact() {
    // marginal rate 2.0 (1 → 2); full size gets only 1.0 (1000 → 1000).
    let impact = price_impact_ratio(1000, 1000, 1, 2);
    assert!((impact - 0.5).abs() < 1e-9, "expected 0.5, got {impact}");
}

/// The catastrophic dead-pool case: the reference still yields output but the
/// real size collapses to zero. Impact must be total so the guard blocks it.
#[test]
fn total_collapse_when_full_output_zero() {
    let impact = price_impact_ratio(1_000_000, 0, 1000, 5);
    assert!((impact - 1.0).abs() < 1e-9, "expected 1.0, got {impact}");
}

/// Fail-open: a degenerate reference (zero output) cannot be assessed, so the
/// guard must report no impact rather than block.
#[test]
fn fail_open_on_zero_reference_output() {
    assert_eq!(price_impact_ratio(1000, 500, 1, 0), 0.0);
}

/// Fail-open on zero input sizes (nothing to assess).
#[test]
fn fail_open_on_zero_inputs() {
    assert_eq!(price_impact_ratio(0, 0, 1, 2), 0.0);
    assert_eq!(price_impact_ratio(1000, 2000, 0, 0), 0.0);
}

/// Result is always bounded to `[0, 1]` even when the full rate exceeds the
/// marginal rate (which would yield a negative raw ratio).
#[test]
fn clamped_to_unit_interval() {
    // full rate (2.0) > marginal rate (1.0) → raw ratio negative → clamp 0.
    let impact = price_impact_ratio(1000, 2000, 1, 1);
    assert_eq!(impact, 0.0);
}

/// `reference_input` never returns zero, even for tiny trades.
#[test]
fn reference_input_is_never_zero() {
    assert_eq!(reference_input(1_000_000), 1000);
    assert_eq!(reference_input(500), 1); // 500/1000 = 0 → floored to 1
    assert_eq!(reference_input(1), 1);
    assert_eq!(reference_input(0), 1);
}

/// The observed mpdao-style dead-pool route (~97 % impact) is flagged well
/// above the 50 % default threshold.
#[test]
fn flags_observed_dead_pool_route() {
    // marginal: 1000 in → 2000 out (rate 2.0).
    // full: 1_000_000 in → 60_000 out (rate 0.06) ≈ 97 % impact.
    let impact = price_impact_ratio(1_000_000, 60_000, 1000, 2000);
    assert!(impact > 0.95, "expected >0.95, got {impact}");
    assert!(impact > 0.5, "must exceed default threshold");
}
