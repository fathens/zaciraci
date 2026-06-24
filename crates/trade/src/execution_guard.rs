//! Execution-quality guard for individual swaps.
//!
//! This guard is orthogonal to [`crate::slippage::SlippagePolicy`]:
//!
//! - `SlippagePolicy` / `min_out` answers "how much *additional* slippage
//!   beyond the estimated output do we tolerate before the on-chain swap
//!   reverts" — and it is `0` for `Unprotected` (liquidation) swaps.
//! - This guard answers "should we route through this pool *at all*", by
//!   measuring how badly the route's effective rate at the real trade size
//!   degrades versus its marginal rate at a tiny reference size.
//!
//! `min_out` cannot catch a thin/dead pool because the estimated output it
//! protects is *already* the bad, depth-impacted value; the guard compares
//! against the marginal rate instead, so it catches routes that convert most
//! of the input into slippage (real cycles up to 97 % impact were observed
//! against dead pools).

/// Fraction of the full trade size used as the marginal-rate reference.
///
/// Small enough that its own depth impact is negligible on a healthy pool,
/// large enough to stay clear of integer-truncation underflow through a
/// multi-hop [`dex::TokenPath::calc_value`]. `1/1000` of the trade.
pub const REFERENCE_FRACTION: u128 = 1000;

/// AMM price-impact ratio of a swap versus its marginal (small-size) rate.
///
/// Both legs are produced by the same path/fee structure, so the constant
/// proportional AMM fee cancels in the ratio and the result isolates pure
/// depth impact (the fee is accounted for elsewhere).
///
/// Returns a value in `[0.0, 1.0]`:
/// - `0.0` = no measurable impact **or** the impact cannot be assessed
///   (degenerate reference). Returning `0.0` in the unknown case is
///   deliberately fail-open: the guard must never block a swap it cannot
///   evaluate, only ones it can prove are catastrophic.
/// - `1.0` = total collapse (`full_out == 0` while the reference still
///   produced output, i.e. the pool is effectively empty at the real size).
///
/// `full_in` / `ref_in` are input amounts, `full_out` / `ref_out` the
/// corresponding outputs from [`dex::TokenPath::calc_value`].
pub fn price_impact_ratio(full_in: u128, full_out: u128, ref_in: u128, ref_out: u128) -> f64 {
    // Cannot size the trade or the reference → cannot assess; fail-open.
    if full_in == 0 || ref_in == 0 {
        return 0.0;
    }
    // No marginal rate available (reference truncated to zero) → fail-open.
    if ref_out == 0 {
        return 0.0;
    }
    // Reference produced output but the full size yields nothing: the depth
    // impact is total. This is the catastrophic case the guard exists for.
    if full_out == 0 {
        return 1.0;
    }
    let exec_rate = full_out as f64 / full_in as f64;
    let marginal_rate = ref_out as f64 / ref_in as f64;
    if !marginal_rate.is_finite() || marginal_rate <= 0.0 {
        return 0.0;
    }
    let ratio = 1.0 - exec_rate / marginal_rate;
    if !ratio.is_finite() {
        return 0.0;
    }
    ratio.clamp(0.0, 1.0)
}

/// Reference input size for the marginal-rate probe of `full_in`.
///
/// `max(1, full_in / REFERENCE_FRACTION)` so the probe is always non-zero
/// (the `calc_value` of `0` is `0` and would force a fail-open).
pub fn reference_input(full_in: u128) -> u128 {
    (full_in / REFERENCE_FRACTION).max(1)
}

#[cfg(test)]
mod tests;
