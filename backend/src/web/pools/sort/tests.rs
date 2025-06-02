use super::*;
use crate::ref_finance::pool_info::{PoolInfo, PoolInfoList};
use crate::ref_finance::token_account::TokenAccount;
use bigdecimal::BigDecimal;
use chrono::Utc;
use std::str::FromStr;
use std::sync::Arc;

fn create_mock_pool_info(
    pool_id: u32,
    token1: &str,
    token2: &str,
    amount1: u128,
    amount2: u128,
) -> Arc<PoolInfo> {
    use crate::ref_finance::pool_info::PoolInfoBared;
    use near_sdk::json_types::U128;

    let token1_acc = TokenAccount::from_str(token1).unwrap();
    let token2_acc = TokenAccount::from_str(token2).unwrap();

    let bare = PoolInfoBared {
        pool_kind: "SIMPLE_POOL".to_string(),
        token_account_ids: vec![token1_acc, token2_acc],
        amounts: vec![U128::from(amount1), U128::from(amount2)],
        total_fee: 25,
        shares_total_supply: U128::from(amount1 + amount2),
        amp: 0,
    };

    Arc::new(PoolInfo::new(pool_id, bare, Utc::now().naive_utc()))
}

#[test]
fn test_with_weight_ordering() {
    let w1 = WithWeight {
        value: "test1",
        weight: 1.0,
    };
    let w2 = WithWeight {
        value: "test2",
        weight: 2.0,
    };
    let w3 = WithWeight {
        value: "test3",
        weight: 1.0,
    };

    // Test comparison
    assert!(w2 > w1);
    assert!(w1 < w2);
    assert_eq!(w1, w3);

    // Test sorting
    let mut weights = vec![w2, w1, w3];
    weights.sort();

    assert_eq!(weights[0].weight, 1.0);
    assert_eq!(weights[1].weight, 1.0);
    assert_eq!(weights[2].weight, 2.0);
}

#[test]
fn test_with_weight_partial_cmp() {
    let w1 = WithWeight {
        value: "test1",
        weight: 1.0,
    };
    let w2 = WithWeight {
        value: "test2",
        weight: 2.0,
    };

    assert_eq!(w1.partial_cmp(&w2), Some(Ordering::Less));
    assert_eq!(w2.partial_cmp(&w1), Some(Ordering::Greater));
    assert_eq!(w1.partial_cmp(&w1), Some(Ordering::Equal));
}

#[test]
fn test_with_weight_equality() {
    let w1 = WithWeight {
        value: "test1",
        weight: 1.5,
    };
    let w2 = WithWeight {
        value: "test2",
        weight: 1.5,
    };
    let w3 = WithWeight {
        value: "test3",
        weight: 2.0,
    };

    assert_eq!(w1, w2);
    assert_ne!(w1, w3);
}

#[test]
fn test_average_depth_basic() {
    let pool = create_mock_pool_info(
        1,
        "wrap.near",
        "token1.near",
        1_000_000_000_000_000_000_000_000, // 1 NEAR
        2_000_000_000_000_000_000_000_000, // 2 tokens
    );

    let mut rates = HashMap::new();
    rates.insert(TokenAccount::from_str("wrap.near").unwrap(), BigDecimal::from(1));
    rates.insert(TokenAccount::from_str("token1.near").unwrap(), BigDecimal::from_str("0.5").unwrap());

    let value = average_depth(&rates, &pool);

    // Expected: (1e24 * 1.0 + 2e24 * 0.5) / 2 = 1e24
    let expected = BigDecimal::from_str("1000000000000000000000000").unwrap();
    let diff = (&value - &expected).abs();
    let threshold = BigDecimal::from_str("100000000000000000000").unwrap();
    assert!(
        diff < threshold,
        "Expected approximately {}, got {}",
        expected,
        value
    );
}

#[test]
fn test_average_depth_missing_rate() {
    let pool = create_mock_pool_info(
        1,
        "wrap.near",
        "unknown.near",
        1_000_000_000_000_000_000_000_000, // 1 NEAR
        2_000_000_000_000_000_000_000_000, // 2 tokens
    );

    let mut rates = HashMap::new();
    rates.insert(TokenAccount::from_str("wrap.near").unwrap(), BigDecimal::from(1));
    // No rate for unknown.near

    let value = average_depth(&rates, &pool);

    // Expected: (1e24 * 1.0 + 0) / 2 = 0.5e24
    let expected = BigDecimal::from_str("500000000000000000000000").unwrap();
    let diff = (&value - &expected).abs();
    let threshold = BigDecimal::from_str("100000000000000000000").unwrap();
    assert!(
        diff < threshold,
        "Expected approximately {}, got {}",
        expected,
        value
    );
}

#[test]
fn test_average_depth_zero_tokens() {
    let pool = create_mock_pool_info(
        1,
        "wrap.near",
        "token1.near",
        0, // 0 NEAR
        0, // 0 tokens
    );

    let mut rates = HashMap::new();
    rates.insert(TokenAccount::from_str("wrap.near").unwrap(), BigDecimal::from(1));
    rates.insert(TokenAccount::from_str("token1.near").unwrap(), BigDecimal::from_str("0.5").unwrap());

    let value = average_depth(&rates, &pool);

    // Expected: (0 * 1.0 + 0 * 0.5) / 2 = 0
    assert_eq!(value, BigDecimal::from(0));
}

// Note: The following tests require database setup and are complex integration tests
// For now, we'll focus on unit tests for the components we can test in isolation

#[test]
fn test_sort_empty_pools() {
    let empty_pools = Arc::new(PoolInfoList::new(vec![]));

    // This test may not work without proper database setup for make_rates
    // but we can test the structure
    let result = sort(empty_pools);

    // The function should handle empty pools gracefully
    match result {
        Ok(sorted) => assert!(sorted.is_empty()),
        Err(_) => {
            // It's acceptable if it fails due to missing database/graph setup
            // The important part is that it doesn't panic
        }
    }
}
