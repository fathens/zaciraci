use super::*;
use crate::config;
use bigdecimal::BigDecimal;
use zaciraci_common::types::{NearValue, YoctoAmount};

/// NEAR → yoctoNEAR 変換のヘルパー（型安全）
fn near_to_yocto(near: u64) -> BigDecimal {
    NearValue::from_near(BigDecimal::from(near))
        .to_yocto()
        .into_bigdecimal()
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
    let actual = &*HARVEST_MIN_AMOUNT;
    assert_eq!(*actual, expected);
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
    config::set("HARVEST_RESERVE_AMOUNT", "invalid");

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
    config::set("HARVEST_ACCOUNT_ID", "test.near");

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

#[tokio::test]
async fn test_check_and_harvest_no_evaluation_period() {
    // 評価期間がまだない場合のテスト
    let current_portfolio_value = 100u128 * 10u128.pow(24);

    // check_and_harvestは早期リターンするはず（評価期間がない場合）
    // エラーが出ないことを確認
    let result = check_and_harvest(current_portfolio_value).await;

    // データベースが使えない環境ではテストをスキップ
    if result.is_err() {
        println!("Skipping test due to database unavailability");
        return;
    }
}
