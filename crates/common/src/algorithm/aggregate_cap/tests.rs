use super::*;

#[test]
fn empty_input_returns_unit() {
    assert_eq!(compose_aggregate_cap(&[]), 1.0);
}

#[test]
fn single_signal_passes_through() {
    let cap = compose_aggregate_cap(&[AggregateCapSignal::Volatility(0.6)]);
    assert!((cap - 0.6).abs() < 1e-12);
}

#[test]
fn multiple_signals_take_minimum() {
    let signals = [
        AggregateCapSignal::Volatility(0.7),
        AggregateCapSignal::Breadth(0.5),
    ];
    let cap = compose_aggregate_cap(&signals);
    assert!((cap - 0.5).abs() < 1e-12);
}

#[test]
fn order_does_not_matter() {
    let a = compose_aggregate_cap(&[
        AggregateCapSignal::Volatility(0.3),
        AggregateCapSignal::Breadth(0.7),
    ]);
    let b = compose_aggregate_cap(&[
        AggregateCapSignal::Breadth(0.7),
        AggregateCapSignal::Volatility(0.3),
    ]);
    assert!((a - b).abs() < 1e-12);
}

#[test]
fn caps_clamp_at_upper() {
    // Even if a signal somehow over-shoots to 1.5, the composed result is
    // clamped to AGGREGATE_CAP_UPPER (= 1.0). Upstream signals already
    // clamp to [0, 1], but the composition layer re-asserts the invariant.
    let cap = compose_aggregate_cap(&[AggregateCapSignal::Volatility(1.5)]);
    assert_eq!(cap, AGGREGATE_CAP_UPPER);
}

#[test]
fn caps_clamp_at_lower() {
    // Pathological signal collapsing to 0 must still leave the optimizer
    // with the documented minimum risk budget so a single noisy signal
    // cannot force full-cash.
    let cap = compose_aggregate_cap(&[AggregateCapSignal::Volatility(0.0)]);
    assert_eq!(cap, AGGREGATE_CAP_LOWER);
    let cap = compose_aggregate_cap(&[AggregateCapSignal::Breadth(-0.5)]);
    assert_eq!(cap, AGGREGATE_CAP_LOWER);
}

#[test]
fn non_finite_signals_are_ignored() {
    // NaN / ±∞ cap values defensively skipped; remaining signal wins.
    let cap = compose_aggregate_cap(&[
        AggregateCapSignal::Volatility(f64::NAN),
        AggregateCapSignal::Breadth(0.6),
    ]);
    assert!((cap - 0.6).abs() < 1e-12);

    let cap = compose_aggregate_cap(&[
        AggregateCapSignal::Volatility(f64::INFINITY),
        AggregateCapSignal::Breadth(0.4),
    ]);
    assert!((cap - 0.4).abs() < 1e-12);
}

#[test]
fn all_non_finite_falls_back_to_unit() {
    // Every contribution dropped → no signal → 1.0 (then clamped, no-op).
    let cap = compose_aggregate_cap(&[
        AggregateCapSignal::Volatility(f64::NAN),
        AggregateCapSignal::Breadth(f64::INFINITY),
    ]);
    assert_eq!(cap, AGGREGATE_CAP_UPPER);
}

#[test]
fn signal_cap_accessor() {
    assert_eq!(AggregateCapSignal::Volatility(0.42).cap(), 0.42);
    assert_eq!(AggregateCapSignal::Breadth(0.42).cap(), 0.42);
}

#[test]
fn composed_cap_always_in_clamp_range() {
    let test_inputs = [
        vec![],
        vec![AggregateCapSignal::Volatility(0.5)],
        vec![
            AggregateCapSignal::Volatility(0.05),
            AggregateCapSignal::Breadth(0.9),
        ],
        vec![
            AggregateCapSignal::Volatility(2.0),
            AggregateCapSignal::Breadth(-1.0),
        ],
    ];
    for signals in &test_inputs {
        let cap = compose_aggregate_cap(signals);
        assert!(
            (AGGREGATE_CAP_LOWER..=AGGREGATE_CAP_UPPER).contains(&cap),
            "cap {cap} out of clamp range for {signals:?}"
        );
    }
}
