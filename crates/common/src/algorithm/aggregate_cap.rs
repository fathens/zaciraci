//! Aggregate-cap composition layer.
//!
//! Each enabled regime signal (volatility targeting, market breadth,
//! optionally future ones such as max-drawdown control) emits an aggregate
//! cap value in `[0, 1]`. This module composes them by taking the
//! element-wise minimum — the "any signal can de-risk" semantics — then
//! clamps the result to `[AGGREGATE_CAP_LOWER, AGGREGATE_CAP_UPPER]` so the
//! optimizer never receives a degenerate value.
//!
//! When no signal is present (e.g. every flag is OFF) the composed cap is
//! `1.0`, so the optimizer falls back to the legacy `sum(w) = 1` behaviour.
//! This is the keystone for the orthogonal-flag design: each Phase can be
//! evaluated independently in simulate via its own flag, and combinations
//! emerge automatically without any extra wiring at the optimizer side.

/// Implementation-only minimum composed cap. Below this value the composed
/// signal effectively forces full-cash, which the dedicated kill-switch
/// (`PORTFOLIO_AGGREGATE_CAP=0.0` in the typed config layer) already
/// expresses; the floor lets us treat 0.1 as "10 % risk on" rather than
/// "broken combine".
const AGGREGATE_CAP_LOWER: f64 = 0.1;

/// Implementation-only maximum composed cap. `cap > 1.0` would silently let
/// the optimizer over-allocate; clamp at 1.0 so the legacy `sum(w) = 1`
/// behaviour is the maximum risk position even under a wildly calm market.
const AGGREGATE_CAP_UPPER: f64 = 1.0;

/// One regime signal's contribution to the aggregate cap.
///
/// All variants carry the cap as an `f64 ∈ [0, 1]`; the variant tag
/// preserves provenance for telemetry (which signal won the `min`?). The
/// upstream pure modules (`vol_targeting::compute_vol_target_cap`,
/// `regime::MarketRegime::aggregate_cap`) already clamp to the unit
/// interval, so this enum is the boundary type passed into the composition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AggregateCapSignal {
    Volatility(f64),
    Breadth(f64),
}

impl AggregateCapSignal {
    pub fn cap(&self) -> f64 {
        match self {
            Self::Volatility(c) | Self::Breadth(c) => *c,
        }
    }
}

/// Compose multiple aggregate-cap signals via element-wise minimum.
///
/// - Empty input → `1.0` (no de-risk, legacy `sum(w) = 1` behaviour).
/// - Non-finite contributions are ignored (defensive).
/// - The final value is clamped to
///   `[AGGREGATE_CAP_LOWER, AGGREGATE_CAP_UPPER]` so the optimizer never
///   sees a degenerate cap.
pub fn compose_aggregate_cap(signals: &[AggregateCapSignal]) -> f64 {
    let raw = signals
        .iter()
        .map(|s| s.cap())
        .filter(|c| c.is_finite())
        .fold(AGGREGATE_CAP_UPPER, f64::min);
    raw.clamp(AGGREGATE_CAP_LOWER, AGGREGATE_CAP_UPPER)
}

#[cfg(test)]
mod tests;
