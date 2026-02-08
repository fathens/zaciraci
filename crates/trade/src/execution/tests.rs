use super::*;

#[test]
fn test_filter_tokens_to_liquidate_excludes_wrap_near() {
    use blockchain::ref_finance::token_account::TokenAccount;
    use near_sdk::json_types::U128;
    use std::collections::HashMap;

    let wrap_near: TokenAccount = "wrap.near".parse().unwrap();
    let token_a: TokenAccount = "token_a.near".parse().unwrap();

    let mut deposits = HashMap::new();
    deposits.insert(wrap_near.clone(), U128(1000));
    deposits.insert(token_a.clone(), U128(500));

    let result = filter_tokens_to_liquidate(&deposits, &wrap_near);

    assert_eq!(result.len(), 1);
    assert!(result.contains(&"token_a.near".to_string()));
    assert!(!result.contains(&"wrap.near".to_string()));
}

#[test]
fn test_filter_tokens_to_liquidate_excludes_zero_balance() {
    use blockchain::ref_finance::token_account::TokenAccount;
    use near_sdk::json_types::U128;
    use std::collections::HashMap;

    let wrap_near: TokenAccount = "wrap.near".parse().unwrap();
    let token_a: TokenAccount = "token_a.near".parse().unwrap();
    let token_b: TokenAccount = "token_b.near".parse().unwrap();

    let mut deposits = HashMap::new();
    deposits.insert(token_a.clone(), U128(500));
    deposits.insert(token_b.clone(), U128(0)); // ゼロ残高

    let result = filter_tokens_to_liquidate(&deposits, &wrap_near);

    assert_eq!(result.len(), 1);
    assert!(result.contains(&"token_a.near".to_string()));
    assert!(!result.contains(&"token_b.near".to_string()));
}

#[test]
fn test_filter_tokens_to_liquidate_includes_tokens_with_balance() {
    use blockchain::ref_finance::token_account::TokenAccount;
    use near_sdk::json_types::U128;
    use std::collections::HashMap;

    let wrap_near: TokenAccount = "wrap.near".parse().unwrap();
    let token_a: TokenAccount = "token_a.near".parse().unwrap();
    let token_b: TokenAccount = "token_b.near".parse().unwrap();
    let token_c: TokenAccount = "token_c.near".parse().unwrap();

    let mut deposits = HashMap::new();
    deposits.insert(wrap_near.clone(), U128(1000)); // 除外されるべき
    deposits.insert(token_a.clone(), U128(500));
    deposits.insert(token_b.clone(), U128(0)); // 除外されるべき
    deposits.insert(token_c.clone(), U128(750));

    let result = filter_tokens_to_liquidate(&deposits, &wrap_near);

    assert_eq!(result.len(), 2);
    assert!(result.contains(&"token_a.near".to_string()));
    assert!(result.contains(&"token_c.near".to_string()));
    assert!(!result.contains(&"wrap.near".to_string()));
    assert!(!result.contains(&"token_b.near".to_string()));
}

#[test]
fn test_filter_tokens_to_liquidate_empty_deposits() {
    use blockchain::ref_finance::token_account::TokenAccount;
    use std::collections::HashMap;

    let wrap_near: TokenAccount = "wrap.near".parse().unwrap();
    let deposits = HashMap::new();

    let result = filter_tokens_to_liquidate(&deposits, &wrap_near);

    assert!(result.is_empty());
}

#[test]
fn test_filter_tokens_to_liquidate_only_wrap_near() {
    use blockchain::ref_finance::token_account::TokenAccount;
    use near_sdk::json_types::U128;
    use std::collections::HashMap;

    let wrap_near: TokenAccount = "wrap.near".parse().unwrap();

    let mut deposits = HashMap::new();
    deposits.insert(wrap_near.clone(), U128(1000));

    let result = filter_tokens_to_liquidate(&deposits, &wrap_near);

    assert!(result.is_empty());
}

// Rebalance logic tests
mod rebalance_tests {
    use bigdecimal::BigDecimal;
    use common::types::{ExchangeRate, NearValue, TokenAmount};
    use num_traits::ToPrimitive;
    use std::str::FromStr;

    #[test]
    fn test_rebalance_calculations_sell_only() {
        // Setup: Token A has 200 NEAR value, target is 100 NEAR
        // Should sell 100 NEAR worth of Token A
        let current_value = NearValue::from_near(BigDecimal::from(200));
        let target_value = NearValue::from_near(BigDecimal::from(100));
        let diff = &target_value - &current_value;

        assert_eq!(diff, NearValue::from_near(BigDecimal::from(-100)));
        assert!(diff < NearValue::zero());

        // ExchangeRate: raw_rate = 5e23 smallest_units/NEAR
        // つまり 1 NEAR で 0.5e24 = 0.5 tokens を取得 (price = 2 NEAR/token)
        // 100 NEAR × 5e23 = 5e25 = 50e24 smallest_units = 50 tokens
        let rate = ExchangeRate::from_raw_rate(
            BigDecimal::from_str("500000000000000000000000").unwrap(), // 5e23
            24,
        );
        let token_amount: TokenAmount = &diff.abs() * &rate;

        // Expected: 50 tokens = 50e24 smallest units
        let expected = BigDecimal::from_str("50000000000000000000000000").unwrap(); // 50e24
        assert_eq!(token_amount.smallest_units(), &expected);
    }

    #[test]
    fn test_rebalance_calculations_buy_only() {
        // Setup: Token B has 50 NEAR value, target is 100 NEAR
        // Should buy 50 NEAR worth of Token B
        let current_value = NearValue::from_near(BigDecimal::from(50));
        let target_value = NearValue::from_near(BigDecimal::from(100));
        let diff = &target_value - &current_value;

        assert_eq!(diff, NearValue::from_near(BigDecimal::from(50)));
        assert!(diff > NearValue::zero());

        // For buying, we use wrap.near amount directly (no token conversion needed)
        let wrap_near_amount = diff;
        assert_eq!(wrap_near_amount, NearValue::from_near(BigDecimal::from(50)));
    }

    #[test]
    fn test_rebalance_minimum_trade_size() {
        // Minimum trade size is 1 NEAR
        let min_trade_size = NearValue::one();

        // Small difference: 0.5 NEAR
        let small_diff = NearValue::from_near(BigDecimal::from_str("0.5").unwrap());
        assert!(small_diff < min_trade_size);

        // Large difference: 2 NEAR
        let large_diff = NearValue::from_near(BigDecimal::from(2));
        assert!(large_diff >= min_trade_size);
    }

    #[test]
    fn test_token_amount_conversion() {
        // Test: Convert NEAR value to token amount
        // If 100 NEAR worth should be sold, and price = 2 NEAR/token
        // Then token_amount = 100 NEAR × (0.5 tokens/NEAR) = 50 tokens
        //
        // raw_rate = 5e23 smallest_units/NEAR (価格の逆数)
        // 計算: 100 NEAR × 5e23 = 5e25 = 50e24 = 50 tokens
        let wrap_near_value = NearValue::from_near(BigDecimal::from(100));
        let rate = ExchangeRate::from_raw_rate(
            BigDecimal::from_str("500000000000000000000000").unwrap(), // 5e23
            24,
        );
        let token_amount: TokenAmount = &wrap_near_value * &rate;

        // Expected: 50 tokens = 50e24 smallest units
        let expected = BigDecimal::from_str("50000000000000000000000000").unwrap();
        assert_eq!(token_amount.smallest_units(), &expected);
    }

    #[test]
    fn test_wrap_near_value_calculation() {
        // Test: Calculate current value in NEAR
        // If balance is 100 tokens and price = 2 NEAR/token
        // Then value = 100 tokens × 2 NEAR/token = 200 NEAR
        //
        // raw_rate = 5e23 smallest_units/NEAR (価格 2 NEAR/token の逆数)
        // 計算: 100e24 / 5e23 = 200 NEAR
        let balance = TokenAmount::from_smallest_units(
            BigDecimal::from_str("100000000000000000000000000").unwrap(), // 100e24 = 100 tokens
            24,
        );
        let rate = ExchangeRate::from_raw_rate(
            BigDecimal::from_str("500000000000000000000000").unwrap(), // 5e23
            24,
        );
        let value: NearValue = &balance / &rate;

        assert_eq!(value, NearValue::from_near(BigDecimal::from(200)));
    }

    #[test]
    fn test_two_phase_rebalance_scenario() {
        // Scenario: Portfolio with 2 tokens
        // Total value: 300 NEAR
        // Target weights: Token A = 40%, Token B = 60%
        // Current: Token A = 200 NEAR, Token B = 100 NEAR
        // Expected:
        //   Token A target = 120 NEAR -> sell 80 NEAR worth
        //   Token B target = 180 NEAR -> buy 80 NEAR worth

        let total_value = NearValue::from_near(BigDecimal::from(300));

        // Token A
        let token_a_current = NearValue::from_near(BigDecimal::from(200));
        let token_a_weight = 0.4;
        let token_a_target = &total_value * token_a_weight;
        let token_a_diff = &token_a_target - &token_a_current;

        // f64 は 0.4 を正確に表現できないため、tolerance-based で比較
        let target_a_f64 = token_a_target.as_bigdecimal().to_f64().unwrap();
        assert!(
            (target_a_f64 - 120.0).abs() < 0.0001,
            "Token A target should be ~120 NEAR, got {}",
            target_a_f64
        );

        let diff_a_f64 = token_a_diff.as_bigdecimal().to_f64().unwrap();
        assert!(
            (diff_a_f64 - (-80.0)).abs() < 0.0001,
            "Token A diff should be ~-80 NEAR, got {}",
            diff_a_f64
        );
        assert!(token_a_diff < NearValue::zero()); // Need to sell

        // Token B
        let token_b_current = NearValue::from_near(BigDecimal::from(100));
        let token_b_weight = 0.6;
        let token_b_target = &total_value * token_b_weight;
        let token_b_diff = &token_b_target - &token_b_current;

        let target_b_f64 = token_b_target.as_bigdecimal().to_f64().unwrap();
        assert!(
            (target_b_f64 - 180.0).abs() < 0.0001,
            "Token B target should be ~180 NEAR, got {}",
            target_b_f64
        );

        let diff_b_f64 = token_b_diff.as_bigdecimal().to_f64().unwrap();
        assert!(
            (diff_b_f64 - 80.0).abs() < 0.0001,
            "Token B diff should be ~80 NEAR, got {}",
            diff_b_f64
        );
        assert!(token_b_diff > NearValue::zero()); // Need to buy

        // Verify balance: sell amount ~= buy amount (within tolerance)
        let sell_amount = token_a_diff.abs().as_bigdecimal().to_f64().unwrap();
        let buy_amount = token_b_diff.as_bigdecimal().to_f64().unwrap();
        assert!(
            (sell_amount - buy_amount).abs() < 0.0001,
            "Sell and buy amounts should match: sell={}, buy={}",
            sell_amount,
            buy_amount
        );
    }

    #[test]
    fn test_rate_conversion_accuracy() {
        // Test precise conversion with realistic values
        // 1 Token = 2.5 NEAR (price = 2.5 NEAR/token)
        // raw_rate = 1e24 / 2.5 = 4e23 smallest_units/NEAR
        let rate = ExchangeRate::from_raw_rate(
            BigDecimal::from_str("400000000000000000000000").unwrap(), // 4e23
            24,
        );

        // Selling: 50 NEAR worth
        // token_amount = 50 NEAR × 4e23 = 2e25 = 20e24 = 20 tokens
        let wrap_near_value = NearValue::from_near(BigDecimal::from(50));
        let token_amount: TokenAmount = &wrap_near_value * &rate;

        // Expected: 20 tokens = 20e24 smallest units
        let expected = BigDecimal::from_str("20000000000000000000000000").unwrap();
        assert_eq!(token_amount.smallest_units(), &expected);

        // Verify roundtrip:
        // value * rate = amount (NearValue → TokenAmount, 乗算)
        // amount / rate = value (TokenAmount → NearValue, 除算)
        let reverse_value: NearValue = &token_amount / &rate;
        assert_eq!(reverse_value, wrap_near_value);
    }

    #[test]
    fn test_phase2_purchase_amount_adjustment() {
        // Scenario: Phase 2 needs to buy 3 tokens for total 300 wrap.near
        // But only 100 wrap.near is available after Phase 1
        // Should adjust all purchase amounts proportionally by factor 100/300 = 1/3

        let available_wrap_near = BigDecimal::from(100);
        let buy_operations = [
            BigDecimal::from(100), // Token A
            BigDecimal::from(100), // Token B
            BigDecimal::from(100), // Token C
        ];

        let total_buy_amount: BigDecimal = buy_operations.iter().sum();
        assert_eq!(total_buy_amount, BigDecimal::from(300));

        // Calculate adjustment factor
        let adjustment_factor = &available_wrap_near / &total_buy_amount;
        // Should be approximately 1/3
        let expected_min = BigDecimal::from_str("0.333").unwrap();
        let expected_max = BigDecimal::from_str("0.334").unwrap();
        assert!(adjustment_factor >= expected_min && adjustment_factor <= expected_max);

        // Apply adjustment to each purchase
        let adjusted_operations: Vec<BigDecimal> = buy_operations
            .iter()
            .map(|amount| amount * &adjustment_factor)
            .collect();

        // Each should be adjusted to ~33.33 wrap.near
        for adjusted in &adjusted_operations {
            assert!(
                adjusted > &BigDecimal::from_str("33.33").unwrap()
                    && adjusted < &BigDecimal::from_str("33.34").unwrap()
            );
        }

        // Total should approximately equal available balance (within rounding error)
        let adjusted_total: BigDecimal = adjusted_operations.iter().sum();
        let tolerance = BigDecimal::from_str("0.01").unwrap(); // Allow 0.01 tolerance
        let diff = (&adjusted_total - &available_wrap_near).abs();
        assert!(
            diff < tolerance,
            "Adjusted total {} should be close to available {}",
            adjusted_total,
            available_wrap_near
        );
    }

    #[test]
    fn test_phase2_no_adjustment_needed() {
        // Scenario: Available wrap.near (200) >= total buy amount (150)
        // No adjustment should be applied

        let available_wrap_near = BigDecimal::from(200);
        let buy_operations = vec![
            BigDecimal::from(50),
            BigDecimal::from(50),
            BigDecimal::from(50),
        ];

        let total_buy_amount: BigDecimal = buy_operations.iter().sum();
        assert_eq!(total_buy_amount, BigDecimal::from(150));

        // No adjustment needed
        assert!(total_buy_amount <= available_wrap_near);

        // Adjustment factor would be >= 1
        let adjustment_factor = &available_wrap_near / &total_buy_amount;
        assert!(adjustment_factor >= 1);

        // In this case, we use the original amounts
        let adjusted_operations = if total_buy_amount > available_wrap_near {
            buy_operations
                .iter()
                .map(|amount| amount * &adjustment_factor)
                .collect()
        } else {
            buy_operations.clone()
        };

        // Amounts should remain unchanged
        assert_eq!(adjusted_operations, buy_operations);
    }

    #[test]
    fn test_phase2_extreme_shortage() {
        // Scenario: Severe shortage - only 1 wrap.near available for 1000 wrap.near needed
        // Adjustment factor = 0.001

        let available_wrap_near = BigDecimal::from(1);
        let buy_operations = [
            BigDecimal::from(400),
            BigDecimal::from(300),
            BigDecimal::from(300),
        ];

        let total_buy_amount: BigDecimal = buy_operations.iter().sum();
        assert_eq!(total_buy_amount, BigDecimal::from(1000));

        let adjustment_factor = &available_wrap_near / &total_buy_amount;
        assert_eq!(adjustment_factor, BigDecimal::from_str("0.001").unwrap());

        // Apply adjustment
        let adjusted_operations: Vec<BigDecimal> = buy_operations
            .iter()
            .map(|amount| amount * &adjustment_factor)
            .collect();

        // Proportions should be maintained
        assert_eq!(adjusted_operations[0], BigDecimal::from_str("0.4").unwrap());
        assert_eq!(adjusted_operations[1], BigDecimal::from_str("0.3").unwrap());
        assert_eq!(adjusted_operations[2], BigDecimal::from_str("0.3").unwrap());

        // Total should equal available balance
        let adjusted_total: BigDecimal = adjusted_operations.iter().sum();
        assert_eq!(adjusted_total, available_wrap_near);
    }

    #[test]
    fn test_small_rate_scaling_issue() {
        // Test: Very small rates can become 0 when converted to u128
        // This happens for expensive tokens with few decimals
        use num_bigint::ToBigInt;

        // Case 1: Normal rate (token worth 0.001 NEAR, 18 decimals)
        // rate = 1e18 / 1e26 = 1e-8
        let rate_normal = BigDecimal::from_str("0.00000001").unwrap();
        let scale = BigDecimal::from_str("1000000000000000000000000").unwrap(); // 1e24
        let scaled_normal = &rate_normal * &scale;
        let bigint_normal = scaled_normal.to_bigint().unwrap();
        println!(
            "Normal rate: {} -> scaled: {} -> bigint: {}",
            rate_normal, scaled_normal, bigint_normal
        );
        assert!(
            bigint_normal > num_bigint::BigInt::from(0),
            "Normal rate should not become 0"
        );

        // Case 2: Problematic rate (expensive token with 0 decimals, worth 2 NEAR)
        // rate = 50 / 1e26 = 5e-25
        let rate_problem = BigDecimal::from_str("0.0000000000000000000000005").unwrap();
        let scaled_problem = &rate_problem * &scale;
        let bigint_problem = scaled_problem.to_bigint();
        println!(
            "Problem rate: {} -> scaled: {} -> bigint: {:?}",
            rate_problem, scaled_problem, bigint_problem
        );

        // This test documents the known issue: small rates become 0
        // The bigint should be Some(0) or the scaled value should be < 1
        if let Some(bi) = bigint_problem {
            println!("WARNING: Very small rate results in bigint = {}", bi);
            // If this is 0, we have a precision issue
            if bi == num_bigint::BigInt::from(0) {
                println!(
                    "ISSUE CONFIRMED: Rate {} scaled to {} truncates to 0",
                    rate_problem, scaled_problem
                );
            }
        }

        // Case 3: Edge case - rate exactly at boundary
        // rate × 1e24 = 1 -> rate = 1e-24
        let rate_boundary = BigDecimal::from_str("0.000000000000000000000001").unwrap();
        let scaled_boundary = &rate_boundary * &scale;
        let bigint_boundary = scaled_boundary.to_bigint().unwrap();
        println!(
            "Boundary rate: {} -> scaled: {} -> bigint: {}",
            rate_boundary, scaled_boundary, bigint_boundary
        );
        assert_eq!(
            bigint_boundary,
            num_bigint::BigInt::from(1),
            "Boundary rate should be exactly 1"
        );
    }
}
