use super::helpers::ta;
use super::*;
use common::types::TokenAccount;
use near_sdk::json_types::U128;
use std::collections::HashMap;

#[test]
fn test_filter_tokens_to_liquidate_excludes_wrap_near() {
    let wrap_near: TokenAccount = "wrap.near".parse().expect("invalid TokenAccount in test");
    let token_a: TokenAccount = "token_a.near"
        .parse()
        .expect("invalid TokenAccount in test");

    let mut deposits = HashMap::new();
    deposits.insert(wrap_near.clone(), U128(1000));
    deposits.insert(token_a.clone(), U128(500));

    let result = filter_tokens_to_liquidate(&deposits, &wrap_near);

    assert_eq!(result.len(), 1);
    assert!(result.contains(&ta("token_a.near")));
    assert!(!result.contains(&ta("wrap.near")));
}

#[test]
fn test_filter_tokens_to_liquidate_excludes_zero_balance() {
    let wrap_near: TokenAccount = "wrap.near".parse().expect("invalid TokenAccount in test");
    let token_a: TokenAccount = "token_a.near"
        .parse()
        .expect("invalid TokenAccount in test");
    let token_b: TokenAccount = "token_b.near"
        .parse()
        .expect("invalid TokenAccount in test");

    let mut deposits = HashMap::new();
    deposits.insert(token_a.clone(), U128(500));
    deposits.insert(token_b.clone(), U128(0)); // ゼロ残高

    let result = filter_tokens_to_liquidate(&deposits, &wrap_near);

    assert_eq!(result.len(), 1);
    assert!(result.contains(&ta("token_a.near")));
    assert!(!result.contains(&ta("token_b.near")));
}

#[test]
fn test_filter_tokens_to_liquidate_includes_tokens_with_balance() {
    let wrap_near: TokenAccount = "wrap.near".parse().expect("invalid TokenAccount in test");
    let token_a: TokenAccount = "token_a.near"
        .parse()
        .expect("invalid TokenAccount in test");
    let token_b: TokenAccount = "token_b.near"
        .parse()
        .expect("invalid TokenAccount in test");
    let token_c: TokenAccount = "token_c.near"
        .parse()
        .expect("invalid TokenAccount in test");

    let mut deposits = HashMap::new();
    deposits.insert(wrap_near.clone(), U128(1000)); // 除外されるべき
    deposits.insert(token_a.clone(), U128(500));
    deposits.insert(token_b.clone(), U128(0)); // 除外されるべき
    deposits.insert(token_c.clone(), U128(750));

    let result = filter_tokens_to_liquidate(&deposits, &wrap_near);

    assert_eq!(result.len(), 2);
    assert!(result.contains(&ta("token_a.near")));
    assert!(result.contains(&ta("token_c.near")));
    assert!(!result.contains(&ta("wrap.near")));
    assert!(!result.contains(&ta("token_b.near")));
}

#[test]
fn test_filter_tokens_to_liquidate_empty_deposits() {
    let wrap_near: TokenAccount = "wrap.near".parse().expect("invalid TokenAccount in test");
    let deposits = HashMap::new();

    let result = filter_tokens_to_liquidate(&deposits, &wrap_near);

    assert!(result.is_empty());
}

#[test]
fn test_filter_tokens_to_liquidate_only_wrap_near() {
    let wrap_near: TokenAccount = "wrap.near".parse().expect("invalid TokenAccount in test");

    let mut deposits = HashMap::new();
    deposits.insert(wrap_near.clone(), U128(1000));

    let result = filter_tokens_to_liquidate(&deposits, &wrap_near);

    assert!(result.is_empty());
}
