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

use bigdecimal::{BigDecimal, ToPrimitive};
use common::algorithm::types::TradingAction;
use common::types::{ExchangeRate, TokenOutAccount};
use std::collections::BTreeMap;
use std::sync::LazyLock;

/// Expected annual carry return (NEAR-denominated) used to seed per-token
/// expected returns.
///
/// Grounded in the backtest: LiNEAR/stNEAR appreciated ~4 %/yr against NEAR
/// over the measured window. This is a deliberately conservative, strategy
/// internal estimate (not a price prediction) — its only downstream consumer
/// is the slippage policy, where `calculate_min_out` clamps the magnitude to a
/// 0.5 % floor, so the exact value matters little for a sub-1 % per-hold carry.
/// Its purpose is to keep the buy off the `Unprotected` (min_out = 0) path that
/// an empty expected-returns map would trigger.
const EXPECTED_ANNUAL_CARRY: f64 = 0.04;

/// Fixed liquid-staking token universe for the carry strategy.
///
/// These are mainnet account IDs. NearX (`v2-nearx.stader-labs.near`) is
/// intentionally excluded: its REF pool rate is frozen/stale and cannot be
/// traded against reliably. The IDs are compile-time constants, so the parse
/// cannot fail at runtime — `expect` documents the invariant rather than
/// guarding a real error path.
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
fn equal_weight_targets(universe: &[TokenOutAccount]) -> BTreeMap<TokenOutAccount, BigDecimal> {
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

/// Expected carry return realized over a `hold_days` horizon.
///
/// `ER = EXPECTED_ANNUAL_CARRY × hold_days / 365`. This is the real expected
/// holding-period return, not a placeholder.
fn expected_return_over_hold(hold_days: u32) -> f64 {
    EXPECTED_ANNUAL_CARRY * (hold_days as f64) / 365.0
}

/// Relative deviation of an `observed` rate from a `reference` rate, as a
/// fraction of the reference.
///
/// `observed` and `reference` are the same token sampled at two times, so they
/// share `decimals` and the raw-rate comparison is valid. Returns `None` when
/// the reference is effectively zero (cannot form a ratio).
fn relative_deviation(observed: &ExchangeRate, reference: &ExchangeRate) -> Option<f64> {
    if reference.is_effectively_zero() {
        return None;
    }
    let diff = (observed.raw_rate() - reference.raw_rate()).abs();
    (diff / reference.raw_rate()).to_f64()
}

/// Whether an LST's observed rate has de-pegged beyond `max_depeg` versus its
/// reference.
///
/// Fails closed: if the deviation cannot be assessed (zero/degenerate
/// reference), the token is treated as de-pegged so the carry mode declines to
/// buy into it.
fn is_depegged(observed: &ExchangeRate, reference: &ExchangeRate, max_depeg: f64) -> bool {
    match relative_deviation(observed, reference) {
        Some(dev) => dev > max_depeg,
        None => true,
    }
}

/// Build the `(target_weights, expected_returns)` pair for the carry universe,
/// excluding any token whose observed rate has de-pegged from its reference.
///
/// - `observed`: the current per-token exchange rate (required to trade).
/// - `reference`: the prior-period per-token rate, used as the de-peg
///   baseline. A token absent from `reference` (no prior observation, e.g. the
///   first entry) is accepted — there is no baseline to deviate from, and the
///   execution-layer `min_out` / price-impact guard still protect the swap.
/// - `hold_days`: the carry hold horizon, seeding the expected return.
/// - `max_depeg`: the de-peg tolerance (fraction).
///
/// The returned maps share identical key sets (the surviving tokens), so the
/// expected-returns map is never empty while there is something to buy — this
/// is what keeps the buy off the `Unprotected` slippage path.
pub(crate) fn carry_targets(
    universe: &[TokenOutAccount],
    observed: &BTreeMap<TokenOutAccount, ExchangeRate>,
    reference: &BTreeMap<TokenOutAccount, ExchangeRate>,
    hold_days: u32,
    max_depeg: f64,
) -> (
    BTreeMap<TokenOutAccount, BigDecimal>,
    BTreeMap<TokenOutAccount, f64>,
) {
    let healthy: Vec<TokenOutAccount> = universe
        .iter()
        .filter(|token| match observed.get(token) {
            // No tradable rate → cannot buy this token.
            None => false,
            Some(obs) => match reference.get(token) {
                // No baseline yet → accept (first entry).
                None => true,
                Some(reference_rate) => !is_depegged(obs, reference_rate, max_depeg),
            },
        })
        .cloned()
        .collect();

    let weights = equal_weight_targets(&healthy);
    let er = expected_return_over_hold(hold_days);
    let expected_returns = healthy
        .iter()
        .map(|token| (token.clone(), er))
        .collect::<BTreeMap<_, _>>();
    (weights, expected_returns)
}

/// Entry-confinement gate: the carry mode buys only on the cycle that opens a
/// new evaluation period, and holds (does nothing) on every continuation
/// cycle.
///
/// This is the structural enforcement of the min-hold invariant. Because the
/// evaluation-period length is set to the carry hold horizon elsewhere, the
/// position acquired on the new-period cycle is held untouched until the
/// period machinery liquidates it at the boundary — there is no mid-period
/// top-up or rebalance that would reset the hold clock or incur churn.
///
/// Returns a single `Rebalance` action on the entry cycle (when there is
/// something to buy), and an empty action list otherwise. An empty
/// `target_weights` (e.g. every LST de-pegged out) also yields no action.
pub(crate) fn carry_actions(
    is_new_period: bool,
    target_weights: BTreeMap<TokenOutAccount, BigDecimal>,
) -> Vec<TradingAction> {
    if is_new_period && !target_weights.is_empty() {
        vec![TradingAction::Rebalance { target_weights }]
    } else {
        Vec::new()
    }
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

    fn rate(raw: i64) -> ExchangeRate {
        ExchangeRate::from_raw_rate(BigDecimal::from(raw), 24)
    }

    #[test]
    fn expected_return_scales_with_hold() {
        let er = expected_return_over_hold(365);
        assert!((er - EXPECTED_ANNUAL_CARRY).abs() < 1e-12);
        let half = expected_return_over_hold(30);
        assert!((half - EXPECTED_ANNUAL_CARRY * 30.0 / 365.0).abs() < 1e-12);
        assert!(half > 0.0);
    }

    #[test]
    fn small_drift_is_not_depegged() {
        // 2 % move, tolerance 5 % → healthy.
        assert!(!is_depegged(&rate(1_020), &rate(1_000), 0.05));
    }

    #[test]
    fn large_move_is_depegged() {
        // 20 % move, tolerance 5 % → de-pegged.
        assert!(is_depegged(&rate(1_200), &rate(1_000), 0.05));
    }

    #[test]
    fn zero_reference_fails_closed() {
        // Cannot assess against a zero reference → treat as de-pegged (skip).
        assert!(is_depegged(&rate(1_000), &rate(0), 0.05));
    }

    #[test]
    fn carry_targets_excludes_depegged_and_keeps_er_nonempty() {
        let u = univ(&["linear-protocol.near", "meta-pool.near"]);
        let mut observed = BTreeMap::new();
        observed.insert(u[0].clone(), rate(1_000)); // LiNEAR healthy
        observed.insert(u[1].clone(), rate(1_500)); // stNEAR moved 50 %
        let mut reference = BTreeMap::new();
        reference.insert(u[0].clone(), rate(1_010));
        reference.insert(u[1].clone(), rate(1_000));

        let (weights, ers) = carry_targets(&u, &observed, &reference, 30, 0.05);
        // stNEAR de-pegged out; LiNEAR remains at full weight.
        assert_eq!(weights.len(), 1);
        assert_eq!(weights.get(&u[0]), Some(&BigDecimal::from(1)));
        // expected_returns shares the surviving key set and is non-empty
        // (keeps the buy off the Unprotected path).
        assert_eq!(ers.len(), 1);
        assert!(ers.contains_key(&u[0]));
        assert!(*ers.get(&u[0]).unwrap() > 0.0);
    }

    #[test]
    fn carry_targets_accepts_token_without_reference() {
        // First entry: no prior rate to compare → token accepted.
        let u = univ(&["linear-protocol.near"]);
        let mut observed = BTreeMap::new();
        observed.insert(u[0].clone(), rate(1_000));
        let reference = BTreeMap::new();
        let (weights, ers) = carry_targets(&u, &observed, &reference, 30, 0.05);
        assert_eq!(weights.len(), 1);
        assert_eq!(ers.len(), 1);
    }

    #[test]
    fn carry_targets_excludes_token_without_observed_rate() {
        // No tradable rate → cannot buy → excluded, maps stay empty.
        let u = univ(&["linear-protocol.near"]);
        let observed = BTreeMap::new();
        let reference = BTreeMap::new();
        let (weights, ers) = carry_targets(&u, &observed, &reference, 30, 0.05);
        assert!(weights.is_empty());
        assert!(ers.is_empty());
    }

    fn weights_2() -> BTreeMap<TokenOutAccount, BigDecimal> {
        equal_weight_targets(&univ(&["linear-protocol.near", "meta-pool.near"]))
    }

    #[test]
    fn carry_actions_buys_only_on_new_period() {
        let actions = carry_actions(true, weights_2());
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            TradingAction::Rebalance { target_weights } => assert_eq!(target_weights.len(), 2),
            other => panic!("expected Rebalance, got {other:?}"),
        }
    }

    #[test]
    fn carry_actions_holds_on_continuation_cycle() {
        // Continuation cycle: no action → positions held untouched (no churn).
        assert!(carry_actions(false, weights_2()).is_empty());
    }

    #[test]
    fn carry_actions_empty_weights_yields_no_action() {
        assert!(carry_actions(true, BTreeMap::new()).is_empty());
    }
}
