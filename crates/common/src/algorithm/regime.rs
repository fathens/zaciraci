//! Market-regime detection via price-breadth indicator.
//!
//! Returns a `MarketRegime` (Bull/Bear/Neutral) based on the fraction of
//! tokens whose latest price is above their own SMA(n). Per-token comparison
//! avoids the WNEAR-base rate sign-inversion trap that would happen if we
//! used cross-token correlations or rate-based moving averages: a price that
//! rises above its own SMA is unambiguously bullish for that token,
//! regardless of how the WNEAR exchange rate is denominated.
//!
//! This module is intentionally pure (no I/O, no `cfg` reads). The async
//! adapter that fetches the historical prices and threads the resulting cap
//! into `BoxBounds::with_aggregate_cap` lives in `trade::regime`.

use crate::algorithm::types::PriceHistory;
use crate::types::TokenOutAccount;
use bigdecimal::BigDecimal;
use std::collections::BTreeMap;

/// Coarse market regime derived from price breadth.
///
/// Defaults to `Neutral` so that callers that obtain a `MarketRegime` from a
/// failed `detect_regime_from_prices` (insufficient data) get the conservative
/// "moderate de-risk" cap rather than full exposure or full cash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MarketRegime {
    Bull,
    Bear,
    #[default]
    Neutral,
}

/// Aggregate-cap scales associated with each regime.
///
/// Invariant (enforced by smart constructor): `0.0 ≤ bear ≤ neutral ≤ bull ≤ 1.0`,
/// so a more bullish regime never reduces the risk budget. The defaults
/// (1.0 / 0.75 / 0.5) match fin-reviewer's recommendation in the strategy
/// review and are tuned for the 30-day simulate evaluation in PR-A Step 12.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExposureScales {
    bull: f64,
    neutral: f64,
    bear: f64,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ExposureScalesError {
    #[error("non-finite scale: bull={bull}, neutral={neutral}, bear={bear}")]
    NonFinite { bull: f64, neutral: f64, bear: f64 },
    #[error("scale out of [0, 1]: bull={bull}, neutral={neutral}, bear={bear}")]
    OutOfRange { bull: f64, neutral: f64, bear: f64 },
    #[error(
        "monotonicity violated (require bear <= neutral <= bull): \
         bull={bull}, neutral={neutral}, bear={bear}"
    )]
    NonMonotonic { bull: f64, neutral: f64, bear: f64 },
}

impl ExposureScales {
    pub const DEFAULT: Self = Self {
        bull: 1.0,
        neutral: 0.75,
        bear: 0.5,
    };

    /// Smart constructor: validates finiteness, range, and monotonicity.
    pub fn new(bull: f64, neutral: f64, bear: f64) -> Result<Self, ExposureScalesError> {
        if !bull.is_finite() || !neutral.is_finite() || !bear.is_finite() {
            return Err(ExposureScalesError::NonFinite {
                bull,
                neutral,
                bear,
            });
        }
        if !(0.0..=1.0).contains(&bull)
            || !(0.0..=1.0).contains(&neutral)
            || !(0.0..=1.0).contains(&bear)
        {
            return Err(ExposureScalesError::OutOfRange {
                bull,
                neutral,
                bear,
            });
        }
        if bear > neutral || neutral > bull {
            return Err(ExposureScalesError::NonMonotonic {
                bull,
                neutral,
                bear,
            });
        }
        Ok(Self {
            bull,
            neutral,
            bear,
        })
    }

    pub fn bull(&self) -> f64 {
        self.bull
    }
    pub fn neutral(&self) -> f64 {
        self.neutral
    }
    pub fn bear(&self) -> f64 {
        self.bear
    }
}

impl Default for ExposureScales {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl MarketRegime {
    /// Map this regime to an aggregate cap using the supplied scales.
    pub fn aggregate_cap(self, scales: &ExposureScales) -> f64 {
        match self {
            MarketRegime::Bull => scales.bull,
            MarketRegime::Neutral => scales.neutral,
            MarketRegime::Bear => scales.bear,
        }
    }
}

/// Default thresholds: > 70 % of tokens above SMA → Bull,
/// < 30 % → Bear, otherwise Neutral. The `Neutral` zone (30 %–70 %) is
/// intentionally wide to dampen whipsaw on transitional days.
const BULL_BREADTH_THRESHOLD: f64 = 0.70;
const BEAR_BREADTH_THRESHOLD: f64 = 0.30;

/// Detect market regime from per-token price histories.
///
/// Returns `Neutral` when:
/// - `historical_prices` is empty,
/// - `sma_period == 0`,
/// - no token has at least `sma_period` data points (insufficient history),
/// - every token's SMA is non-positive (degenerate input).
///
/// This conservative fallback mirrors the "fail-soft to moderate de-risk"
/// policy: rather than refusing to produce a regime, we hand back the middle
/// regime so that the upstream cap stays within a safe band.
pub fn detect_regime_from_prices(
    historical_prices: &BTreeMap<TokenOutAccount, PriceHistory>,
    sma_period: usize,
) -> MarketRegime {
    let breadth = match calculate_market_breadth(historical_prices, sma_period) {
        Some(b) => b,
        None => return MarketRegime::Neutral,
    };
    if breadth > BULL_BREADTH_THRESHOLD {
        MarketRegime::Bull
    } else if breadth < BEAR_BREADTH_THRESHOLD {
        MarketRegime::Bear
    } else {
        MarketRegime::Neutral
    }
}

/// Returns the fraction of tokens whose most recent price exceeds their own
/// SMA(`sma_period`). Tokens with insufficient history (< `sma_period`
/// points) or a non-positive SMA are excluded from both numerator and
/// denominator.
///
/// Returns `None` when no token contributes a usable SMA so the caller can
/// fall back to `Neutral` rather than divide by zero.
pub fn calculate_market_breadth(
    historical_prices: &BTreeMap<TokenOutAccount, PriceHistory>,
    sma_period: usize,
) -> Option<f64> {
    if sma_period == 0 || historical_prices.is_empty() {
        return None;
    }

    let mut total = 0_usize;
    let mut above = 0_usize;
    let zero = BigDecimal::from(0);
    let period_bd = BigDecimal::from(sma_period as u32);

    for hist in historical_prices.values() {
        if hist.prices.len() < sma_period {
            continue;
        }
        let latest = hist
            .prices
            .last()
            .expect("len >= sma_period > 0 ⇒ non-empty");
        let latest_price = latest.price.as_bigdecimal();
        let sma_window = &hist.prices[hist.prices.len() - sma_period..];

        let sum: BigDecimal = sma_window
            .iter()
            .map(|p| p.price.as_bigdecimal().clone())
            .sum();
        let sma = sum / &period_bd;

        if sma <= zero {
            continue;
        }

        total += 1;
        if latest_price > &sma {
            above += 1;
        }
    }

    if total == 0 {
        return None;
    }
    Some(above as f64 / total as f64)
}

#[cfg(test)]
mod tests;
