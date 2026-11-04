//! Per-token stop-loss trigger.
//!
//! Returns `true` when a token's current price has fallen below
//! `entry_price × (1 - threshold)`, i.e. the loss exceeds `threshold` of
//! the entry price. Designed to be applied after the optimizer's normal
//! upper bounds — for tokens that trigger, the caller should force the
//! per-asset upper to 0 (effectively a sell-only constraint), composing
//! naturally with the existing active-set solver.
//!
//! Orthogonal to half-Kelly: half-Kelly caps based on *predicted* return
//! (an estimate that may be wrong); stop-loss caps based on *realised*
//! drawdown (the actual price action). Combining both via per-asset `min`
//! lets the realised data correct for prediction error after the fact.
//!
//! This module is intentionally pure (no I/O, no `cfg` reads, no DB).
//! The caller is responsible for sourcing entry prices from the existing
//! TradeTransaction history.

use crate::types::TokenPrice;
use bigdecimal::{BigDecimal, FromPrimitive};

/// Returns `true` iff `current` has dropped more than `threshold × entry`
/// below `entry`, i.e. `current < entry × (1 - threshold)`.
///
/// Defensive returns of `false`:
/// - `entry.is_zero()` → cannot compute a meaningful drawdown ratio.
/// - `threshold` is non-finite, ≤ 0, or ≥ 1 → operator misconfigured the
///   flag; typed-config clamping should already prevent this, but we
///   re-check at the boundary so an injected NaN cannot silently disable
///   the trigger (a NaN comparison would always be false, masking real
///   drawdowns).
pub fn should_trigger_stop_loss(entry: &TokenPrice, current: &TokenPrice, threshold: f64) -> bool {
    if entry.is_zero() {
        return false;
    }
    if !threshold.is_finite() || threshold <= 0.0 || threshold >= 1.0 {
        return false;
    }
    let one_minus_threshold = match BigDecimal::from_f64(1.0 - threshold) {
        Some(v) => v,
        None => return false,
    };
    let stop_price = entry.as_bigdecimal() * &one_minus_threshold;
    current.as_bigdecimal() < &stop_price
}

#[cfg(test)]
mod tests;
