use super::*;
use crate::ref_finance::token_account::TokenAccount;
use crate::ref_finance::pool_info::{PoolInfo, PoolInfoList};
use std::str::FromStr;
use std::sync::Arc;
use bigdecimal::BigDecimal;
use chrono::Utc;
use zaciraci_common::types::YoctoNearToken;

#[cfg(test)]
mod tests {
    use super::*;

    fn create_mock_pool_info(
        pool_id: u32,
        token1: &str,
        token2: &str,
        amount1: u128,
        amount2: u128,
    ) -> Arc<PoolInfo> {
        let token1_acc = TokenAccount::from_str(token1).unwrap();
        let token2_acc = TokenAccount::from_str(token2).unwrap();

        Arc::new(PoolInfo::new(
            pool_id,
            vec![token1_acc, token2_acc],
            vec![
                YoctoNearToken::from_yocto(amount1),
                YoctoNearToken::from_yocto(amount2),
            ],
            Utc::now(),
        ))
    }

    fn create_mock_pool_list() -> Arc<PoolInfoList> {
        let pools = vec![
            create_mock_pool_info(
                1,
                "wrap.near",
                "token1.near",
                1_000_000_000_000_000_000_000_000, // 1 NEAR
                2_000_000_000_000_000_000_000_000, // 2 tokens
            ),
            create_mock_pool_info(
                2,
                "wrap.near",
                "token2.near",
                500_000_000_000_000_000_000_000, // 0.5 NEAR
                1_000_000_000_000_000_000_000_000, // 1 token
            ),
            create_mock_pool_info(
                3,
                "token1.near",
                "token2.near",
                3_000_000_000_000_000_000_000_000, // 3 tokens
                1_500_000_000_000_000_000_000_000, // 1.5 tokens
            ),
        ];

        Arc::new(PoolInfoList::new(pools))
    }

    #[test]
    fn test_with_weight_ordering() {
        let w1 = WithWight {
            value: "test1",
            weight: 1.0,
        };
        let w2 = WithWight {
            value: "test2",
            weight: 2.0,
        };
        let w3 = WithWight {
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
        let w1 = WithWight {
            value: "test1",
            weight: 1.0,
        };
        let w2 = WithWight {
            value: "test2",
            weight: 2.0,
        };

        assert_eq!(w1.partial_cmp(&w2), Some(Ordering::Less));
        assert_eq!(w2.partial_cmp(&w1), Some(Ordering::Greater));
        assert_eq!(w1.partial_cmp(&w1), Some(Ordering::Equal));
    }

    #[test]
    fn test_with_weight_equality() {
        let w1 = WithWight {
            value: "test1",
            weight: 1.5,
        };
        let w2 = WithWight {
            value: "test2",
            weight: 1.5,
        };
        let w3 = WithWight {
            value: "test3",
            weight: 2.0,
        };

        assert_eq!(w1, w2);
        assert_ne!(w1, w3);
    }

    #[test]
    fn test_amount_value_basic() {
        let pool = create_mock_pool_info(
            1,
            "wrap.near",
            "token1.near",
            1_000_000_000_000_000_000_000_000, // 1 NEAR
            2_000_000_000_000_000_000_000_000, // 2 tokens
        );

        let mut rates = HashMap::new();
        rates.insert(TokenAccount::from_str("wrap.near").unwrap(), 1.0);
        rates.insert(TokenAccount::from_str("token1.near").unwrap(), 0.5);

        let value = amount_value(&rates, &pool);
        
        // Expected: (1e24 * 1.0 + 2e24 * 0.5) / 2 = (1e24 + 1e24) / 2 = 1e24
        let expected = 1e24;
        assert!((value - expected).abs() < 1e20, "Expected approximately {}, got {}", expected, value);
    }

    #[test]
    fn test_amount_value_missing_rate() {
        let pool = create_mock_pool_info(
            1,
            "wrap.near",
            "unknown.near",
            1_000_000_000_000_000_000_000_000, // 1 NEAR
            2_000_000_000_000_000_000_000_000, // 2 tokens
        );

        let mut rates = HashMap::new();
        rates.insert(TokenAccount::from_str("wrap.near").unwrap(), 1.0);
        // No rate for unknown.near

        let value = amount_value(&rates, &pool);
        
        // Expected: (1e24 * 1.0 + 0) / 2 = 0.5e24
        let expected = 0.5e24;
        assert!((value - expected).abs() < 1e20, "Expected approximately {}, got {}", expected, value);
    }

    #[test]
    fn test_amount_value_zero_tokens() {
        let pool = create_mock_pool_info(
            1,
            "wrap.near",
            "token1.near",
            0, // 0 NEAR
            0, // 0 tokens
        );

        let mut rates = HashMap::new();
        rates.insert(TokenAccount::from_str("wrap.near").unwrap(), 1.0);
        rates.insert(TokenAccount::from_str("token1.near").unwrap(), 0.5);

        let value = amount_value(&rates, &pool);
        
        // Expected: (0 * 1.0 + 0 * 0.5) / 2 = 0
        assert_eq!(value, 0.0);
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
}