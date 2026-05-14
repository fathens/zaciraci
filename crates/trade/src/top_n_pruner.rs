//! Top-N candidate pruning by composite score, with held-token bypass.
//!
//! When `TRADE_TOP_N_AFTER_PREDICTION > 0` and all-token mode is on, the
//! confidence-filtered candidate set is further reduced to keep only the
//! top-N highest-scoring non-held tokens plus all currently held tokens
//! (so sell-only liquidation paths are always available).
//!
//! The composite score `confidence × liquidity_score × max(0, ER)` is
//! deliberately one-sided: tokens whose expected return is non-positive
//! contribute zero to the rank because the strategy is long-only and the
//! optimizer would assign them weight=0 anyway. Held tokens with negative
//! ER are still surfaced (via the bypass) so they can be sold.

use common::types::TokenOutAccount;
use std::collections::{BTreeMap, BTreeSet};

/// Composite score used to rank non-held candidates.
///
/// All inputs are expected to be in the post-confidence-filter set. NaN/inf
/// inputs propagate through the multiplication; callers should pre-filter or
/// rely on `f64::total_cmp` in the consumer.
pub(crate) fn composite_score(expected_return: f64, confidence: f64, liquidity_score: f64) -> f64 {
    confidence * liquidity_score * expected_return.max(0.0)
}

/// Return the set of tokens kept after Top-N pruning.
///
/// - Every token in `held` is always present in the result (sell-only bypass),
///   even if its `scores` entry is missing.
/// - For non-held tokens with a `scores` entry, the top `n` by score are kept
///   (descending order, NaN-safe via `f64::total_cmp`).
/// - Ties are resolved by `BTreeMap` iteration order (lexicographic on
///   `TokenOutAccount`), giving a deterministic outcome across runs.
/// - `n == 0` means no non-held tokens are kept; the result is `held.clone()`.
///
/// Invariants (checked by proptest in tests):
/// - `held ⊆ result`
/// - `result.len() ≤ held.len() + min(n, non_held_count)`
pub(crate) fn prune_top_n(
    scores: &BTreeMap<TokenOutAccount, f64>,
    held: &BTreeSet<TokenOutAccount>,
    n: usize,
) -> BTreeSet<TokenOutAccount> {
    let mut result: BTreeSet<TokenOutAccount> = held.clone();
    if n == 0 {
        return result;
    }

    // NaN scores indicate a degenerate input (e.g. NaN ER or NaN confidence
    // upstream) and would rank as the largest value under `total_cmp`'s NaN
    // ordering. Drop them up front so a single bad score cannot crowd out
    // legitimate candidates.
    let mut non_held: Vec<(&TokenOutAccount, f64)> = scores
        .iter()
        .filter(|(t, s)| !held.contains(*t) && !s.is_nan())
        .map(|(t, s)| (t, *s))
        .collect();
    // Descending sort. `total_cmp` is NaN-safe and never panics.
    non_held.sort_by(|a, b| b.1.total_cmp(&a.1));

    for (token, _) in non_held.into_iter().take(n) {
        result.insert(token.clone());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::types::TokenAccount;

    fn token(s: &str) -> TokenOutAccount {
        s.parse::<TokenAccount>().unwrap().into()
    }

    #[test]
    fn composite_score_truncates_negative_return() {
        assert_eq!(composite_score(-0.05, 0.9, 0.8), 0.0);
        assert_eq!(composite_score(0.0, 0.9, 0.8), 0.0);
        assert!((composite_score(0.1, 0.9, 0.8) - 0.072).abs() < 1e-9);
    }

    #[test]
    fn n_zero_returns_only_held() {
        let mut scores = BTreeMap::new();
        scores.insert(token("a.near"), 1.0);
        scores.insert(token("b.near"), 0.5);
        let mut held = BTreeSet::new();
        held.insert(token("h.near"));

        let kept = prune_top_n(&scores, &held, 0);
        assert_eq!(kept, held);
    }

    #[test]
    fn top_n_keeps_highest_scoring() {
        let mut scores = BTreeMap::new();
        scores.insert(token("a.near"), 0.1);
        scores.insert(token("b.near"), 0.9);
        scores.insert(token("c.near"), 0.5);
        scores.insert(token("d.near"), 0.3);
        let held = BTreeSet::new();

        let kept = prune_top_n(&scores, &held, 2);
        assert_eq!(kept.len(), 2);
        assert!(kept.contains(&token("b.near")));
        assert!(kept.contains(&token("c.near")));
    }

    #[test]
    fn held_always_included_even_if_low_score() {
        let mut scores = BTreeMap::new();
        scores.insert(token("a.near"), 0.9);
        scores.insert(token("b.near"), 0.8);
        scores.insert(token("h.near"), 0.01); // low-scoring held token
        let mut held = BTreeSet::new();
        held.insert(token("h.near"));

        let kept = prune_top_n(&scores, &held, 2);
        // Top 2 non-held: a, b. Plus held: h. Total 3.
        assert_eq!(kept.len(), 3);
        assert!(kept.contains(&token("a.near")));
        assert!(kept.contains(&token("b.near")));
        assert!(kept.contains(&token("h.near")));
    }

    #[test]
    fn held_with_no_score_still_included() {
        let mut scores = BTreeMap::new();
        scores.insert(token("a.near"), 0.5);
        let mut held = BTreeSet::new();
        held.insert(token("orphan.near")); // not in scores

        let kept = prune_top_n(&scores, &held, 1);
        assert!(kept.contains(&token("orphan.near")));
        assert!(kept.contains(&token("a.near")));
    }

    #[test]
    fn n_larger_than_candidates_keeps_all() {
        let mut scores = BTreeMap::new();
        scores.insert(token("a.near"), 0.5);
        scores.insert(token("b.near"), 0.3);
        let held = BTreeSet::new();

        let kept = prune_top_n(&scores, &held, 100);
        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn nan_scores_dont_panic_and_rank_last() {
        let mut scores = BTreeMap::new();
        scores.insert(token("a.near"), f64::NAN);
        scores.insert(token("b.near"), 0.5);
        scores.insert(token("c.near"), 0.3);
        let held = BTreeSet::new();

        // Top 2 should be b and c (finite values rank above NaN).
        let kept = prune_top_n(&scores, &held, 2);
        assert_eq!(kept.len(), 2);
        assert!(kept.contains(&token("b.near")));
        assert!(kept.contains(&token("c.near")));
    }
}
