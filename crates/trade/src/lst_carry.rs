//! Tier-1 liquid-staking carry strategy.
//!
//! This module implements a low-turnover buy-and-hold over a fixed universe of
//! liquid-staking tokens (LiNEAR, stNEAR) to capture their structural ~4 %/yr
//! appreciation against NEAR. It deliberately bypasses the volatility-portfolio
//! pipeline (prediction, CoV ranking, alpha gate, Markowitz optimizer): those
//! stages are tuned for high-volatility candidates and would reject the LSTs'
//! tiny per-cycle expected return outright.
//!
//! The functions here that compute target weights are kept pure (no `C`/`W`
//! generics, no I/O) so they are trivial to unit-test; the strategy layer is
//! responsible for turning the weights into swaps via the existing execution
//! path.

use bigdecimal::BigDecimal;
use common::types::TokenOutAccount;
use std::collections::BTreeMap;
use std::sync::LazyLock;

/// Fixed liquid-staking token universe for the carry strategy.
///
/// These are mainnet account IDs. NearX (`v2-nearx.stader-labs.near`) is
/// intentionally excluded: its REF pool rate is frozen/stale and cannot be
/// traded against reliably. The IDs are compile-time constants, so the parse
/// cannot fail at runtime — `expect` documents the invariant rather than
/// guarding a real error path.
// NOTE: `CARRY_UNIVERSE` / `equal_weight_targets` are wired into the strategy
// in a later commit (strategy mode branch). The `dead_code` allowance is
// temporary scaffolding and is removed once the wiring lands.
#[allow(dead_code)]
pub(crate) static CARRY_UNIVERSE: LazyLock<[TokenOutAccount; 2]> = LazyLock::new(|| {
    [
        "linear-protocol.near"
            .parse()
            .expect("valid hardcoded LiNEAR account id"),
        "meta-pool.near"
            .parse()
            .expect("valid hardcoded stNEAR account id"),
    ]
});

/// Compute equal target weights over the given LST universe.
///
/// Each token receives `1/N` of the portfolio. Returns an empty map for an
/// empty universe (the strategy treats that as "hold cash" rather than
/// dividing by zero). The weights sum to `1` exactly whenever `N` divides
/// evenly; for `N = 2` (the production universe) this is `0.5` each.
#[allow(dead_code)]
pub(crate) fn equal_weight_targets(
    universe: &[TokenOutAccount],
) -> BTreeMap<TokenOutAccount, BigDecimal> {
    let n = universe.len();
    if n == 0 {
        return BTreeMap::new();
    }
    let weight = BigDecimal::from(1) / BigDecimal::from(n as u64);
    universe
        .iter()
        .map(|token| (token.clone(), weight.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn univ(ids: &[&str]) -> Vec<TokenOutAccount> {
        ids.iter()
            .map(|s| TokenOutAccount::from_str(s).expect("valid test account id"))
            .collect()
    }

    #[test]
    fn carry_universe_is_linear_and_meta_pool() {
        let u = &*CARRY_UNIVERSE;
        assert_eq!(u.len(), 2);
        assert_eq!(u[0].to_string(), "linear-protocol.near");
        assert_eq!(u[1].to_string(), "meta-pool.near");
    }

    #[test]
    fn equal_weights_sum_to_one_and_are_uniform() {
        let u = univ(&["linear-protocol.near", "meta-pool.near"]);
        let w = equal_weight_targets(&u);
        assert_eq!(w.len(), 2);
        let half = BigDecimal::from(1) / BigDecimal::from(2);
        for token in &u {
            assert_eq!(w.get(token), Some(&half));
        }
        let sum: BigDecimal = w.values().cloned().sum();
        assert_eq!(sum, BigDecimal::from(1));
    }

    #[test]
    fn equal_weights_single_token_is_full_allocation() {
        let u = univ(&["linear-protocol.near"]);
        let w = equal_weight_targets(&u);
        assert_eq!(w.len(), 1);
        assert_eq!(w.values().next(), Some(&BigDecimal::from(1)));
    }

    #[test]
    fn equal_weights_empty_universe_is_empty() {
        assert!(equal_weight_targets(&[]).is_empty());
    }
}
