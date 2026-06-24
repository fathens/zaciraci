//! Volatility-targeting risk-budget signal.
//!
//! Returns an aggregate-cap value `cap ∈ [AGGREGATE_CAP_LOWER, 1.0]` such that
//! `cap = σ_target / σ_portfolio` (Moreira & Muir, 2017). When the portfolio
//! is calmer than the target the cap saturates at 1.0 (no de-risk); when it
//! is more volatile the cap shrinks proportionally, with the lower bound
//! preventing pathological full-cash collapse on a noisy estimate.
//!
//! This module is intentionally pure (no I/O, no `cfg` reads) so the maths can
//! be exercised by unit / property tests without async fixtures. The async
//! adapter that fetches σ_portfolio from current weights + covariance and
//! threads the resulting cap into `BoxBounds::with_aggregate_cap` lives in
//! `trade::regime`.
//!
//! Defensive contracts:
//! - Non-finite inputs (`NaN`, `±∞`) collapse to `cap = 1.0` (no de-risk),
//!   so an injected NaN cannot silently turn off the optimizer.
//! - `σ_portfolio` is floored at `SIGMA_FLOOR` to bound the worst-case
//!   division magnification when the portfolio is artificially calm
//!   (`σ_portfolio` close to zero would otherwise push `cap` to `+∞`,
//!   which the upper clamp would then re-cap at 1.0 anyway, but flooring
//!   keeps the intermediate value finite and trace-friendly).

/// Implementation-only minimum allowed cap. Below this value the
/// vol-targeting signal effectively forces full-cash, which the dedicated
/// kill-switch `BoxBoundsCap::Equality` with all uppers at 0 already
/// expresses; the floor lets us treat 0.1 as "10 % risk on" rather than
/// "broken signal".
const AGGREGATE_CAP_LOWER: f64 = 0.1;

/// Implementation-only maximum allowed cap. `cap > 1.0` would silently let
/// the optimizer over-allocate; clamp at 1.0 so the legacy `sum(w) = 1`
/// behaviour is the maximum risk position even under a wildly calm market.
const AGGREGATE_CAP_UPPER: f64 = 1.0;

/// Floor applied to `σ_portfolio` before the division. Anything calmer than
/// this is treated as "essentially zero portfolio volatility", at which
/// point any `σ_target > floor` already saturates the upper clamp.
const SIGMA_FLOOR: f64 = 1e-10;

/// Map a vol-targeting pair `(σ_target, σ_portfolio)` to an aggregate cap
/// suitable for `BoxBounds::with_aggregate_cap`.
///
/// Formula: `cap = σ_target / max(σ_portfolio, SIGMA_FLOOR)`, then clamped
/// to `[AGGREGATE_CAP_LOWER, AGGREGATE_CAP_UPPER]`.
///
/// Returns `1.0` (no de-risk) for any non-finite input — this matches the
/// "fail-soft to legacy behaviour" policy used elsewhere when a config /
/// runtime value gets poisoned by `NaN`.
pub fn compute_vol_target_cap(sigma_target: f64, sigma_portfolio: f64) -> f64 {
    if !sigma_target.is_finite() || !sigma_portfolio.is_finite() {
        return AGGREGATE_CAP_UPPER;
    }
    if sigma_target <= 0.0 {
        // A non-positive target would invert the optimizer's intent; treat
        // it as "no signal" rather than as a kill switch.
        return AGGREGATE_CAP_UPPER;
    }
    let denom = sigma_portfolio.max(SIGMA_FLOOR);
    let raw = sigma_target / denom;
    raw.clamp(AGGREGATE_CAP_LOWER, AGGREGATE_CAP_UPPER)
}

#[cfg(test)]
mod tests;
