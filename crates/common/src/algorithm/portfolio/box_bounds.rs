use crate::types::TokenOutAccount;
use std::collections::BTreeSet;
use thiserror::Error;

/// Aggregate-budget constraint for `BoxBounds`.
///
/// `Equality` (the default) is the historical behaviour: `sum(w) = 1`,
/// i.e. the optimizer is forced to allocate the entire budget across
/// risky assets. `AtMost(cap)` relaxes this to `sum(w) ≤ cap`, with the
/// remaining `1 - sum(w)` implicitly held as cash. The cap is stored
/// here as a smart-constructor-validated `f64` in `(0, 1]`; the
/// optimizer integration that actually honours it lands in a follow-up
/// commit so this commit is purely additive.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum BoxBoundsCap {
    /// Strict simplex: `sum(w) = 1` (legacy behaviour).
    #[default]
    Equality,
    /// Relaxed: `sum(w) ≤ cap`, `cap ∈ (0, 1]`. Anything not allocated is
    /// implicit cash.
    AtMost(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoxBounds {
    lower: Vec<f64>,
    upper: Vec<f64>,
    aggregate_cap: BoxBoundsCap,
}

#[derive(Debug, Error, PartialEq)]
pub enum BoxBoundsError {
    #[error("non-finite bound at index {0}")]
    NonFinite(usize),
    #[error("lower {1} > upper {2} at index {0}")]
    Inverted(usize, f64, f64),
    #[error("negative bound at index {0}")]
    Negative(usize),
    #[error("length mismatch: tokens={tokens}, current_weights={current_weights}")]
    LengthMismatch {
        tokens: usize,
        current_weights: usize,
    },
    #[error("infeasible: sum_upper={sum_upper} < 1.0")]
    UpperInfeasible { sum_upper: f64 },
    #[error("infeasible: sum_lower={sum_lower} > 1.0")]
    LowerInfeasible { sum_lower: f64 },
    #[error("non-finite aggregate cap: {0}")]
    NonFiniteCap(f64),
    #[error("non-positive aggregate cap: {0}")]
    NonPositiveCap(f64),
    #[error("aggregate cap exceeds unit: {0}")]
    CapExceedsUnit(f64),
    #[error("kelly upper length mismatch: bounds={bounds}, kelly={kelly}")]
    KellyLengthMismatch { bounds: usize, kelly: usize },
    #[error("non-finite kelly upper at index {idx}")]
    NonFiniteKellyUpper { idx: usize },
}

const FEASIBILITY_TOLERANCE: f64 = 1e-9;

impl BoxBounds {
    pub fn uniform(n: usize, max_position: f64) -> Self {
        Self {
            lower: vec![0.0; n],
            upper: vec![max_position; n],
            aggregate_cap: BoxBoundsCap::Equality,
        }
    }

    /// 任意の per-asset 上限を持つ BoxBounds (lower は全て 0)。
    pub fn from_uppers(upper: Vec<f64>) -> Self {
        Self {
            lower: vec![0.0; upper.len()],
            upper,
            aggregate_cap: BoxBoundsCap::Equality,
        }
    }

    pub fn with_held_sell_only(
        tokens: &[TokenOutAccount],
        current_weights: &[f64],
        held: &BTreeSet<TokenOutAccount>,
        max_position: f64,
        sell_only_epsilon: f64,
    ) -> Result<Self, BoxBoundsError> {
        if tokens.len() != current_weights.len() {
            return Err(BoxBoundsError::LengthMismatch {
                tokens: tokens.len(),
                current_weights: current_weights.len(),
            });
        }
        let mut upper = vec![max_position; tokens.len()];
        for (i, tok) in tokens.iter().enumerate() {
            if held.contains(tok) {
                upper[i] = (current_weights[i] + sell_only_epsilon).min(1.0);
            }
        }
        let bounds = Self {
            lower: vec![0.0; tokens.len()],
            upper,
            aggregate_cap: BoxBoundsCap::Equality,
        };
        bounds.validate()?;
        Ok(bounds)
    }

    /// Returns the aggregate-budget constraint. Defaults to
    /// `BoxBoundsCap::Equality`.
    pub fn aggregate_cap(&self) -> BoxBoundsCap {
        self.aggregate_cap
    }

    /// Tighten each per-asset upper to the minimum of the existing upper
    /// and the supplied half-Kelly upper. The bound is taken element-wise:
    /// `upper'[i] = min(upper[i], kelly_uppers[i])`. This composes naturally
    /// with the box bound — the more conservative cap wins, so the
    /// optimizer can never enter a position larger than either constraint
    /// allows.
    ///
    /// # Errors
    /// - [`BoxBoundsError::KellyLengthMismatch`] when `kelly_uppers.len()`
    ///   does not equal `self.len()`.
    /// - [`BoxBoundsError::NonFiniteKellyUpper`] when any element is
    ///   `NaN` / `±∞`. The half-Kelly module already returns `MAX_POSITION_SIZE`
    ///   for non-finite Kelly inputs, but we re-validate at the boundary so
    ///   that a future caller cannot bypass that defensive policy.
    pub fn apply_half_kelly(self, kelly_uppers: &[f64]) -> Result<Self, BoxBoundsError> {
        if kelly_uppers.len() != self.upper.len() {
            return Err(BoxBoundsError::KellyLengthMismatch {
                bounds: self.upper.len(),
                kelly: kelly_uppers.len(),
            });
        }
        for (idx, &k) in kelly_uppers.iter().enumerate() {
            if !k.is_finite() {
                return Err(BoxBoundsError::NonFiniteKellyUpper { idx });
            }
        }
        let mut new_upper = self.upper.clone();
        for (i, &k) in kelly_uppers.iter().enumerate() {
            new_upper[i] = new_upper[i].min(k.max(0.0));
        }
        Ok(Self {
            upper: new_upper,
            ..self
        })
    }

    /// Build a copy of these bounds with `aggregate_cap = AtMost(cap)`.
    ///
    /// Smart constructor — validates `cap` against the cash-bucket invariants
    /// before stamping it on the struct. Existing per-asset `lower` / `upper`
    /// values are preserved; the only effect is to relax `sum(w) = 1` to
    /// `sum(w) ≤ cap`.
    ///
    /// # Errors
    /// - [`BoxBoundsError::NonFiniteCap`] when `cap` is `NaN` / `±∞`.
    /// - [`BoxBoundsError::NonPositiveCap`] when `cap ≤ 0`. The aggregate
    ///   cap is the optimizer's risk budget; setting it to zero would force
    ///   100 % cash, which the dedicated `BoxBoundsCap::Equality` (with all
    ///   uppers at 0) already expresses more clearly.
    /// - [`BoxBoundsError::CapExceedsUnit`] when `cap > 1`. Any value above
    ///   1 would silently allow over-allocation; we hard-cap at 1 so the
    ///   invariant `sum(w) ≤ 1` is preserved exactly as in the legacy
    ///   `Equality` mode.
    pub fn with_aggregate_cap(self, cap: f64) -> Result<Self, BoxBoundsError> {
        if !cap.is_finite() {
            return Err(BoxBoundsError::NonFiniteCap(cap));
        }
        if cap <= 0.0 {
            return Err(BoxBoundsError::NonPositiveCap(cap));
        }
        if cap > 1.0 {
            return Err(BoxBoundsError::CapExceedsUnit(cap));
        }
        Ok(Self {
            aggregate_cap: BoxBoundsCap::AtMost(cap),
            ..self
        })
    }

    pub fn lower(&self, i: usize) -> f64 {
        self.lower[i]
    }

    pub fn upper(&self, i: usize) -> f64 {
        self.upper[i]
    }

    pub fn len(&self) -> usize {
        self.lower.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lower.is_empty()
    }

    pub fn lower_slice(&self) -> &[f64] {
        &self.lower
    }

    pub fn upper_slice(&self) -> &[f64] {
        &self.upper
    }

    /// 指定したインデックスのサブセットに対応する BoxBounds を抽出する。
    ///
    /// サブセット最適化（exhaustive 列挙等）で使用する。
    /// `aggregate_cap` はサブセット側に伝播する (cash bucket は portfolio
    /// レベルの constraint なので、sub-portfolio でも同じ cap を維持する)。
    pub fn subset(&self, indices: &[usize]) -> Self {
        Self {
            lower: indices.iter().map(|&i| self.lower[i]).collect(),
            upper: indices.iter().map(|&i| self.upper[i]).collect(),
            aggregate_cap: self.aggregate_cap,
        }
    }

    /// Per-asset effective upper bounds for box-constrained optimization.
    ///
    /// When `sum(upper) < 1.0` the simplex constraint `sum(w) = 1` is
    /// infeasible inside the box. In that case all uppers are scaled
    /// proportionally so that they sum to 1.0. For uniform bounds
    /// (`upper[i] = m`, `sum = n*m`) this yields `m / (n*m) = 1/n`, matching
    /// the legacy `effective_max = if n*m < 1.0 { 1/n } else { m }` behavior.
    pub fn effective_uppers(&self) -> Vec<f64> {
        let sum_upper: f64 = self.upper.iter().sum();
        if sum_upper > 0.0 && sum_upper < 1.0 {
            self.upper.iter().map(|&u| u / sum_upper).collect()
        } else {
            self.upper.clone()
        }
    }

    pub fn validate(&self) -> Result<(), BoxBoundsError> {
        for (i, (&l, &u)) in self.lower.iter().zip(&self.upper).enumerate() {
            if !l.is_finite() || !u.is_finite() {
                return Err(BoxBoundsError::NonFinite(i));
            }
            if l < 0.0 || u < 0.0 {
                return Err(BoxBoundsError::Negative(i));
            }
            if l > u {
                return Err(BoxBoundsError::Inverted(i, l, u));
            }
        }
        let sum_upper: f64 = self.upper.iter().sum();
        if sum_upper < 1.0 - FEASIBILITY_TOLERANCE {
            return Err(BoxBoundsError::UpperInfeasible { sum_upper });
        }
        let sum_lower: f64 = self.lower.iter().sum();
        if sum_lower > 1.0 + FEASIBILITY_TOLERANCE {
            return Err(BoxBoundsError::LowerInfeasible { sum_lower });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
