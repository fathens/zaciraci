use bigdecimal::{BigDecimal, Zero};
use common::types::{ExchangeRate, NearValue, TokenAmount};
use std::str::FromStr;

#[test]
fn test_rebalance_calculations_sell_only() {
    let current_value = NearValue::from_near(BigDecimal::from(200));
    let target_value = NearValue::from_near(BigDecimal::from(100));
    let diff = &target_value - &current_value;

    assert_eq!(diff, NearValue::from_near(BigDecimal::from(-100)));
    assert!(diff < NearValue::zero());

    let rate = ExchangeRate::from_raw_rate(
        BigDecimal::from_str("500000000000000000000000").unwrap(),
        24,
    );
    let token_amount: TokenAmount = &diff.abs() * &rate;

    let expected = BigDecimal::from_str("50000000000000000000000000").unwrap();
    assert_eq!(token_amount.smallest_units(), &expected);
}

#[test]
fn test_rebalance_calculations_buy_only() {
    let current_value = NearValue::from_near(BigDecimal::from(50));
    let target_value = NearValue::from_near(BigDecimal::from(100));
    let diff = &target_value - &current_value;

    assert_eq!(diff, NearValue::from_near(BigDecimal::from(50)));
    assert!(diff > NearValue::zero());

    let wrap_near_amount = diff;
    assert_eq!(wrap_near_amount, NearValue::from_near(BigDecimal::from(50)));
}

#[test]
fn test_rebalance_minimum_trade_size() {
    let min_trade_size = NearValue::one();

    let small_diff = NearValue::from_near(BigDecimal::from_str("0.5").unwrap());
    assert!(small_diff < min_trade_size);

    let large_diff = NearValue::from_near(BigDecimal::from(2));
    assert!(large_diff >= min_trade_size);
}

#[test]
fn test_token_amount_conversion() {
    let wrap_near_value = NearValue::from_near(BigDecimal::from(100));
    let rate = ExchangeRate::from_raw_rate(
        BigDecimal::from_str("500000000000000000000000").unwrap(),
        24,
    );
    let token_amount: TokenAmount = &wrap_near_value * &rate;

    let expected = BigDecimal::from_str("50000000000000000000000000").unwrap();
    assert_eq!(token_amount.smallest_units(), &expected);
}

#[test]
fn test_wrap_near_value_calculation() {
    let balance = TokenAmount::from_smallest_units(
        BigDecimal::from_str("100000000000000000000000000").unwrap(),
        24,
    );
    let rate = ExchangeRate::from_raw_rate(
        BigDecimal::from_str("500000000000000000000000").unwrap(),
        24,
    );
    let value: NearValue = &balance / &rate;

    assert_eq!(value, NearValue::from_near(BigDecimal::from(200)));
}

#[test]
fn test_two_phase_rebalance_scenario() {
    let total_value = NearValue::from_near(BigDecimal::from(300));

    let token_a_current = NearValue::from_near(BigDecimal::from(200));
    let token_a_weight = BigDecimal::from_str("0.4").unwrap();
    let token_a_target = &total_value * &token_a_weight;
    let token_a_diff = &token_a_target - &token_a_current;

    assert_eq!(
        token_a_target,
        NearValue::from_near(BigDecimal::from(120)),
        "Token A target should be 120 NEAR",
    );
    assert_eq!(
        token_a_diff,
        NearValue::from_near(BigDecimal::from(-80)),
        "Token A diff should be -80 NEAR",
    );
    assert!(token_a_diff < NearValue::zero());

    let token_b_current = NearValue::from_near(BigDecimal::from(100));
    let token_b_weight = BigDecimal::from_str("0.6").unwrap();
    let token_b_target = &total_value * &token_b_weight;
    let token_b_diff = &token_b_target - &token_b_current;

    assert_eq!(
        token_b_target,
        NearValue::from_near(BigDecimal::from(180)),
        "Token B target should be 180 NEAR",
    );
    assert_eq!(
        token_b_diff,
        NearValue::from_near(BigDecimal::from(80)),
        "Token B diff should be 80 NEAR",
    );
    assert!(token_b_diff > NearValue::zero());

    assert_eq!(
        token_a_diff.abs(),
        token_b_diff,
        "Sell and buy amounts should match",
    );
}

#[test]
fn test_rate_conversion_accuracy() {
    let rate = ExchangeRate::from_raw_rate(
        BigDecimal::from_str("400000000000000000000000").unwrap(),
        24,
    );

    let wrap_near_value = NearValue::from_near(BigDecimal::from(50));
    let token_amount: TokenAmount = &wrap_near_value * &rate;

    let expected = BigDecimal::from_str("20000000000000000000000000").unwrap();
    assert_eq!(token_amount.smallest_units(), &expected);

    let reverse_value: NearValue = &token_amount / &rate;
    assert_eq!(reverse_value, wrap_near_value);
}

#[test]
fn test_phase2_purchase_amount_adjustment() {
    let available_wrap_near = BigDecimal::from(100);
    let buy_operations = [
        BigDecimal::from(100),
        BigDecimal::from(100),
        BigDecimal::from(100),
    ];

    let total_buy_amount: BigDecimal = buy_operations.iter().sum();
    assert_eq!(total_buy_amount, BigDecimal::from(300));

    let adjustment_factor = &available_wrap_near / &total_buy_amount;
    let expected_min = BigDecimal::from_str("0.333").unwrap();
    let expected_max = BigDecimal::from_str("0.334").unwrap();
    assert!(adjustment_factor >= expected_min && adjustment_factor <= expected_max);

    let adjusted_operations: Vec<BigDecimal> = buy_operations
        .iter()
        .map(|amount| amount * &adjustment_factor)
        .collect();

    for adjusted in &adjusted_operations {
        assert!(
            adjusted > &BigDecimal::from_str("33.33").unwrap()
                && adjusted < &BigDecimal::from_str("33.34").unwrap()
        );
    }

    let adjusted_total: BigDecimal = adjusted_operations.iter().sum();
    let tolerance = BigDecimal::from_str("0.01").unwrap();
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
    let available_wrap_near = BigDecimal::from(200);
    let buy_operations = vec![
        BigDecimal::from(50),
        BigDecimal::from(50),
        BigDecimal::from(50),
    ];

    let total_buy_amount: BigDecimal = buy_operations.iter().sum();
    assert_eq!(total_buy_amount, BigDecimal::from(150));

    assert!(total_buy_amount <= available_wrap_near);

    let adjustment_factor = &available_wrap_near / &total_buy_amount;
    assert!(adjustment_factor >= 1);

    let adjusted_operations = if total_buy_amount > available_wrap_near {
        buy_operations
            .iter()
            .map(|amount| amount * &adjustment_factor)
            .collect()
    } else {
        buy_operations.clone()
    };

    assert_eq!(adjusted_operations, buy_operations);
}

#[test]
fn test_phase2_extreme_shortage() {
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

    let adjusted_operations: Vec<BigDecimal> = buy_operations
        .iter()
        .map(|amount| amount * &adjustment_factor)
        .collect();

    assert_eq!(adjusted_operations[0], BigDecimal::from_str("0.4").unwrap());
    assert_eq!(adjusted_operations[1], BigDecimal::from_str("0.3").unwrap());
    assert_eq!(adjusted_operations[2], BigDecimal::from_str("0.3").unwrap());

    let adjusted_total: BigDecimal = adjusted_operations.iter().sum();
    assert_eq!(adjusted_total, available_wrap_near);
}

#[test]
fn test_small_rate_scaling_issue() {
    let rate_normal = ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6);
    assert!(
        !rate_normal.is_effectively_zero(),
        "Normal rate should be tradeable"
    );

    let rate_problem = ExchangeRate::from_raw_rate(BigDecimal::from_str("0.5").unwrap(), 0);
    assert!(
        rate_problem.is_effectively_zero(),
        "Rate < 1 should be effectively zero (untradeable)"
    );

    let rate_boundary = ExchangeRate::from_raw_rate(BigDecimal::from(1), 0);
    assert!(
        !rate_boundary.is_effectively_zero(),
        "Rate = 1 should be tradeable"
    );

    let rate_zero = ExchangeRate::from_raw_rate(BigDecimal::zero(), 0);
    assert!(
        rate_zero.is_effectively_zero(),
        "Zero rate should be effectively zero"
    );
}

#[test]
fn test_rounding_remainder_three_way_split() {
    // 3等分で割り切れない yoctoNEAR 値の端数回収テスト
    // allocate_add_position_amounts と同パターンで、
    // 最後の要素が saturating_sub で端数を回収する
    let available: u128 = 10_000_000_000_000_000_000_000_001; // 10 NEAR + 1 yocto
    let ratio = BigDecimal::from_str("0.5").unwrap(); // 半分に按分

    let buy_values = [
        NearValue::from_near(BigDecimal::from(10)),
        NearValue::from_near(BigDecimal::from(5)),
        NearValue::from_near(BigDecimal::from(5)),
    ];

    let mut allocated_sum: u128 = 0;
    let mut amounts = Vec::new();
    let buy_count = buy_values.len();

    for (i, buy) in buy_values.iter().enumerate() {
        let is_last = i == buy_count - 1;
        let amount = if is_last {
            available.saturating_sub(allocated_sum)
        } else {
            let adjusted = buy * &ratio;
            adjusted.to_yocto().to_amount().to_u128()
        };
        allocated_sum = allocated_sum.saturating_add(amount);
        amounts.push(amount);
    }

    // 不変条件: 合計が available に一致
    let total: u128 = amounts.iter().sum();
    assert_eq!(
        total, available,
        "total allocated must equal available balance"
    );

    // 最後の要素が端数を含む
    // buy[0] = 10 * 0.5 = 5 NEAR, buy[1] = 5 * 0.5 = 2.5 NEAR
    // buy[2] = available - sum(buy[0..2])
    assert!(amounts[2] > 0, "last element should collect remainder");
}

#[test]
fn test_rounding_remainder_indivisible_yocto() {
    // 7 yocto を 3 等分: floor(7/3)=2, floor(7/3)=2, remainder=3
    let available: u128 = 7;
    let total_buy = NearValue::from_near(BigDecimal::from(3));
    let ratio = &NearValue::from_near(BigDecimal::from_str("0.000000000000000000000007").unwrap())
        / &total_buy;

    let buy_values = [
        NearValue::from_near(BigDecimal::from(1)),
        NearValue::from_near(BigDecimal::from(1)),
        NearValue::from_near(BigDecimal::from(1)),
    ];

    let mut allocated_sum: u128 = 0;
    let mut amounts = Vec::new();
    let buy_count = buy_values.len();

    for (i, buy) in buy_values.iter().enumerate() {
        let is_last = i == buy_count - 1;
        let amount = if is_last {
            available.saturating_sub(allocated_sum)
        } else {
            let adjusted = buy * &ratio;
            adjusted.to_yocto().to_amount().to_u128()
        };
        allocated_sum = allocated_sum.saturating_add(amount);
        amounts.push(amount);
    }

    let total: u128 = amounts.iter().sum();
    assert_eq!(
        total, available,
        "total must equal available even for small indivisible values"
    );
}
