//! Per-token Kelly fraction signal.
//!
//! For each candidate, the (full) Kelly fraction is `f_i = (μ_i - rf) / σ²_i`.
//! In production we use a *fractional* Kelly (0.25 by default) because Kelly
//! sizing is famously fragile to expected-return estimation error: a 10 %
//! MAPE on `μ_i` translates into ~100 % error on `f_i` since the error gets
//! amplified by the variance denominator.
//!
//! Outputs are per-asset upper bounds in `[0, MAX_POSITION_SIZE]` ready to
//! be applied via `BoxBounds::apply_half_kelly`. Negative-Kelly assets
//! (those with `μ_i ≤ rf`) get an upper of `0`, which is the long-only
//! analogue of "do not enter this position", and folds into the active-set
//! solver as a hard exclusion.
//!
//! This module is intentionally pure (no I/O, no `cfg` reads). The async
//! adapter that fetches `expected_returns` / `diag_vars` from
//! `execute_portfolio_strategy` and threads the resulting uppers into
//! `BoxBounds::apply_half_kelly` lives in `trade::regime`.
//!
//! Defensive contracts:
//! - Non-finite inputs (`NaN`, `±∞`) collapse to `MAX_POSITION_SIZE` (no
//!   Kelly cap), so an injected NaN cannot silently zero out a position.
//! - `var_i ≤ 0` collapses to `MAX_POSITION_SIZE`, mirroring the
//!   "degenerate covariance ⇒ defer to box bound" policy used elsewhere.
//! - `fraction ≤ 0` collapses to `MAX_POSITION_SIZE` for the same reason.

use super::portfolio::MAX_POSITION_SIZE;

/// Map per-asset `(expected_return, diag_variance)` to per-asset upper
/// bounds suitable for `BoxBounds::apply_half_kelly`.
///
/// `fraction` is typically 0.25 (Quarter Kelly) in production; the typed
/// config layer clamps it to `[0.1, 0.5]`.
///
/// Returns a vector of the same length as `expected_returns`. Caller is
/// responsible for ensuring `diag_vars.len() == expected_returns.len()`.
pub fn compute_half_kelly_uppers(
    expected_returns: &[f64],
    diag_vars: &[f64],
    rf: f64,
    fraction: f64,
) -> Vec<f64> {
    debug_assert_eq!(
        expected_returns.len(),
        diag_vars.len(),
        "expected_returns / diag_vars length mismatch"
    );
    if !fraction.is_finite() || fraction <= 0.0 || !rf.is_finite() {
        return vec![MAX_POSITION_SIZE; expected_returns.len()];
    }
    expected_returns
        .iter()
        .zip(diag_vars.iter())
        .map(|(&er, &var)| compute_single_kelly_upper(er, var, rf, fraction))
        .collect()
}

fn compute_single_kelly_upper(er: f64, var: f64, rf: f64, fraction: f64) -> f64 {
    if !er.is_finite() || !var.is_finite() {
        return MAX_POSITION_SIZE;
    }
    if var <= 0.0 {
        return MAX_POSITION_SIZE;
    }
    let raw = (er - rf) / var * fraction;
    raw.clamp(0.0, MAX_POSITION_SIZE)
}

#[cfg(test)]
mod tests;
