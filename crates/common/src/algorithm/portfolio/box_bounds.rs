use crate::types::TokenOutAccount;
use std::collections::BTreeSet;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct BoxBounds {
    lower: Vec<f64>,
    upper: Vec<f64>,
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
}

const FEASIBILITY_TOLERANCE: f64 = 1e-9;

impl BoxBounds {
    pub fn uniform(n: usize, max_position: f64) -> Self {
        Self {
            lower: vec![0.0; n],
            upper: vec![max_position; n],
        }
    }

    /// 任意の per-asset 上限を持つ BoxBounds (lower は全て 0)。
    pub fn from_uppers(upper: Vec<f64>) -> Self {
        Self {
            lower: vec![0.0; upper.len()],
            upper,
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
        };
        bounds.validate()?;
        Ok(bounds)
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
