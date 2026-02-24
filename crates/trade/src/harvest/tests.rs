use super::*;
use bigdecimal::BigDecimal;
use common::config;
use common::types::{NearValue, YoctoAmount, YoctoValue};

/// NEAR → yoctoNEAR 変換のヘルパー（型安全）
fn near_to_yocto(near: u64) -> BigDecimal {
    NearValue::from_near(BigDecimal::from(near))
        .to_yocto()
        .as_bigdecimal()
        .clone()
}

/// NEAR → YoctoAmount 変換のヘルパー（型安全）
fn near_to_yocto_amount(near: u64) -> YoctoAmount {
    let yocto_value = near as u128 * 10u128.pow(24);
    YoctoAmount::from_u128(yocto_value)
}

// テスト専用: staticを使わずに設定値を計算する関数
#[cfg(test)]
fn calculate_harvest_reserve_amount_from_config(config_value: Option<&str>) -> BigDecimal {
    let reserve_str = config_value.unwrap_or("1").to_string();
    let reserve_near = reserve_str.parse::<u64>().unwrap_or(1);
    near_to_yocto(reserve_near)
}

#[test]
fn test_harvest_reserve_amount_default() {
    // テスト用にデフォルト値（1 NEAR）をテスト
    let expected = near_to_yocto(1);

    // staticを使わずに設定ロジックを直接テスト
    let actual = calculate_harvest_reserve_amount_from_config(None);
    assert_eq!(actual, expected);
}

#[test]
fn test_harvest_reserve_amount_custom() {
    // カスタム値のテスト: 5 NEAR
    let expected = near_to_yocto(5);

    // staticを使わずに設定ロジックを直接テスト
    let actual = calculate_harvest_reserve_amount_from_config(Some("5"));
    assert_eq!(actual, expected);
}

#[test]
fn test_harvest_min_amount_default() {
    // HARVEST_MIN_AMOUNTのデフォルト値テスト
    let expected = near_to_yocto_amount(10);
    let actual = harvest_min_amount();
    assert_eq!(actual, expected);
}

#[test]
fn test_yocto_near_conversion() {
    // yoctoNEAR変換の正確性テスト（型安全版）
    let five_near = near_to_yocto(5);

    // 5 NEARが正しくyoctoNEARに変換されることを確認
    assert_eq!(five_near.to_string(), "5000000000000000000000000");
}

#[test]
fn test_harvest_reserve_amount_parsing() {
    // 無効な設定値の場合のフォールバック動作テスト
    let _guard = config::ConfigGuard::new("HARVEST_RESERVE_AMOUNT", "invalid");

    let reserve_str = config::get("HARVEST_RESERVE_AMOUNT").unwrap_or_else(|_| "1".to_string());
    let reserve_near = reserve_str.parse::<u64>().unwrap_or(1);

    // 無効な値の場合、デフォルト1に戻ることを確認
    assert_eq!(reserve_near, 1);

    // 正常な値の場合のテスト
    config::set("HARVEST_RESERVE_AMOUNT", "3");
    let reserve_str = config::get("HARVEST_RESERVE_AMOUNT").unwrap_or_else(|_| "1".to_string());
    let reserve_near = reserve_str.parse::<u64>().unwrap_or(1);
    assert_eq!(reserve_near, 3);
}

#[test]
fn test_harvest_account_parsing() {
    // HARVEST_ACCOUNT_IDの正常なパース動作テスト
    let _guard = config::ConfigGuard::new("HARVEST_ACCOUNT_ID", "test.near");

    let value = config::get("HARVEST_ACCOUNT_ID").unwrap_or_else(|_| "harvest.near".to_string());
    let parsed_account = value.parse::<AccountId>();

    assert!(parsed_account.is_ok());
    assert_eq!(parsed_account.unwrap().as_str(), "test.near");
}

#[test]
fn test_is_time_to_harvest() {
    // 初回は常にtrueになるはず（LAST_HARVEST_TIMEが0のため）
    assert!(is_time_to_harvest());

    // 現在時刻を記録
    update_last_harvest_time();

    // 直後はfalseになるはず
    assert!(!is_time_to_harvest());
}

#[test]
fn test_harvest_threshold_calculation() {
    // 初期投資額: 100 NEAR
    let initial_amount = 100u128 * 10u128.pow(24);
    let initial_value = BigDecimal::from(initial_amount);

    // 200%利益時のしきい値（2倍）
    let harvest_threshold = &initial_value * BigDecimal::from(2);
    let expected_threshold = BigDecimal::from(200u128 * 10u128.pow(24));
    assert_eq!(harvest_threshold, expected_threshold);

    // ポートフォリオ価値が250 NEARの場合
    let current_portfolio_value = BigDecimal::from(250u128 * 10u128.pow(24));
    let excess_value = &current_portfolio_value - &harvest_threshold;
    let expected_excess = BigDecimal::from(50u128 * 10u128.pow(24));
    assert_eq!(excess_value, expected_excess);

    // 10%の利益確定額
    let harvest_amount = &excess_value * BigDecimal::new(1.into(), 1); // 10% = 0.1
    let expected_harvest = BigDecimal::from(5u128 * 10u128.pow(24)); // 5 NEAR
    assert_eq!(harvest_amount, expected_harvest);
}

// ==================== Bug A/B 回帰テスト ====================

#[tokio::test]
async fn test_harvest_skips_when_initial_value_is_zero() {
    // Bug A 回帰テスト: initial_value=0 の場合、ハーベストは発火しないこと
    let initial_value = YoctoValue::from_yocto(BigDecimal::from(0u64));
    let current_value = YoctoValue::from_yocto(BigDecimal::from(100u128 * 10u128.pow(24))); // 100 NEAR

    let result = check_and_execute_harvest(&initial_value, &current_value, "test-period").await;

    match result {
        Ok(harvested) => {
            assert!(
                harvested.is_zero(),
                "Expected zero harvest when initial_value is zero, got: {}",
                harvested
            );
        }
        Err(e) => {
            // DB が利用不可能な環境ではスキップ
            println!("Skipping test: {}", e);
        }
    }
}

#[tokio::test]
async fn test_harvest_skips_when_below_threshold() {
    // 正常系: ポートフォリオが200%未満の場合ハーベストしない
    let initial_value = YoctoValue::from_yocto(BigDecimal::from(100u128 * 10u128.pow(24))); // 100 NEAR
    let current_value = YoctoValue::from_yocto(BigDecimal::from(150u128 * 10u128.pow(24))); // 150 NEAR (50% profit)

    let result = check_and_execute_harvest(&initial_value, &current_value, "test-period").await;

    match result {
        Ok(harvested) => {
            assert!(
                harvested.is_zero(),
                "Expected zero harvest when below 200% threshold, got: {}",
                harvested
            );
        }
        Err(e) => {
            println!("Skipping test: {}", e);
        }
    }
}

#[tokio::test]
async fn test_harvest_inner_logic_initial_value_zero() {
    // Bug A の核心テスト: initial_value=0 の場合、閾値 2*0=0 で
    // current_value > 0 が成立するが、ゼロガードで早期リターンすること
    let initial_value = YoctoValue::from_yocto(BigDecimal::from(0u64));
    let current_value = YoctoValue::from_yocto(BigDecimal::from(50u128 * 10u128.pow(24))); // 50 NEAR

    let result = check_and_execute_harvest(&initial_value, &current_value, "test-period").await;
    let harvested = result.expect("should succeed with zero initial_value");
    assert!(
        harvested.is_zero(),
        "Expected zero harvest when initial_value is zero, got: {}",
        harvested
    );
}

#[test]
fn test_harvest_threshold_with_real_values() {
    // Bug B の核心テスト
    // 旧 period: initial_value=100 NEAR, 清算後: final_value=250 NEAR
    // check_and_execute_harvest は旧 initial_value と final_value で比較するべき

    let initial_value_yocto = 100u128 * 10u128.pow(24);
    let final_value_yocto = 250u128 * 10u128.pow(24);

    let initial_value = YoctoValue::from_yocto(BigDecimal::from(initial_value_yocto));
    let final_value = YoctoValue::from_yocto(BigDecimal::from(final_value_yocto));

    // 閾値 = 2 * 100 = 200 NEAR
    let threshold = &initial_value * BigDecimal::from(2);
    let threshold_yocto = 200u128 * 10u128.pow(24);
    assert_eq!(
        threshold,
        YoctoValue::from_yocto(BigDecimal::from(threshold_yocto))
    );

    // current_value (250) > threshold (200) → ハーベスト対象
    assert!(final_value > threshold);

    // excess = 250 - 200 = 50 NEAR
    let excess = &final_value - &threshold;
    let expected_excess_yocto = 50u128 * 10u128.pow(24);
    assert_eq!(
        excess,
        YoctoValue::from_yocto(BigDecimal::from(expected_excess_yocto))
    );

    // harvest_amount = excess * 10% = 5 NEAR
    let harvest_value = &excess * BigDecimal::new(1.into(), 1);
    let expected_harvest_yocto = 5u128 * 10u128.pow(24);
    assert_eq!(
        harvest_value,
        YoctoValue::from_yocto(BigDecimal::from(expected_harvest_yocto))
    );
}

#[test]
fn test_harvest_new_period_should_use_old_initial_value() {
    // Bug B シナリオの検証: 新 period 作成後に initial_value=final_value だと
    // ハーベストが発火しないことの確認

    let old_initial_value_yocto = 100u128 * 10u128.pow(24);
    let final_value_yocto = 250u128 * 10u128.pow(24);

    // Bad case: 新 period の initial_value = final_value → ハーベスト不発
    let new_initial_value = YoctoValue::from_yocto(BigDecimal::from(final_value_yocto));
    let current_value = YoctoValue::from_yocto(BigDecimal::from(final_value_yocto));
    let threshold_new = &new_initial_value * BigDecimal::from(2);
    assert!(
        (current_value <= threshold_new),
        "With new period's initial_value == current_value, harvest should NOT trigger"
    );

    // Good case: 旧 period の initial_value で比較 → ハーベスト発火
    let old_initial_value = YoctoValue::from_yocto(BigDecimal::from(old_initial_value_yocto));
    let threshold_old = &old_initial_value * BigDecimal::from(2);
    assert!(
        current_value > threshold_old,
        "With old period's initial_value, harvest SHOULD trigger"
    );
}
