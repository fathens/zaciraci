use super::helpers::token_account;
use super::*;
use near_sdk::json_types::U128;
use std::collections::BTreeMap;

#[test]
fn test_filter_tokens_to_liquidate_excludes_wrap_near() {
    let wrap_near = token_account("wrap.near");
    let token_a = token_account("token_a.near");

    let mut deposits = BTreeMap::new();
    deposits.insert(wrap_near.clone(), U128(1000));
    deposits.insert(token_a.clone(), U128(500));

    let result = filter_tokens_to_liquidate(&deposits, &wrap_near);

    assert_eq!(result.len(), 1);
    assert!(result.contains(&token_account("token_a.near")));
    assert!(!result.contains(&token_account("wrap.near")));
}

#[test]
fn test_filter_tokens_to_liquidate_excludes_zero_balance() {
    let wrap_near = token_account("wrap.near");
    let token_a = token_account("token_a.near");
    let token_b = token_account("token_b.near");

    let mut deposits = BTreeMap::new();
    deposits.insert(token_a.clone(), U128(500));
    deposits.insert(token_b.clone(), U128(0)); // ゼロ残高

    let result = filter_tokens_to_liquidate(&deposits, &wrap_near);

    assert_eq!(result.len(), 1);
    assert!(result.contains(&token_account("token_a.near")));
    assert!(!result.contains(&token_account("token_b.near")));
}

#[test]
fn test_filter_tokens_to_liquidate_includes_tokens_with_balance() {
    let wrap_near = token_account("wrap.near");
    let token_a = token_account("token_a.near");
    let token_b = token_account("token_b.near");
    let token_c = token_account("token_c.near");

    let mut deposits = BTreeMap::new();
    deposits.insert(wrap_near.clone(), U128(1000)); // 除外されるべき
    deposits.insert(token_a.clone(), U128(500));
    deposits.insert(token_b.clone(), U128(0)); // 除外されるべき
    deposits.insert(token_c.clone(), U128(750));

    let result = filter_tokens_to_liquidate(&deposits, &wrap_near);

    assert_eq!(result.len(), 2);
    assert!(result.contains(&token_account("token_a.near")));
    assert!(result.contains(&token_account("token_c.near")));
    assert!(!result.contains(&token_account("wrap.near")));
    assert!(!result.contains(&token_account("token_b.near")));
}

#[test]
fn test_filter_tokens_to_liquidate_empty_deposits() {
    let wrap_near = token_account("wrap.near");
    let deposits = BTreeMap::new();

    let result = filter_tokens_to_liquidate(&deposits, &wrap_near);

    assert!(result.is_empty());
}

#[test]
fn test_filter_tokens_to_liquidate_only_wrap_near() {
    let wrap_near = token_account("wrap.near");

    let mut deposits = BTreeMap::new();
    deposits.insert(wrap_near.clone(), U128(1000));

    let result = filter_tokens_to_liquidate(&deposits, &wrap_near);

    assert!(result.is_empty());
}
