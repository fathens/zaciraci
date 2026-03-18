use super::*;
use std::str::FromStr;

fn bd(s: &str) -> BigDecimal {
    BigDecimal::from_str(s).unwrap()
}

#[test]
fn test_single_add_position_uses_full_balance() {
    let result = allocate_add_position_amounts(&[(0, bd("1.0"))], 1_000_000);
    assert_eq!(result, vec![(0, 1_000_000)]);
}

#[test]
fn test_two_equal_weights_split_evenly() {
    let result = allocate_add_position_amounts(&[(0, bd("0.5")), (1, bd("0.5"))], 1_000_000);
    assert_eq!(result[0].1 + result[1].1, 1_000_000);
    assert_eq!(result[0].1, 500_000);
    assert_eq!(result[1].1, 500_000);
}

#[test]
fn test_last_gets_remainder() {
    let balance: u128 = 19_020_000_000_000_000_000_000_000;
    let result = allocate_add_position_amounts(
        &[(0, bd("0.245")), (1, bd("0.332")), (2, bd("0.423"))],
        balance,
    );
    let total: u128 = result.iter().map(|(_, a)| a).sum();
    assert_eq!(total, balance);
}

#[test]
fn test_unequal_weights() {
    let balance: u128 = 10_000;
    let result =
        allocate_add_position_amounts(&[(0, bd("0.1")), (1, bd("0.2")), (2, bd("0.7"))], balance);
    assert_eq!(result[0].1, 1_000); // 10%
    assert_eq!(result[1].1, 2_000); // 20%
    assert_eq!(result[2].1, 7_000); // 残額 = 70%
}

#[test]
fn test_zero_balance() {
    let result = allocate_add_position_amounts(&[(0, bd("0.5")), (1, bd("0.5"))], 0);
    assert_eq!(result[0].1, 0);
    assert_eq!(result[1].1, 0);
}

#[test]
fn test_empty_add_positions() {
    let result = allocate_add_position_amounts(&[], 1_000_000);
    assert!(result.is_empty());
}

#[test]
fn test_preserves_action_indices() {
    let result = allocate_add_position_amounts(&[(1, bd("0.4")), (3, bd("0.6"))], 1_000_000);
    assert_eq!(result[0].0, 1);
    assert_eq!(result[1].0, 3);
}

/// 大きな yocto 値で精度が保たれることを検証
#[test]
fn test_allocate_large_yocto_precision() {
    let balance: u128 = 100_000_000_000_000_000_000_000_000;
    let result = allocate_add_position_amounts(&[(0, bd("0.3")), (1, bd("0.7"))], balance);

    let total: u128 = result.iter().map(|(_, a)| a).sum();
    assert_eq!(total, balance);

    let expected_0 = 30_000_000_000_000_000_000_000_000u128;
    let diff_0 = (result[0].1 as i128 - expected_0 as i128).unsigned_abs();
    assert!(
        diff_0 < 1_000_000_000_000_000_000_000_000,
        "precision loss too large: diff = {} yocto",
        diff_0
    );
}

/// 小さなバランス値で整数除算の切り捨て誤差が軽減されることを検証
#[test]
fn test_allocate_small_balance_precision() {
    let result =
        allocate_add_position_amounts(&[(0, bd("0.3")), (1, bd("0.3")), (2, bd("0.4"))], 100);
    assert_eq!(result[0].1, 30, "30% of 100 should be 30");
    assert_eq!(result[1].1, 30, "30% of 100 should be 30");
    assert_eq!(result[2].1, 40, "remainder should be 40");
}

/// balance が total_bps より小さい場合でも正しく配分される
#[test]
fn test_allocate_balance_smaller_than_total_bps() {
    let result = allocate_add_position_amounts(&[(0, bd("0.5")), (1, bd("0.5"))], 7);
    assert_eq!(result[0].1, 3, "50% of 7 should be ~3");
    assert_eq!(result[1].1, 4, "remainder should be 4");
    assert_eq!(result[0].1 + result[1].1, 7, "total should equal balance");
}
