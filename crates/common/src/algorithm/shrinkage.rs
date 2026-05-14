//! Soft-threshold shrinkage on per-token expected returns.
//!
//! Implements `μ_adj = sign(μ) × max(0, |μ| - λ × √MSRE)` for portfolio
//! optimization input adjustment. The formula:
//!
//! - **Preserves sign**: a positive `μ` shrinks toward zero from above, a
//!   negative `μ` shrinks toward zero from below. Long-only strategies never
//!   see negative ER amplified by the shrinkage (a pitfall of the naïve
//!   subtractive form `μ - λ√MSRE`).
//! - **Bounds magnitude**: `|μ_adj| ≤ |μ|`, so shrinkage can only dampen,
//!   never amplify.
//! - **Dimensions cleanly**: `√MSRE` is in return scale (MSRE is the mean of
//!   squared relative errors), so `λ` is dimensionless. A typical `λ ∈
//!   [0.05, 0.3]` shrinks a `μ ≈ 3 %` signal by `0.5 %–1.5 %` when MAPE ≈ 10 %.
//!
//! Background: see Donoho-Johnstone (1995) soft-thresholding and the
//! James-Stein positive-part estimator for the theoretical heritage.

/// Apply the soft-threshold shrinkage to a single expected return.
///
/// Returns the raw `μ` unchanged when:
/// - `μ` is not finite (NaN/±∞) — caller is expected to detect and drop.
/// - `msre` is `None`, negative, or non-finite — no usable uncertainty signal.
///
/// Otherwise reduces `|μ|` by `λ × √MSRE`, clamping at 0.
pub fn apply_soft_threshold(mu: f64, msre: Option<f64>, lambda: f64) -> f64 {
    if !mu.is_finite() {
        return mu;
    }
    let Some(msre) = msre else {
        return mu;
    };
    if !msre.is_finite() || msre < 0.0 {
        return mu;
    }
    let penalty = lambda * msre.sqrt();
    let shrunk_abs = (mu.abs() - penalty).max(0.0);
    if mu > 0.0 {
        shrunk_abs
    } else if mu < 0.0 {
        -shrunk_abs
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected {a} ≈ {b}");
    }

    #[test]
    fn lambda_zero_is_identity() {
        approx(apply_soft_threshold(0.03, Some(0.01), 0.0), 0.03);
        approx(apply_soft_threshold(-0.05, Some(0.04), 0.0), -0.05);
    }

    #[test]
    fn msre_none_passes_through() {
        approx(apply_soft_threshold(0.03, None, 0.5), 0.03);
        approx(apply_soft_threshold(-0.03, None, 0.5), -0.03);
    }

    #[test]
    fn msre_invalid_passes_through() {
        approx(apply_soft_threshold(0.03, Some(f64::NAN), 0.5), 0.03);
        approx(apply_soft_threshold(0.03, Some(-0.01), 0.5), 0.03);
        approx(apply_soft_threshold(0.03, Some(f64::INFINITY), 0.5), 0.03);
    }

    #[test]
    fn mu_nan_propagates() {
        assert!(apply_soft_threshold(f64::NAN, Some(0.01), 0.5).is_nan());
    }

    #[test]
    fn positive_mu_dampens_toward_zero() {
        // μ = 0.03, λ = 0.1, MSRE = 0.01 → √MSRE = 0.1 → penalty = 0.01 → 0.02
        approx(apply_soft_threshold(0.03, Some(0.01), 0.1), 0.02);
    }

    #[test]
    fn negative_mu_dampens_toward_zero() {
        // |μ| = 0.03, penalty = 0.01 → 0.02, sign preserved → -0.02
        approx(apply_soft_threshold(-0.03, Some(0.01), 0.1), -0.02);
    }

    #[test]
    fn penalty_exceeding_mu_clamps_to_zero() {
        // |μ| = 0.005, penalty = 0.01 → max(0, -0.005) = 0
        approx(apply_soft_threshold(0.005, Some(0.01), 0.1), 0.0);
        approx(apply_soft_threshold(-0.005, Some(0.01), 0.1), 0.0);
    }

    #[test]
    fn sign_is_preserved_or_zero() {
        for mu in [-0.5, -0.1, -0.001, 0.001, 0.1, 0.5] {
            let adj = apply_soft_threshold(mu, Some(0.04), 0.2);
            if adj != 0.0 {
                assert_eq!(adj.signum(), mu.signum());
            }
        }
    }

    #[test]
    fn shrinkage_never_amplifies() {
        // |μ_adj| ≤ |μ| for any λ ≥ 0, valid MSRE.
        for &mu in &[0.05_f64, -0.05, 0.001, -0.001, 1.0, -1.0] {
            for &msre in &[0.0_f64, 0.01, 0.1, 1.0] {
                for &lambda in &[0.0_f64, 0.1, 0.5, 1.0] {
                    let adj = apply_soft_threshold(mu, Some(msre), lambda);
                    assert!(
                        adj.abs() <= mu.abs() + 1e-12,
                        "μ={mu} msre={msre} λ={lambda}: |adj|={} > |μ|={}",
                        adj.abs(),
                        mu.abs(),
                    );
                }
            }
        }
    }
}
