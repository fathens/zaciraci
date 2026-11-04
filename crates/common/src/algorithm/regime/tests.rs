use super::*;
use crate::algorithm::types::PricePoint;
use crate::types::{TokenAccount, TokenOutAccount, TokenPrice};
use bigdecimal::BigDecimal;
use chrono::Utc;
use std::str::FromStr;

fn token(s: &str) -> TokenOutAccount {
    TokenOutAccount::from(TokenAccount::from_str(s).expect("valid account"))
}

fn quote_token() -> crate::types::TokenInAccount {
    crate::types::TokenInAccount::from(TokenAccount::from_str("wrap.near").expect("valid account"))
}

fn price_history_with_prices(token_str: &str, prices: &[f64]) -> PriceHistory {
    PriceHistory {
        token: token(token_str),
        quote_token: quote_token(),
        prices: prices
            .iter()
            .map(|&p| PricePoint {
                timestamp: Utc::now(),
                price: TokenPrice::from_near_per_token(
                    BigDecimal::from_str(&format!("{p}")).expect("finite price"),
                ),
                volume: None,
            })
            .collect(),
    }
}

// ── ExposureScales smart constructor ──

#[test]
fn exposure_scales_default_satisfies_invariants() {
    let s = ExposureScales::DEFAULT;
    assert!(s.bear() <= s.neutral());
    assert!(s.neutral() <= s.bull());
    assert!(s.bear() >= 0.0);
    assert!(s.bull() <= 1.0);
}

#[test]
fn exposure_scales_new_accepts_default() {
    let s = ExposureScales::new(1.0, 0.75, 0.5).unwrap();
    assert_eq!(s, ExposureScales::DEFAULT);
}

#[test]
fn exposure_scales_rejects_non_finite() {
    assert!(matches!(
        ExposureScales::new(f64::NAN, 0.5, 0.3),
        Err(ExposureScalesError::NonFinite { .. })
    ));
    assert!(matches!(
        ExposureScales::new(1.0, f64::INFINITY, 0.5),
        Err(ExposureScalesError::NonFinite { .. })
    ));
}

#[test]
fn exposure_scales_rejects_out_of_range() {
    assert!(matches!(
        ExposureScales::new(1.5, 0.5, 0.3),
        Err(ExposureScalesError::OutOfRange { .. })
    ));
    assert!(matches!(
        ExposureScales::new(1.0, 0.5, -0.1),
        Err(ExposureScalesError::OutOfRange { .. })
    ));
}

#[test]
fn exposure_scales_rejects_non_monotonic() {
    // bear > neutral
    assert!(matches!(
        ExposureScales::new(1.0, 0.3, 0.5),
        Err(ExposureScalesError::NonMonotonic { .. })
    ));
    // neutral > bull
    assert!(matches!(
        ExposureScales::new(0.5, 0.7, 0.3),
        Err(ExposureScalesError::NonMonotonic { .. })
    ));
}

// ── MarketRegime::aggregate_cap ──

#[test]
fn regime_maps_to_correct_scale() {
    let s = ExposureScales::DEFAULT;
    assert_eq!(MarketRegime::Bull.aggregate_cap(&s), 1.0);
    assert_eq!(MarketRegime::Neutral.aggregate_cap(&s), 0.75);
    assert_eq!(MarketRegime::Bear.aggregate_cap(&s), 0.5);
}

// ── calculate_market_breadth ──

#[test]
fn breadth_returns_none_for_empty_input() {
    let prices: BTreeMap<TokenOutAccount, PriceHistory> = BTreeMap::new();
    assert!(calculate_market_breadth(&prices, 20).is_none());
}

#[test]
fn breadth_returns_none_for_zero_period() {
    let mut prices = BTreeMap::new();
    prices.insert(
        token("a.near"),
        price_history_with_prices("a.near", &[1.0, 1.1]),
    );
    assert!(calculate_market_breadth(&prices, 0).is_none());
}

#[test]
fn breadth_skips_tokens_with_insufficient_history() {
    let mut prices = BTreeMap::new();
    prices.insert(
        token("a.near"),
        price_history_with_prices("a.near", &[1.0, 1.1]),
    );
    // sma_period = 20 > 2 data points → skip → no usable token → None
    assert!(calculate_market_breadth(&prices, 20).is_none());
}

#[test]
fn breadth_unity_when_all_above_sma() {
    // Monotonically increasing prices: latest > SMA always
    let mut prices = BTreeMap::new();
    prices.insert(
        token("a.near"),
        price_history_with_prices("a.near", &[1.0, 1.1, 1.2, 1.3, 1.4, 1.5]),
    );
    prices.insert(
        token("b.near"),
        price_history_with_prices("b.near", &[2.0, 2.1, 2.2, 2.3, 2.4, 2.5]),
    );
    let breadth = calculate_market_breadth(&prices, 5).expect("non-empty");
    assert!((breadth - 1.0).abs() < 1e-12);
}

#[test]
fn breadth_zero_when_all_below_sma() {
    // Monotonically decreasing prices: latest < SMA always
    let mut prices = BTreeMap::new();
    prices.insert(
        token("a.near"),
        price_history_with_prices("a.near", &[1.5, 1.4, 1.3, 1.2, 1.1, 1.0]),
    );
    prices.insert(
        token("b.near"),
        price_history_with_prices("b.near", &[2.5, 2.4, 2.3, 2.2, 2.1, 2.0]),
    );
    let breadth = calculate_market_breadth(&prices, 5).expect("non-empty");
    assert!((breadth - 0.0).abs() < 1e-12);
}

#[test]
fn breadth_half_when_one_above_one_below() {
    let mut prices = BTreeMap::new();
    prices.insert(
        token("a.near"),
        price_history_with_prices("a.near", &[1.0, 1.1, 1.2, 1.3, 1.4, 1.5]),
    );
    prices.insert(
        token("b.near"),
        price_history_with_prices("b.near", &[2.5, 2.4, 2.3, 2.2, 2.1, 2.0]),
    );
    let breadth = calculate_market_breadth(&prices, 5).expect("non-empty");
    assert!((breadth - 0.5).abs() < 1e-12);
}

// ── detect_regime_from_prices ──

#[test]
fn regime_neutral_for_insufficient_data() {
    let prices = BTreeMap::new();
    assert_eq!(
        detect_regime_from_prices(&prices, 20),
        MarketRegime::Neutral
    );
}

#[test]
fn regime_bull_when_breadth_above_threshold() {
    // 4 of 4 tokens above own SMA → breadth = 1.0 > 0.70 → Bull
    let mut prices = BTreeMap::new();
    for sym in ["a.near", "b.near", "c.near", "d.near"] {
        prices.insert(
            token(sym),
            price_history_with_prices(sym, &[1.0, 1.1, 1.2, 1.3, 1.4, 1.5]),
        );
    }
    assert_eq!(detect_regime_from_prices(&prices, 5), MarketRegime::Bull);
}

#[test]
fn regime_bear_when_breadth_below_threshold() {
    // 4 of 4 tokens below own SMA → breadth = 0.0 < 0.30 → Bear
    let mut prices = BTreeMap::new();
    for sym in ["a.near", "b.near", "c.near", "d.near"] {
        prices.insert(
            token(sym),
            price_history_with_prices(sym, &[1.5, 1.4, 1.3, 1.2, 1.1, 1.0]),
        );
    }
    assert_eq!(detect_regime_from_prices(&prices, 5), MarketRegime::Bear);
}

#[test]
fn regime_neutral_when_breadth_in_middle_band() {
    // 2 of 4 tokens above own SMA → breadth = 0.5 ∈ (0.30, 0.70] → Neutral
    let mut prices = BTreeMap::new();
    prices.insert(
        token("a.near"),
        price_history_with_prices("a.near", &[1.0, 1.1, 1.2, 1.3, 1.4, 1.5]),
    );
    prices.insert(
        token("b.near"),
        price_history_with_prices("b.near", &[1.0, 1.1, 1.2, 1.3, 1.4, 1.5]),
    );
    prices.insert(
        token("c.near"),
        price_history_with_prices("c.near", &[1.5, 1.4, 1.3, 1.2, 1.1, 1.0]),
    );
    prices.insert(
        token("d.near"),
        price_history_with_prices("d.near", &[1.5, 1.4, 1.3, 1.2, 1.1, 1.0]),
    );
    assert_eq!(detect_regime_from_prices(&prices, 5), MarketRegime::Neutral);
}

// ── invariants ──

#[test]
fn aggregate_cap_always_in_unit_interval() {
    let s = ExposureScales::DEFAULT;
    for r in [
        MarketRegime::Bull,
        MarketRegime::Neutral,
        MarketRegime::Bear,
    ] {
        let c = r.aggregate_cap(&s);
        assert!((0.0..=1.0).contains(&c));
    }
}
