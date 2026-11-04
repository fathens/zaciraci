use super::*;

const RF: f64 = 5.479e-5;

#[test]
fn negative_excess_return_yields_zero_upper() {
    // er < rf → f < 0 → clamp to 0 (long-only exclusion)
    let uppers = compute_half_kelly_uppers(&[-0.01, 0.0], &[0.001, 0.001], RF, 0.25);
    assert_eq!(uppers, vec![0.0, 0.0]);
}

#[test]
fn positive_excess_return_yields_positive_upper() {
    // er = 5%, var = 0.001 → f = 0.05 / 0.001 × 0.25 = 12.5 → clamp to 0.6
    let uppers = compute_half_kelly_uppers(&[0.05], &[0.001], RF, 0.25);
    assert!((uppers[0] - 0.6).abs() < 1e-12);
}

#[test]
fn small_excess_return_high_var_yields_small_upper() {
    // er = 0.01, var = 0.5 → f = 0.01 / 0.5 × 0.5 = 0.01
    let uppers = compute_half_kelly_uppers(&[0.01], &[0.5], RF, 0.5);
    let expected = (0.01 - RF) / 0.5 * 0.5;
    assert!((uppers[0] - expected).abs() < 1e-12, "got {}", uppers[0]);
}

#[test]
fn fraction_scales_kelly_proportionally() {
    let er = vec![0.02];
    let var = vec![0.001];
    let half = compute_half_kelly_uppers(&er, &var, RF, 0.5);
    let quarter = compute_half_kelly_uppers(&er, &var, RF, 0.25);
    // both clamp at MAX_POSITION_SIZE for these aggressive numbers
    // so use a milder example
    let er2 = vec![0.001];
    let var2 = vec![0.5];
    let half2 = compute_half_kelly_uppers(&er2, &var2, RF, 0.5);
    let quarter2 = compute_half_kelly_uppers(&er2, &var2, RF, 0.25);
    assert!((half2[0] - 2.0 * quarter2[0]).abs() < 1e-12);
    // sanity: half kelly is always >= quarter kelly when both are uncapped
    assert!(half[0] >= quarter[0]);
}

#[test]
fn zero_or_negative_variance_falls_back_to_max() {
    // degenerate variance → no Kelly cap, defer to box
    let uppers = compute_half_kelly_uppers(&[0.05, 0.05], &[0.0, -0.001], RF, 0.25);
    assert_eq!(uppers, vec![MAX_POSITION_SIZE, MAX_POSITION_SIZE]);
}

#[test]
fn non_finite_inputs_fall_back_to_max() {
    let uppers = compute_half_kelly_uppers(
        &[f64::NAN, f64::INFINITY, 0.05, 0.05],
        &[0.001, 0.001, f64::NAN, f64::NEG_INFINITY],
        RF,
        0.25,
    );
    assert_eq!(uppers, vec![MAX_POSITION_SIZE; 4]);
}

#[test]
fn non_finite_fraction_or_rf_falls_back_to_max() {
    let er = vec![0.05, 0.05];
    let var = vec![0.001, 0.001];
    assert_eq!(
        compute_half_kelly_uppers(&er, &var, RF, f64::NAN),
        vec![MAX_POSITION_SIZE; 2]
    );
    assert_eq!(
        compute_half_kelly_uppers(&er, &var, f64::NAN, 0.25),
        vec![MAX_POSITION_SIZE; 2]
    );
    assert_eq!(
        compute_half_kelly_uppers(&er, &var, RF, 0.0),
        vec![MAX_POSITION_SIZE; 2]
    );
    assert_eq!(
        compute_half_kelly_uppers(&er, &var, RF, -0.1),
        vec![MAX_POSITION_SIZE; 2]
    );
}

#[test]
fn empty_input_yields_empty_output() {
    let uppers = compute_half_kelly_uppers(&[], &[], RF, 0.25);
    assert!(uppers.is_empty());
}

#[test]
fn upper_always_in_unit_interval() {
    let er = vec![-0.1, -0.01, 0.0, 0.001, 0.01, 0.05, 0.5, 1.0];
    let var = vec![0.001, 0.0001, 0.01, 0.5, 1e-6, 0.1, 0.001, 0.001];
    let uppers = compute_half_kelly_uppers(&er, &var, RF, 0.25);
    for u in uppers {
        assert!(
            (0.0..=MAX_POSITION_SIZE).contains(&u),
            "upper out of [0, MAX_POSITION_SIZE]: {u}"
        );
    }
}
