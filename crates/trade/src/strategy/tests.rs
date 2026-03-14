use super::*;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use common::types::{ExchangeRate, NearValue, TokenAccount};
use near_sdk::json_types::U128;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

fn make_token(name: &str) -> TokenAccount {
    TokenAccount::from_str(name).unwrap()
}

fn make_pool(id: u32, tokens: Vec<&str>, amounts: Vec<u128>) -> Arc<dex::PoolInfo> {
    Arc::new(dex::PoolInfo::new(
        id,
        dex::pool_info::PoolInfoBared {
            pool_kind: "SIMPLE_POOL".to_string(),
            token_account_ids: tokens.into_iter().map(make_token).collect(),
            amounts: amounts.into_iter().map(U128).collect(),
            total_fee: 30,
            shares_total_supply: U128(0),
            amp: 0,
        },
        NaiveDate::from_ymd_opt(2024, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap(),
    ))
}

fn wnear() -> TokenAccount {
    make_token("wrap.near")
}

// =============================================================================
// estimate_pool_liquidity_in_near テスト
// =============================================================================

#[test]
fn test_balanced_wnear_pool() {
    // 500 NEAR + 500 NEAR 相当のトークン → min = 500 NEAR
    let wnear = wnear();
    // 500 NEAR = 500 * 10^24 yoctoNEAR
    let near_500_yocto: u128 = 500 * 10u128.pow(24);
    // USDT: 500 NEAR 相当 = 500 * rate
    // rate = 5_000_000 (decimals=6) → 1 NEAR = 5_000_000 smallest USDT
    // 500 NEAR → 500 * 5_000_000 = 2_500_000_000 smallest USDT
    let usdt_amount: u128 = 2_500_000_000;

    let pool = make_pool(
        1,
        vec!["wrap.near", "usdt.tether-token.near"],
        vec![near_500_yocto, usdt_amount],
    );

    let mut rates = HashMap::new();
    rates.insert(
        make_token("usdt.tether-token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6),
    );

    let result = estimate_pool_liquidity_in_near(&pool, &wnear, &rates);
    assert!(result.is_some());
    let liquidity = result.unwrap();
    assert_eq!(liquidity, NearValue::from_near(BigDecimal::from(500)));
}

#[test]
fn test_non_wnear_balanced_pool() {
    // USDT-USDC プール（wnear なし）、両側 100 NEAR 相当
    // USDT: rate=5_000_000 (dec=6), 100 NEAR = 500_000_000 smallest
    // USDC: rate=5_100_000 (dec=6), 100 NEAR = 510_000_000 smallest
    let wnear = wnear();
    let pool = make_pool(
        2,
        vec!["usdt.tether-token.near", "usdc.token.near"],
        vec![500_000_000, 510_000_000],
    );

    let mut rates = HashMap::new();
    rates.insert(
        make_token("usdt.tether-token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6),
    );
    rates.insert(
        make_token("usdc.token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_100_000), 6),
    );

    let result = estimate_pool_liquidity_in_near(&pool, &wnear, &rates);
    assert!(result.is_some());
    let liquidity = result.unwrap();
    // USDT side: 500_000_000 / 5_000_000 = 100 NEAR
    // USDC side: 510_000_000 / 5_100_000 = 100 NEAR
    assert_eq!(liquidity, NearValue::from_near(BigDecimal::from(100)));
}

#[test]
fn test_unbalanced_pool() {
    // アンバランスなプール: wnear=1000 NEAR, トークン=1 NEAR 相当 → min=1
    let wnear = wnear();
    let near_1000_yocto: u128 = 1000 * 10u128.pow(24);
    // 1 NEAR 相当の USDT = 5_000_000 smallest
    let pool = make_pool(
        3,
        vec!["wrap.near", "usdt.tether-token.near"],
        vec![near_1000_yocto, 5_000_000],
    );

    let mut rates = HashMap::new();
    rates.insert(
        make_token("usdt.tether-token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6),
    );

    let result = estimate_pool_liquidity_in_near(&pool, &wnear, &rates);
    assert!(result.is_some());
    let liquidity = result.unwrap();
    assert_eq!(liquidity, NearValue::from_near(BigDecimal::from(1)));
}

#[test]
fn test_partial_rate_missing() {
    // 片方のレートのみ存在: wnear 側は直接変換可能
    let wnear = wnear();
    let near_200_yocto: u128 = 200 * 10u128.pow(24);
    let pool = make_pool(
        4,
        vec!["wrap.near", "unknown-token.near"],
        vec![near_200_yocto, 999999],
    );

    let rates = HashMap::new(); // レートなし

    let result = estimate_pool_liquidity_in_near(&pool, &wnear, &rates);
    assert!(result.is_some());
    // wnear 側のみで判定: 200 NEAR
    let liquidity = result.unwrap();
    assert_eq!(liquidity, NearValue::from_near(BigDecimal::from(200)));
}

#[test]
fn test_all_rates_missing() {
    // 全トークンのレートが不明（wnear も含まないプール）
    let wnear = wnear();
    let pool = make_pool(
        5,
        vec!["unknown-a.near", "unknown-b.near"],
        vec![1000, 2000],
    );

    let rates = HashMap::new();

    let result = estimate_pool_liquidity_in_near(&pool, &wnear, &rates);
    assert!(result.is_none());
}

#[test]
fn test_zero_amounts() {
    // amounts がゼロのプール → Some(NearValue::zero())
    let wnear = wnear();
    let pool = make_pool(6, vec!["wrap.near", "usdt.tether-token.near"], vec![0, 0]);

    let mut rates = HashMap::new();
    rates.insert(
        make_token("usdt.tether-token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6),
    );

    let result = estimate_pool_liquidity_in_near(&pool, &wnear, &rates);
    assert!(result.is_some());
    assert!(result.unwrap().is_zero());
}

#[test]
fn test_empty_pool() {
    // トークンが無い空プール → None
    let wnear = wnear();
    let pool = make_pool(7, vec![], vec![]);

    let rates = HashMap::new();

    let result = estimate_pool_liquidity_in_near(&pool, &wnear, &rates);
    assert!(result.is_none());
}

// =============================================================================
// filter_pools_by_liquidity テスト
// =============================================================================

#[test]
fn test_filter_keeps_sufficient_pools() {
    let wnear = wnear();
    let min_liquidity = NearValue::from_near(BigDecimal::from(100));

    let near_500_yocto: u128 = 500 * 10u128.pow(24);
    let pool = make_pool(
        1,
        vec!["wrap.near", "usdt.tether-token.near"],
        vec![near_500_yocto, 2_500_000_000],
    );

    let pools = Arc::new(dex::PoolInfoList::new(vec![pool]));

    let mut rates = HashMap::new();
    rates.insert(
        make_token("usdt.tether-token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6),
    );

    let filtered = filter_pools_by_liquidity(&pools, &wnear, &min_liquidity, &rates);
    assert_eq!(filtered.list().len(), 1);
}

#[test]
fn test_filter_removes_insufficient_pools() {
    let wnear = wnear();
    let min_liquidity = NearValue::from_near(BigDecimal::from(100));

    // 1 NEAR しかないプール
    let near_1_yocto: u128 = 10u128.pow(24);
    let pool = make_pool(
        1,
        vec!["wrap.near", "usdt.tether-token.near"],
        vec![near_1_yocto, 5_000_000],
    );

    let pools = Arc::new(dex::PoolInfoList::new(vec![pool]));

    let mut rates = HashMap::new();
    rates.insert(
        make_token("usdt.tether-token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6),
    );

    let filtered = filter_pools_by_liquidity(&pools, &wnear, &min_liquidity, &rates);
    assert_eq!(filtered.list().len(), 0);
}

#[test]
fn test_filter_empty_pool_list() {
    let wnear = wnear();
    let min_liquidity = NearValue::from_near(BigDecimal::from(100));
    let pools = Arc::new(dex::PoolInfoList::new(vec![]));
    let rates = HashMap::new();

    let filtered = filter_pools_by_liquidity(&pools, &wnear, &min_liquidity, &rates);
    assert_eq!(filtered.list().len(), 0);
}
