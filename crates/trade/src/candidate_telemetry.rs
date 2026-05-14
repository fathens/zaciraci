//! Telemetry for the all-predicted candidate pipeline.
//!
//! Captures funnel counts at each filter stage and percentile-based distribution
//! summaries to make optimizer input quality observable. Used by `strategy` when
//! `TRADE_ALL_PREDICTED_ENABLED` is on.

/// Count of candidates surviving each filter stage in a single trade cycle.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CandidateFunnel {
    /// All-predicted candidates union'd with held tokens (raw pool).
    pub predicted: usize,
    /// After per-token confidence filter (`trade_min_token_confidence`).
    pub after_confidence: usize,
    /// After hard liquidity / graph-reachability filter.
    pub after_liquidity: usize,
    /// Final input to the portfolio optimizer.
    pub optimizer_input: usize,
    /// Tokens with non-zero weight in the optimizer output.
    pub selected: usize,
}

/// Percentile summary of a distribution.
///
/// `p5` is included alongside the usual quartiles so left-tail skewness
/// (e.g. a small cluster of strongly-negative expected returns) is visible.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DistStats {
    pub min: f64,
    pub p5: f64,
    pub p25: f64,
    pub p50: f64,
    pub p75: f64,
    pub max: f64,
}

/// Summarize the distribution of `values`, returning `None` when no finite
/// values are present.
///
/// Non-finite inputs (`NaN`, `±inf`) are filtered out before computing
/// percentiles. Sorting uses `f64::total_cmp` so the operation never panics.
/// Percentiles are linearly interpolated between adjacent samples.
pub(crate) fn summarize_distribution(values: &[f64]) -> Option<DistStats> {
    let mut finite: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return None;
    }
    finite.sort_by(|a, b| a.total_cmp(b));

    let n = finite.len();
    let last = n - 1;
    let pct = |p: f64| -> f64 {
        let idx = p * last as f64;
        let lo = idx.floor() as usize;
        let hi = idx.ceil() as usize;
        if lo == hi {
            finite[lo]
        } else {
            let frac = idx - lo as f64;
            finite[lo] * (1.0 - frac) + finite[hi] * frac
        }
    };

    Some(DistStats {
        min: finite[0],
        p5: pct(0.05),
        p25: pct(0.25),
        p50: pct(0.50),
        p75: pct(0.75),
        max: finite[last],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected {a} ≈ {b}");
    }

    #[test]
    fn empty_returns_none() {
        assert!(summarize_distribution(&[]).is_none());
    }

    #[test]
    fn all_non_finite_returns_none() {
        let values = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY];
        assert!(summarize_distribution(&values).is_none());
    }

    #[test]
    fn single_value_collapses_to_constant() {
        let stats = summarize_distribution(&[1.5]).unwrap();
        approx(stats.min, 1.5);
        approx(stats.p5, 1.5);
        approx(stats.p25, 1.5);
        approx(stats.p50, 1.5);
        approx(stats.p75, 1.5);
        approx(stats.max, 1.5);
    }

    #[test]
    fn five_values_known_percentiles() {
        let stats = summarize_distribution(&[1.0, 2.0, 3.0, 4.0, 5.0]).unwrap();
        approx(stats.min, 1.0);
        approx(stats.p25, 2.0);
        approx(stats.p50, 3.0);
        approx(stats.p75, 4.0);
        approx(stats.max, 5.0);
    }

    #[test]
    fn percentiles_monotone() {
        let stats =
            summarize_distribution(&[0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0])
                .unwrap();
        assert!(stats.min <= stats.p5);
        assert!(stats.p5 <= stats.p25);
        assert!(stats.p25 <= stats.p50);
        assert!(stats.p50 <= stats.p75);
        assert!(stats.p75 <= stats.max);
    }

    #[test]
    fn non_finite_filtered() {
        let stats =
            summarize_distribution(&[1.0, f64::NAN, 2.0, f64::INFINITY, 3.0, f64::NEG_INFINITY])
                .unwrap();
        approx(stats.min, 1.0);
        approx(stats.max, 3.0);
        approx(stats.p50, 2.0);
    }

    #[test]
    fn unsorted_input_handled() {
        let stats = summarize_distribution(&[5.0, 1.0, 4.0, 2.0, 3.0]).unwrap();
        approx(stats.min, 1.0);
        approx(stats.max, 5.0);
        approx(stats.p50, 3.0);
    }
}
