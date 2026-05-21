use super::*;
use crate::portfolio_cost::{PortfolioCostInputs, TokenSwapBundle};
use bigdecimal::BigDecimal;
use blockchain::types::gas_price::GasPrice;
use chrono::Utc;
use common::algorithm::types::TokenData;
use common::types::{ExchangeRate, NearValue, TokenAccount, TokenOutAccount, YoctoValue};
use dex::{PoolInfo, PoolInfoBared, TokenIn, TokenOut, TokenPath};
use near_sdk::NearToken;
use near_sdk::json_types::U128;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

const ONE_NEAR_YOCTO: u128 = 1_000_000_000_000_000_000_000_000;

fn make_token(name: &str) -> TokenOutAccount {
    TokenAccount::from(name.parse::<near_sdk::AccountId>().unwrap()).to_out()
}

fn make_token_data(symbol: TokenOutAccount) -> TokenData {
    TokenData {
        symbol,
        current_rate: ExchangeRate::wnear(),
        historical_volatility: 0.05,
        liquidity_score: Some(0.5),
        market_cap: Some(NearValue::from_near(BigDecimal::from(1000))),
    }
}

/// Build a 100_000:1_000 wnear/token pool for the round-trip cost helper.
fn make_round_trip_bundle(token_name: &str) -> TokenSwapBundle {
    let pool = Arc::new(PoolInfo::new(
        0,
        PoolInfoBared {
            pool_kind: "SIMPLE_POOL".to_string(),
            token_account_ids: vec!["wrap.near".parse().unwrap(), token_name.parse().unwrap()],
            amounts: vec![U128(100_000 * ONE_NEAR_YOCTO), U128(1_000 * ONE_NEAR_YOCTO)],
            total_fee: 30,
            shares_total_supply: U128(0),
            amp: 0,
        },
        Utc::now().naive_utc(),
    ));
    let buy_pair = pool
        .get_pair(TokenIn::from(0), TokenOut::from(1))
        .expect("valid buy pair");
    let sell_pair = pool
        .get_pair(TokenIn::from(1), TokenOut::from(0))
        .expect("valid sell pair");
    TokenSwapBundle {
        buy_path: TokenPath(vec![buy_pair]),
        sell_path: TokenPath(vec![sell_pair]),
        rate: ExchangeRate::from_raw_rate(BigDecimal::from(ONE_NEAR_YOCTO / 100), 24),
    }
}

fn make_cost_inputs(bundles: BTreeMap<TokenOutAccount, TokenSwapBundle>) -> PortfolioCostInputs {
    PortfolioCostInputs {
        gas_price: GasPrice::from_balance(NearToken::from_yoctonear(100_000_000)),
        storage_min: YoctoValue::from_yocto_u128(100_000_000_000_000_000_000_000),
        existing_deposits: std::collections::HashSet::new(),
        bundles,
        max_position_vs_pool_ratio: 0.02,
        failed_tokens: Vec::new(),
    }
}

fn default_thresholds() -> AlphaGateThresholds {
    AlphaGateThresholds {
        multiplier: 2.0,
        hold_cycles: 1,
        min_pass_count: 0,
    }
}

#[test]
fn empty_token_set_returns_empty_outcome() {
    let total_value = BigDecimal::from(100_u32) * BigDecimal::from(ONE_NEAR_YOCTO);
    let outcome = apply_alpha_gate(
        &[],
        &BTreeMap::new(),
        &make_cost_inputs(BTreeMap::new()),
        &total_value,
        &default_thresholds(),
        &HashSet::new(),
    );
    assert!(outcome.kept.is_empty());
    assert!(outcome.rejected.is_empty());
    assert!(!outcome.fallback_used);
    assert_eq!(outcome.fallback_count, 0);
}

#[test]
fn high_alpha_token_passes_gate() {
    let token = make_token("good.near");
    let tokens = vec![make_token_data(token.clone())];
    let mut bundles = BTreeMap::new();
    bundles.insert(token.clone(), make_round_trip_bundle("good.near"));
    let mut ers = BTreeMap::new();
    // Very high ER far exceeds the round-trip cost (~0.6-1.6% on this pool).
    ers.insert(token.clone(), 0.5);
    let total_value = BigDecimal::from(100_u32) * BigDecimal::from(ONE_NEAR_YOCTO);

    let outcome = apply_alpha_gate(
        &tokens,
        &ers,
        &make_cost_inputs(bundles),
        &total_value,
        &default_thresholds(),
        &HashSet::new(),
    );
    assert!(outcome.kept.contains(&token), "high-ER token must pass");
    assert!(outcome.rejected.is_empty());
}

#[test]
fn flat_prediction_rejected_by_gate() {
    let token = make_token("flat.near");
    let tokens = vec![make_token_data(token.clone())];
    let mut bundles = BTreeMap::new();
    bundles.insert(token.clone(), make_round_trip_bundle("flat.near"));
    let mut ers = BTreeMap::new();
    // ER = 0 cannot recoup any positive cost.
    ers.insert(token.clone(), 0.0);
    let total_value = BigDecimal::from(100_u32) * BigDecimal::from(ONE_NEAR_YOCTO);

    let outcome = apply_alpha_gate(
        &tokens,
        &ers,
        &make_cost_inputs(bundles),
        &total_value,
        &default_thresholds(),
        &HashSet::new(),
    );
    assert!(!outcome.kept.contains(&token), "flat ER must be rejected");
    assert_eq!(outcome.rejected.len(), 1);
    assert_eq!(outcome.rejected[0].expected_return, 0.0);
    assert!(outcome.rejected[0].round_trip_cost > 0.0);
}

#[test]
fn held_token_bypasses_gate_even_with_flat_er() {
    let token = make_token("held.near");
    let tokens = vec![make_token_data(token.clone())];
    let mut bundles = BTreeMap::new();
    bundles.insert(token.clone(), make_round_trip_bundle("held.near"));
    let mut ers = BTreeMap::new();
    ers.insert(token.clone(), 0.0);
    let total_value = BigDecimal::from(100_u32) * BigDecimal::from(ONE_NEAR_YOCTO);
    let mut held = HashSet::new();
    held.insert(token.clone());

    let outcome = apply_alpha_gate(
        &tokens,
        &ers,
        &make_cost_inputs(bundles),
        &total_value,
        &default_thresholds(),
        &held,
    );
    assert!(outcome.kept.contains(&token), "held token must bypass gate");
    assert!(
        outcome.rejected.is_empty(),
        "held token never enters rejected"
    );
}

#[test]
fn token_without_bundle_is_kept_for_caller_to_filter() {
    let token = make_token("unreachable.near");
    let tokens = vec![make_token_data(token.clone())];
    let mut ers = BTreeMap::new();
    ers.insert(token.clone(), 0.0);
    let total_value = BigDecimal::from(100_u32) * BigDecimal::from(ONE_NEAR_YOCTO);

    let outcome = apply_alpha_gate(
        &tokens,
        &ers,
        &make_cost_inputs(BTreeMap::new()), // no bundles
        &total_value,
        &default_thresholds(),
        &HashSet::new(),
    );
    assert!(
        outcome.kept.contains(&token),
        "tokens without a bundle must be kept (caller's failed_tokens path)"
    );
    assert!(outcome.rejected.is_empty());
}

#[test]
fn min_pass_count_fallback_reinstates_highest_er_rejections() {
    // 3 rejected tokens with distinct ERs; fallback target = 2 → top 2
    // highest-ER tokens must be reinstated.
    let t_high = make_token("high.near");
    let t_mid = make_token("mid.near");
    let t_low = make_token("low.near");
    let tokens = vec![
        make_token_data(t_high.clone()),
        make_token_data(t_mid.clone()),
        make_token_data(t_low.clone()),
    ];
    let mut bundles = BTreeMap::new();
    bundles.insert(t_high.clone(), make_round_trip_bundle("high.near"));
    bundles.insert(t_mid.clone(), make_round_trip_bundle("mid.near"));
    bundles.insert(t_low.clone(), make_round_trip_bundle("low.near"));
    let mut ers = BTreeMap::new();
    ers.insert(t_high.clone(), 0.001);
    ers.insert(t_mid.clone(), 0.0005);
    ers.insert(t_low.clone(), 0.0001);
    let total_value = BigDecimal::from(100_u32) * BigDecimal::from(ONE_NEAR_YOCTO);
    let thresholds = AlphaGateThresholds {
        multiplier: 2.0,
        hold_cycles: 1,
        min_pass_count: 2,
    };

    let outcome = apply_alpha_gate(
        &tokens,
        &ers,
        &make_cost_inputs(bundles),
        &total_value,
        &thresholds,
        &HashSet::new(),
    );
    assert!(outcome.fallback_used, "fallback must engage when 0 pass");
    assert_eq!(outcome.fallback_count, 2);
    assert!(outcome.kept.contains(&t_high), "highest ER reinstated");
    assert!(outcome.kept.contains(&t_mid), "second highest reinstated");
    assert!(!outcome.kept.contains(&t_low), "lowest stays rejected");
}

#[test]
fn min_pass_count_zero_disables_fallback() {
    let token = make_token("flat.near");
    let tokens = vec![make_token_data(token.clone())];
    let mut bundles = BTreeMap::new();
    bundles.insert(token.clone(), make_round_trip_bundle("flat.near"));
    let mut ers = BTreeMap::new();
    ers.insert(token.clone(), 0.0);
    let total_value = BigDecimal::from(100_u32) * BigDecimal::from(ONE_NEAR_YOCTO);
    let thresholds = AlphaGateThresholds {
        multiplier: 2.0,
        hold_cycles: 1,
        min_pass_count: 0,
    };

    let outcome = apply_alpha_gate(
        &tokens,
        &ers,
        &make_cost_inputs(bundles),
        &total_value,
        &thresholds,
        &HashSet::new(),
    );
    assert!(!outcome.fallback_used, "min_pass_count=0 disables fallback");
    assert_eq!(outcome.fallback_count, 0);
    assert!(outcome.kept.is_empty(), "rejected token stays out");
}

#[test]
fn hold_cycles_relaxes_gate_threshold() {
    // ER = 0.005, cost ≈ 0.016 on this pool. With H=1, k=2 → 0.005 < 0.032 → reject.
    // With H=10, k=2 → 0.05 > 0.032 → pass.
    let token = make_token("borderline.near");
    let tokens = vec![make_token_data(token.clone())];
    let mut bundles = BTreeMap::new();
    bundles.insert(token.clone(), make_round_trip_bundle("borderline.near"));
    let mut ers = BTreeMap::new();
    ers.insert(token.clone(), 0.005);
    let total_value = BigDecimal::from(100_u32) * BigDecimal::from(ONE_NEAR_YOCTO);

    let tight = AlphaGateThresholds {
        multiplier: 2.0,
        hold_cycles: 1,
        min_pass_count: 0,
    };
    let relaxed = AlphaGateThresholds {
        multiplier: 2.0,
        hold_cycles: 10,
        min_pass_count: 0,
    };

    let tight_outcome = apply_alpha_gate(
        &tokens,
        &ers,
        &make_cost_inputs({
            let mut b = BTreeMap::new();
            b.insert(token.clone(), make_round_trip_bundle("borderline.near"));
            b
        }),
        &total_value,
        &tight,
        &HashSet::new(),
    );
    let relaxed_outcome = apply_alpha_gate(
        &tokens,
        &ers,
        &make_cost_inputs(bundles),
        &total_value,
        &relaxed,
        &HashSet::new(),
    );
    assert!(
        !tight_outcome.kept.contains(&token),
        "tight gate rejects borderline ER"
    );
    assert!(
        relaxed_outcome.kept.contains(&token),
        "relaxed gate (H=10) accepts borderline ER"
    );
}

#[test]
fn non_finite_expected_return_is_rejected() {
    let token = make_token("nan.near");
    let tokens = vec![make_token_data(token.clone())];
    let mut bundles = BTreeMap::new();
    bundles.insert(token.clone(), make_round_trip_bundle("nan.near"));
    let mut ers = BTreeMap::new();
    ers.insert(token.clone(), f64::NAN);
    let total_value = BigDecimal::from(100_u32) * BigDecimal::from(ONE_NEAR_YOCTO);

    let outcome = apply_alpha_gate(
        &tokens,
        &ers,
        &make_cost_inputs(bundles),
        &total_value,
        &default_thresholds(),
        &HashSet::new(),
    );
    assert!(!outcome.kept.contains(&token), "NaN ER must be rejected");
}
