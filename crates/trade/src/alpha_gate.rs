//! Pre-Markowitz "alpha gate" filter.
//!
//! For each candidate token, the strategy estimates the round-trip AMM cost
//! for a worst-case position (`total_value × MAX_POSITION_SIZE`) and excludes
//! tokens whose expected return cannot recoup that cost over the assumed
//! holding period:
//!
//! ```text
//!   hold_cycles × expected_return > multiplier × round_trip_cost
//! ```
//!
//! The gate is the structural answer to the simulation finding that
//! flat-prediction tokens (`expected_return ≈ 0`) get assembled by Markowitz
//! into apparent "high Sharpe" portfolios that immediately bleed value to
//! AMM slippage. Setting `expected_return < k × cost` tokens aside before
//! the optimizer ensures the optimizer only sees tokens where the alpha is
//! large enough to overcome execution costs.
//!
//! Held tokens always bypass the gate so existing positions can be
//! liquidated regardless of forward alpha. When the gate rejects too many
//! tokens (fewer than `min_pass_count` survive) the strategy falls back to
//! the highest-`expected_return` rejected tokens to avoid collapsing into a
//! single-token corner solution.
//!
//! This module is intentionally `pub(crate)` and pure (no I/O). The async
//! orchestration that fetches `cost_inputs` lives in `strategy.rs`.

use crate::cost::estimate_full_position_round_trip_ratio;
use crate::portfolio_cost::PortfolioCostInputs;
use bigdecimal::{BigDecimal, FromPrimitive};
use common::algorithm::portfolio::MAX_POSITION_SIZE;
use common::algorithm::types::TokenData;
use common::types::{TokenOutAccount, YoctoValue};
use std::collections::{BTreeMap, HashSet};

/// Thresholds driving [`apply_alpha_gate`]. Grouped to keep the function
/// signature manageable as more knobs are added.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AlphaGateThresholds {
    /// Safety multiplier `k` in `H × ER > k × cost`. Clamped at
    /// the typed-config layer (`TRADE_ALPHA_GATE_MULTIPLIER`).
    pub multiplier: f64,
    /// Holding period `H` (cycles). `1` is the conservative single-cycle
    /// round trip; larger values amortise cost across multiple cycles.
    pub hold_cycles: u32,
    /// Minimum number of tokens that must reach the optimizer. When fewer
    /// pass the gate, the highest-`expected_return` rejected tokens are
    /// reinstated to fill the shortage. `0` disables the fallback.
    pub min_pass_count: usize,
}

/// One token rejected by the gate, with the values used to decide.
/// Carried in [`AlphaGateOutcome`] for telemetry and fallback ranking.
#[derive(Debug, Clone)]
pub(crate) struct AlphaGateRejection {
    pub token: TokenOutAccount,
    pub expected_return: f64,
    pub round_trip_cost: f64,
}

/// Outcome of [`apply_alpha_gate`].
///
/// `kept` is the set of tokens that survive to the optimizer (gate pass +
/// held bypass + fallback fill). `rejected` lists every token the gate
/// would have removed, including any that were later reinstated via
/// fallback — `fallback_count` records how many. Held bypass tokens are
/// not represented in `rejected` (they never enter the gate decision).
pub(crate) struct AlphaGateOutcome {
    pub kept: HashSet<TokenOutAccount>,
    pub rejected: Vec<AlphaGateRejection>,
    pub fallback_used: bool,
    pub fallback_count: usize,
}

/// Apply the alpha gate filter. See module docs for the math.
///
/// `tokens` is the candidate set after upstream filters (confidence /
/// liquidity). `expected_returns` carries per-token alpha (typically
/// `(predicted - current) / current`). `cost_inputs.bundles` supplies
/// the swap paths and spot rate per token; tokens missing a bundle are
/// kept (the caller's existing path-failure flow handles them).
///
/// `total_value_yocto` is the wallet's total NEAR equivalent in
/// yoctoNEAR. The gate sizes the worst-case position as
/// `total_value × MAX_POSITION_SIZE` (the Markowitz box-bound ceiling),
/// matching the largest single-token exposure Markowitz can ask for.
///
/// `held_tokens` always bypass the gate.
pub(crate) fn apply_alpha_gate(
    tokens: &[TokenData],
    expected_returns: &BTreeMap<TokenOutAccount, f64>,
    cost_inputs: &PortfolioCostInputs,
    total_value_yocto: &BigDecimal,
    thresholds: &AlphaGateThresholds,
    held_tokens: &HashSet<TokenOutAccount>,
) -> AlphaGateOutcome {
    // Worst-case position size: 60% of total value (= MAX_POSITION_SIZE).
    // This is the largest single-asset weight the box-bound optimizer can
    // assign, so the gate's cost estimate is upper-bounded by what the
    // optimizer can actually instruct.
    let position_size_yocto = position_size_yocto(total_value_yocto);

    let mut kept: HashSet<TokenOutAccount> = HashSet::with_capacity(tokens.len());
    let mut rejected: Vec<AlphaGateRejection> = Vec::new();

    for token_data in tokens {
        let token = &token_data.symbol;

        // Held bypass: existing positions can always exit even if their
        // forward alpha would not pass the gate. Mirrors the held-bypass
        // pattern in `top_n_pruner::prune_top_n`.
        if held_tokens.contains(token) {
            kept.insert(token.clone());
            continue;
        }

        // Tokens without a swap bundle (path failure) are kept and
        // delegated to the caller's `failed_tokens` / `retain_excluding`
        // flow. Filtering them here would double-handle the failure.
        let Some(bundle) = cost_inputs.bundles.get(token) else {
            kept.insert(token.clone());
            continue;
        };

        let er = expected_returns.get(token).copied().unwrap_or(0.0);

        // Round-trip cost estimation can fail when path/rate combinations
        // are pathological (e.g. position_size = 0). Treat the failure as
        // a hard rejection with `f64::INFINITY` cost so the rejection is
        // visible in telemetry. The `RoundTripCostRatio` invariant ensures
        // a successful estimate never returns NaN/Infinity.
        let cost = match estimate_full_position_round_trip_ratio(
            &bundle.buy_path,
            &bundle.sell_path,
            &position_size_yocto,
            &bundle.rate,
            cost_inputs.gas_price,
            &cost_inputs.storage_min,
            new_token_count(cost_inputs, token),
        ) {
            Ok(ratio) => ratio.as_f64(),
            Err(_) => {
                rejected.push(AlphaGateRejection {
                    token: token.clone(),
                    expected_return: er,
                    round_trip_cost: f64::INFINITY,
                });
                continue;
            }
        };

        if gate_passes(er, cost, thresholds) {
            kept.insert(token.clone());
        } else {
            rejected.push(AlphaGateRejection {
                token: token.clone(),
                expected_return: er,
                round_trip_cost: cost,
            });
        }
    }

    let (fallback_used, fallback_count) =
        apply_min_pass_fallback(&mut kept, &rejected, thresholds.min_pass_count, held_tokens);

    AlphaGateOutcome {
        kept,
        rejected,
        fallback_used,
        fallback_count,
    }
}

/// Compute the worst-case single-token position size as a `YoctoValue`.
///
/// `MAX_POSITION_SIZE = 0.6` is a finite positive constant, so
/// `BigDecimal::from_f64` always succeeds; the `unwrap_or` is defensive
/// against a future regression that would break the finiteness invariant.
fn position_size_yocto(total_value_yocto: &BigDecimal) -> YoctoValue {
    let max_pos = BigDecimal::from_f64(MAX_POSITION_SIZE).unwrap_or_else(|| BigDecimal::from(0));
    YoctoValue::from_yocto(total_value_yocto * max_pos)
}

/// Resolve the `new_token_count` argument to `estimate_full_position_round_trip_ratio`.
///
/// `0` when the token is already registered in ref storage (no fresh
/// deposit needed), `1` otherwise. Mirrors `compute_cost_deductions`.
fn new_token_count(cost_inputs: &PortfolioCostInputs, token: &TokenOutAccount) -> usize {
    let token_account: common::types::TokenAccount = token.clone().into();
    if cost_inputs.existing_deposits.contains(&token_account) {
        0
    } else {
        1
    }
}

/// Evaluate `hold_cycles × ER > multiplier × cost`.
///
/// Non-finite `er` (NaN/Inf) collapses to "reject" because the comparison
/// against any finite threshold returns false anyway, but the explicit
/// `is_finite` guard documents the contract and protects against future
/// refactors that might rely on a stable "ER > 0 ⇒ inspect" invariant.
fn gate_passes(er: f64, cost: f64, thresholds: &AlphaGateThresholds) -> bool {
    if !er.is_finite() {
        return false;
    }
    let h = thresholds.hold_cycles as f64;
    er * h > thresholds.multiplier * cost
}

/// Reinstate top-`expected_return` rejected tokens until `kept.len() >= min_pass_count`.
///
/// Returns `(fallback_used, fallback_count)`. `held_tokens` are excluded
/// from the count target because the caller already counts them via the
/// held-bypass pass earlier.
fn apply_min_pass_fallback(
    kept: &mut HashSet<TokenOutAccount>,
    rejected: &[AlphaGateRejection],
    min_pass_count: usize,
    held_tokens: &HashSet<TokenOutAccount>,
) -> (bool, usize) {
    if min_pass_count == 0 {
        return (false, 0);
    }
    let non_held_kept = kept.iter().filter(|t| !held_tokens.contains(*t)).count();
    if non_held_kept >= min_pass_count {
        return (false, 0);
    }
    let needed = min_pass_count - non_held_kept;
    // Stable sort rejected by expected_return descending. `partial_cmp`
    // returns `None` for NaN; treat NaN as the smallest value so it ends
    // up last (effectively never reinstated by fallback).
    let mut ranked: Vec<&AlphaGateRejection> = rejected.iter().collect();
    ranked.sort_by(|a, b| {
        b.expected_return
            .partial_cmp(&a.expected_return)
            .unwrap_or(std::cmp::Ordering::Less)
    });
    let mut count = 0;
    for rejection in ranked.into_iter().take(needed) {
        if kept.insert(rejection.token.clone()) {
            count += 1;
        }
    }
    (count > 0, count)
}

#[cfg(test)]
mod tests;
