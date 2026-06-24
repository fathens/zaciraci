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

use crate::algorithm::regime::ExposureScales;
use crate::types::{TokenOutAccount, TokenPrice};
use std::collections::BTreeMap;

/// Per-cycle configuration for the aggregate-cap pipeline.
///
/// Carries the operator-controlled inputs (typed-config flags + runtime
/// snapshots like entry prices) that the optimizer needs to compute and
/// apply the cap. Each field is `Option`-wrapped so the legacy code path
/// (every flag OFF) is the natural `Self::legacy()` constructor and the
/// existing optimizer call sites do not need to know about the new
/// machinery until they choose to.
#[derive(Debug, Clone, Default)]
pub struct AggregateCapStrategy {
    /// Phase 1: σ_target value when vol-targeting is enabled.
    pub vol_target_sigma: Option<f64>,
    /// Phase 2: `(sma_period, exposure_scales)` when breadth regime is enabled.
    pub regime_breadth: Option<(usize, ExposureScales)>,
    /// Phase 3a: Kelly fraction when half-Kelly is enabled.
    pub half_kelly_fraction: Option<f64>,
    /// Phase 3b: `(threshold, entry_prices)` when per-token stop-loss is enabled.
    pub stop_loss: Option<(f64, BTreeMap<TokenOutAccount, TokenPrice>)>,
}

impl AggregateCapStrategy {
    /// Returns the legacy strategy: every signal off, optimizer behaves as
    /// before. The same as `Default::default()` but spelled out for grep.
    pub fn legacy() -> Self {
        Self::default()
    }

    /// `true` iff every signal is off, i.e. the optimizer should apply no
    /// cap-side adjustments and the cycle is indistinguishable from the
    /// pre-PR-A behaviour.
    pub fn is_legacy(&self) -> bool {
        self.vol_target_sigma.is_none()
            && self.regime_breadth.is_none()
            && self.half_kelly_fraction.is_none()
            && self.stop_loss.is_none()
    }
}

#[cfg(test)]
mod tests;
