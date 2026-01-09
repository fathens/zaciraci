use super::*;
use std::str::FromStr;

#[test]
fn test_price_arithmetic() {
    let p1 = TokenPrice::new(BigDecimal::from(10));
    let p2 = TokenPrice::new(BigDecimal::from(3));

    // 加算
    let sum = p1.clone() + p2.clone();
    assert_eq!(sum.0, BigDecimal::from(13));

    // 減算
    let diff = p1.clone() - p2.clone();
    assert_eq!(diff.0, BigDecimal::from(7));

    // 除算（比率を返す）
    let ratio = p1.clone() / p2.clone();
    assert!(ratio > BigDecimal::from(3) && ratio < BigDecimal::from(4));

    // スカラー乗算
    let scaled = p1.clone() * 2.0;
    assert_eq!(scaled.0, BigDecimal::from(20));
}

#[test]
fn test_price_f64_arithmetic() {
    let p1 = PriceF64::new(10.0);
    let p2 = PriceF64::new(3.0);

    // 加算
    let sum = p1 + p2;
    assert!((sum.0 - 13.0).abs() < 1e-10);

    // 減算
    let diff = p1 - p2;
    assert!((diff.0 - 7.0).abs() < 1e-10);

    // 除算（比率を返す）
    let ratio = p1 / p2;
    assert!((ratio - 3.333333).abs() < 0.001);
}

#[test]
fn test_yocto_amount_arithmetic() {
    let a1 = YoctoAmount::new(1000);
    let a2 = YoctoAmount::new(300);

    // 加算
    let sum = a1.clone() + a2.clone();
    assert_eq!(sum.to_u128(), 1300);

    // 減算
    let diff = a1.clone() - a2.clone();
    assert_eq!(diff.to_u128(), 700);

    // 減算（アンダーフロー防止）
    let diff2 = a2 - a1;
    assert_eq!(diff2.to_u128(), 0);
}

#[test]
fn test_unit_conversion() {
    // 1 NEAR = 10^24 yoctoNEAR
    let yocto = YoctoAmount::new(YOCTO_PER_NEAR);
    let near = yocto.to_near();
    assert_eq!(near.as_bigdecimal(), &BigDecimal::from(1));

    // 逆変換
    let back_to_yocto = near.to_yocto();
    assert_eq!(back_to_yocto.to_u128(), YOCTO_PER_NEAR);
}

#[test]
fn test_price_times_amount() {
    // TokenPrice × YoctoAmount = YoctoValue
    let price = TokenPrice::new(BigDecimal::from_str("0.5").unwrap());
    let amount = YoctoAmount::new(1000);
    let value: YoctoValue = price.clone() * amount;
    assert_eq!(value.as_bigdecimal(), &BigDecimal::from(500));

    // TokenPrice × NearAmount = NearValue
    let near_amount = NearAmount::new(BigDecimal::from(2));
    let near_value: NearValue = price * near_amount;
    assert_eq!(near_value.as_bigdecimal(), &BigDecimal::from(1));
}

#[test]
fn test_value_divided_by_price() {
    // YoctoValue / TokenPrice = YoctoAmount
    let value = YoctoValue::new(BigDecimal::from(1000));
    let price = TokenPrice::new(BigDecimal::from(2));
    let amount: YoctoAmount = value / price;
    assert_eq!(amount.to_u128(), 500);
}

#[test]
fn test_value_divided_by_amount() {
    // YoctoValue / YoctoAmount = TokenPrice
    let value = YoctoValue::new(BigDecimal::from(1000));
    let amount = YoctoAmount::new(500);
    let price: TokenPrice = value / amount;
    assert_eq!(price.as_bigdecimal(), &BigDecimal::from(2));
}

#[test]
fn test_zero_division() {
    // ゼロ除算は安全にゼロを返す
    let p1 = TokenPrice::new(BigDecimal::from(10));
    let p2 = TokenPrice::zero();
    let ratio = p1 / p2;
    assert_eq!(ratio, BigDecimal::zero());

    let value = YoctoValue::new(BigDecimal::from(100));
    let zero_price = TokenPrice::zero();
    let amount: YoctoAmount = value / zero_price;
    assert_eq!(amount.to_u128(), 0);
}

#[test]
fn test_price_f64_conversion() {
    let price = TokenPrice::new(BigDecimal::from_str("123.456").unwrap());
    // to_f64() は直接 f64 を返す
    let price_f64_raw = price.to_f64();
    assert!((price_f64_raw - 123.456).abs() < 0.001);

    // to_price_f64() は PriceF64 を返す
    let price_f64 = price.to_price_f64();
    assert!((price_f64.as_f64() - 123.456).abs() < 0.001);

    let back_to_price = price_f64.to_bigdecimal();
    // 精度損失があるため、完全一致はしない
    assert!((back_to_price.as_bigdecimal().to_f64().unwrap() - 123.456).abs() < 0.001);
}

#[test]
fn test_price_serialization() {
    // TokenPrice のシリアライズ/デシリアライズ
    let price = TokenPrice::new(BigDecimal::from_str("123.456789").unwrap());
    let json = serde_json::to_string(&price).unwrap();
    let deserialized: TokenPrice = serde_json::from_str(&json).unwrap();
    assert_eq!(price, deserialized);

    // PriceF64 のシリアライズ/デシリアライズ
    let price_f64 = PriceF64::new(123.456);
    let json_f64 = serde_json::to_string(&price_f64).unwrap();
    let deserialized_f64: PriceF64 = serde_json::from_str(&json_f64).unwrap();
    assert!((price_f64.as_f64() - deserialized_f64.as_f64()).abs() < 1e-10);
}

#[test]
fn test_price_comparison() {
    let p1 = TokenPrice::new(BigDecimal::from(10));
    let p2 = TokenPrice::new(BigDecimal::from(20));
    let p3 = TokenPrice::new(BigDecimal::from(10));

    // PartialEq
    assert_eq!(p1, p3);
    assert_ne!(p1, p2);

    // PartialOrd/Ord
    assert!(p1 < p2);
    assert!(p2 > p1);
    assert!(p1 <= p3);
    assert!(p1 >= p3);

    // ソート可能
    let mut prices = [p2.clone(), p1.clone(), p3.clone()];
    prices.sort();
    assert_eq!(prices[0], p1);
    assert_eq!(prices[2], p2);
}

#[test]
fn test_price_display() {
    let price = TokenPrice::new(BigDecimal::from_str("123.456").unwrap());
    assert_eq!(format!("{}", price), "123.456");

    let price_f64 = PriceF64::new(123.456);
    assert!(format!("{}", price_f64).starts_with("123.45"));

    let near_amount = NearAmount::new(BigDecimal::from(5));
    assert_eq!(format!("{}", near_amount), "5 NEAR");

    let near_value = NearValue::new(BigDecimal::from(10));
    assert_eq!(format!("{}", near_value), "10 NEAR");
}

#[test]
fn test_is_zero_methods() {
    // TokenPrice
    assert!(TokenPrice::zero().is_zero());
    assert!(!TokenPrice::new(BigDecimal::from(1)).is_zero());

    // PriceF64
    assert!(PriceF64::zero().is_zero());
    assert!(!PriceF64::new(0.001).is_zero());

    // YoctoAmount
    assert!(YoctoAmount::zero().is_zero());
    assert!(!YoctoAmount::new(1).is_zero());

    // NearAmount
    assert!(NearAmount::zero().is_zero());
    assert!(!NearAmount::new(BigDecimal::from(1)).is_zero());

    // YoctoValue
    assert!(YoctoValue::zero().is_zero());
    assert!(!YoctoValue::new(BigDecimal::from(1)).is_zero());

    // NearValue
    assert!(NearValue::zero().is_zero());
    assert!(!NearValue::new(BigDecimal::from(1)).is_zero());
}

#[test]
fn test_near_value_arithmetic() {
    let v1 = NearValue::new(BigDecimal::from(100));
    let v2 = NearValue::new(BigDecimal::from(30));

    // 加算
    let sum = v1.clone() + v2.clone();
    assert_eq!(sum.as_bigdecimal(), &BigDecimal::from(130));

    // 減算
    let diff = v1.clone() - v2.clone();
    assert_eq!(diff.as_bigdecimal(), &BigDecimal::from(70));

    // 除算（比率を返す）
    let ratio = v1.clone() / v2.clone();
    assert!(ratio > BigDecimal::from(3) && ratio < BigDecimal::from(4));

    // NearValue / TokenPrice = NearAmount
    let price = TokenPrice::new(BigDecimal::from(2));
    let amount: NearAmount = v1.clone() / price;
    assert_eq!(amount.as_bigdecimal(), &BigDecimal::from(50));

    // NearValue / NearAmount = TokenPrice
    let near_amount = NearAmount::new(BigDecimal::from(50));
    let price_result: TokenPrice = v1 / near_amount;
    assert_eq!(price_result.as_bigdecimal(), &BigDecimal::from(2));
}

#[test]
fn test_yocto_value_conversion() {
    // YoctoValue → NearValue
    let yocto_value = YoctoValue::new(BigDecimal::from(YOCTO_PER_NEAR) * BigDecimal::from(5));
    let near_value = yocto_value.to_near();
    assert_eq!(near_value.as_bigdecimal(), &BigDecimal::from(5));

    // NearValue → YoctoValue
    let near_value2 = NearValue::new(BigDecimal::from(3));
    let yocto_value2 = near_value2.to_yocto();
    assert_eq!(
        yocto_value2.as_bigdecimal(),
        &(BigDecimal::from(YOCTO_PER_NEAR) * BigDecimal::from(3))
    );
}

#[test]
fn test_reference_arithmetic() {
    // TokenPrice の参照演算
    let p1 = TokenPrice::new(BigDecimal::from(10));
    let p2 = TokenPrice::new(BigDecimal::from(3));

    let diff = &p1 - &p2;
    assert_eq!(diff.as_bigdecimal(), &BigDecimal::from(7));

    // NearAmount の参照演算
    let a1 = NearAmount::new(BigDecimal::from(10));
    let a2 = NearAmount::new(BigDecimal::from(3));

    let sum = a1.clone() + &a2;
    assert_eq!(sum.as_bigdecimal(), &BigDecimal::from(13));

    let diff = a1 - &a2;
    assert_eq!(diff.as_bigdecimal(), &BigDecimal::from(7));
}

#[test]
fn test_price_edge_cases() {
    // 非常に小さい価格
    let tiny = TokenPrice::new(BigDecimal::from_str("0.000000000001").unwrap());
    assert!(!tiny.is_zero());
    let doubled = tiny.clone() * 2.0;
    assert!(doubled.as_bigdecimal() > tiny.as_bigdecimal());

    // 非常に大きい価格
    let huge = TokenPrice::new(BigDecimal::from_str("999999999999999999").unwrap());
    let half = huge.clone() * 0.5;
    assert!(half.as_bigdecimal() < huge.as_bigdecimal());

    // PriceF64 のエッジケース
    let tiny_f64 = PriceF64::new(1e-15);
    assert!(!tiny_f64.is_zero());

    let huge_f64 = PriceF64::new(1e15);
    let ratio = huge_f64 / tiny_f64;
    assert!(ratio > 1e29);
}

#[test]
fn test_price_into_bigdecimal() {
    let price = TokenPrice::new(BigDecimal::from(42));
    let bd = price.into_bigdecimal();
    assert_eq!(bd, BigDecimal::from(42));
}

#[test]
fn test_yocto_amount_scalar_mul() {
    let amount = YoctoAmount::new(100);
    let scaled = amount * 3u128;
    assert_eq!(scaled.to_u128(), 300);

    // BigDecimal版はオーバーフローしない（任意精度）
    // u128::MAX より大きい値も保持できる
    let large = YoctoAmount::new(u128::MAX);
    let result = large * 2u128;
    // BigDecimal として値を保持している
    let expected = BigDecimal::from(u128::MAX) * BigDecimal::from(2u128);
    assert_eq!(result.as_bigdecimal(), &expected);
    // to_u128() は None になるので 0 を返す
    assert_eq!(result.to_u128(), 0);
}

// =============================================================================
// TokenPrice 追加演算テスト
// =============================================================================

#[test]
fn test_price_scalar_division() {
    let price = TokenPrice::new(BigDecimal::from(100));

    // TokenPrice / f64
    let divided = price.clone() / 4.0;
    assert_eq!(divided.as_bigdecimal(), &BigDecimal::from(25));

    // ゼロ除算
    let zero_div = price / 0.0;
    assert!(zero_div.is_zero());
}

#[test]
fn test_f64_times_price() {
    let price = TokenPrice::new(BigDecimal::from(10));

    // f64 × TokenPrice
    let scaled = 3.0 * price;
    assert_eq!(scaled.as_bigdecimal(), &BigDecimal::from(30));
}

#[test]
fn test_price_times_bigdecimal() {
    let price = TokenPrice::new(BigDecimal::from(10));

    // TokenPrice × BigDecimal
    let scaled = price * BigDecimal::from(5);
    assert_eq!(scaled.as_bigdecimal(), &BigDecimal::from(50));
}

#[test]
fn test_expected_return() {
    // 上昇ケース: 10 → 12 = 20% リターン
    let current = TokenPrice::new(BigDecimal::from(10));
    let predicted = TokenPrice::new(BigDecimal::from(12));
    let ret = current.expected_return(&predicted);
    assert!((ret - 0.2).abs() < 1e-10, "Expected 0.2, got {}", ret);

    // 下落ケース: 10 → 8 = -20% リターン
    let predicted_down = TokenPrice::new(BigDecimal::from(8));
    let ret_down = current.expected_return(&predicted_down);
    assert!(
        (ret_down - (-0.2)).abs() < 1e-10,
        "Expected -0.2, got {}",
        ret_down
    );

    // ゼロ価格からのリターン
    let zero_price = TokenPrice::zero();
    let ret_zero = zero_price.expected_return(&predicted);
    assert_eq!(ret_zero, 0.0, "Zero price should return 0.0");
}

// =============================================================================
// PriceF64 追加演算テスト
// =============================================================================

#[test]
fn test_price_f64_scalar_operations() {
    let price = PriceF64::new(10.0);

    // PriceF64 × f64
    let scaled = price * 2.5;
    assert!((scaled.as_f64() - 25.0).abs() < 1e-10);

    // f64 × PriceF64
    let scaled2 = 3.0 * price;
    assert!((scaled2.as_f64() - 30.0).abs() < 1e-10);

    // PriceF64 / f64
    let divided = price / 2.0;
    assert!((divided.as_f64() - 5.0).abs() < 1e-10);

    // PriceF64 / 0 (ゼロ除算)
    let zero_div = price / 0.0;
    assert!(zero_div.is_zero());
}

#[test]
fn test_price_f64_zero_division() {
    let p1 = PriceF64::new(10.0);
    let p2 = PriceF64::zero();

    // PriceF64 / PriceF64 (ゼロ除算)
    let ratio = p1 / p2;
    assert_eq!(ratio, 0.0);
}

#[test]
fn test_price_f64_comparison() {
    let p1 = PriceF64::new(10.0);
    let p2 = PriceF64::new(20.0);
    let p3 = PriceF64::new(10.0);

    // PartialEq
    assert_eq!(p1, p3);
    assert_ne!(p1, p2);

    // PartialOrd
    assert!(p1 < p2);
    assert!(p2 > p1);
}

// =============================================================================
// TokenAmountF64 テスト
// =============================================================================

#[test]
fn test_token_amount_f64_basic() {
    let amount = TokenAmountF64::new(1000.0);

    assert_eq!(amount.as_f64(), 1000.0);
    assert!(!amount.is_zero());
    assert!(amount.is_positive());

    let zero = TokenAmountF64::zero();
    assert!(zero.is_zero());
    assert!(!zero.is_positive());
}

#[test]
fn test_token_amount_f64_arithmetic() {
    let a1 = TokenAmountF64::new(100.0);
    let a2 = TokenAmountF64::new(30.0);

    // 加算
    let sum = a1 + a2;
    assert!((sum.as_f64() - 130.0).abs() < 1e-10);

    // 減算
    let diff = a1 - a2;
    assert!((diff.as_f64() - 70.0).abs() < 1e-10);

    // 除算（比率）
    let ratio = a1 / a2;
    assert!((ratio - 3.333333).abs() < 0.001);

    // ゼロ除算
    let zero_div = a1 / TokenAmountF64::zero();
    assert_eq!(zero_div, 0.0);
}

#[test]
fn test_token_amount_f64_scalar_operations() {
    let amount = TokenAmountF64::new(100.0);

    // スカラー乗算
    let scaled = amount * 2.5;
    assert!((scaled.as_f64() - 250.0).abs() < 1e-10);

    // f64 × TokenAmountF64
    let scaled2 = 3.0 * amount;
    assert!((scaled2.as_f64() - 300.0).abs() < 1e-10);

    // スカラー除算
    let divided = amount / 4.0;
    assert!((divided.as_f64() - 25.0).abs() < 1e-10);

    // ゼロ除算
    let zero_div = amount / 0.0;
    assert!(zero_div.is_zero());
}

#[test]
fn test_token_amount_f64_to_bigdecimal() {
    let amount = TokenAmountF64::new(123.456);
    let bd = amount.to_bigdecimal();
    assert!((bd.to_f64().unwrap() - 123.456).abs() < 0.001);
}

#[test]
fn test_token_amount_f64_display() {
    let amount = TokenAmountF64::new(123.456);
    let display = format!("{}", amount);
    assert!(display.starts_with("123.45"));
}

// =============================================================================
// YoctoValueF64 テスト
// =============================================================================

#[test]
fn test_yocto_value_f64_basic() {
    let value = YoctoValueF64::new(1e24);

    assert_eq!(value.as_f64(), 1e24);
    assert!(!value.is_zero());
    assert!(value.is_positive());

    let zero = YoctoValueF64::zero();
    assert!(zero.is_zero());
    assert!(!zero.is_positive());
}

#[test]
fn test_yocto_value_f64_arithmetic() {
    let v1 = YoctoValueF64::new(100.0);
    let v2 = YoctoValueF64::new(30.0);

    // 加算
    let sum = v1 + v2;
    assert!((sum.as_f64() - 130.0).abs() < 1e-10);

    // 減算
    let diff = v1 - v2;
    assert!((diff.as_f64() - 70.0).abs() < 1e-10);

    // 除算（比率）
    let ratio = v1 / v2;
    assert!((ratio - 3.333333).abs() < 0.001);

    // ゼロ除算
    let zero_div = v1 / YoctoValueF64::zero();
    assert_eq!(zero_div, 0.0);
}

#[test]
fn test_yocto_value_f64_scalar_operations() {
    let value = YoctoValueF64::new(100.0);

    // スカラー乗算
    let scaled = value * 2.5;
    assert!((scaled.as_f64() - 250.0).abs() < 1e-10);

    // f64 × YoctoValueF64
    let scaled2 = 3.0 * value;
    assert!((scaled2.as_f64() - 300.0).abs() < 1e-10);
}

#[test]
fn test_yocto_value_f64_conversion() {
    // 1 NEAR = 10^24 yoctoNEAR
    let yocto = YoctoValueF64::new(1e24);
    let near = yocto.to_near();
    assert!((near.as_f64() - 1.0).abs() < 1e-10);

    // to_bigdecimal
    let bd = yocto.to_bigdecimal();
    assert!((bd.as_bigdecimal().to_f64().unwrap() - 1e24).abs() < 1e10);
}

// =============================================================================
// NearValueF64 テスト
// =============================================================================

#[test]
fn test_near_value_f64_basic() {
    let value = NearValueF64::new(10.0);

    assert_eq!(value.as_f64(), 10.0);
    assert!(!value.is_zero());
    assert!(value.is_positive());

    let zero = NearValueF64::zero();
    assert!(zero.is_zero());
    assert!(!zero.is_positive());
}

#[test]
fn test_near_value_f64_arithmetic() {
    let v1 = NearValueF64::new(100.0);
    let v2 = NearValueF64::new(30.0);

    // 加算
    let sum = v1 + v2;
    assert!((sum.as_f64() - 130.0).abs() < 1e-10);

    // 減算
    let diff = v1 - v2;
    assert!((diff.as_f64() - 70.0).abs() < 1e-10);

    // 除算（比率）
    let ratio = v1 / v2;
    assert!((ratio - 3.333333).abs() < 0.001);

    // ゼロ除算
    let zero_div = v1 / NearValueF64::zero();
    assert_eq!(zero_div, 0.0);
}

#[test]
fn test_near_value_f64_scalar_operations() {
    let value = NearValueF64::new(100.0);

    // スカラー乗算
    let scaled = value * 2.5;
    assert!((scaled.as_f64() - 250.0).abs() < 1e-10);

    // f64 × NearValueF64
    let scaled2 = 3.0 * value;
    assert!((scaled2.as_f64() - 300.0).abs() < 1e-10);
}

#[test]
fn test_near_value_f64_conversion() {
    let near = NearValueF64::new(1.0);
    let yocto = near.to_yocto();
    assert!((yocto.as_f64() - 1e24).abs() < 1e10);
}

#[test]
fn test_near_value_f64_display() {
    let value = NearValueF64::new(123.456);
    assert_eq!(format!("{}", value), "123.456 NEAR");
}

// =============================================================================
// f64 版の Price × Amount = Value 演算テスト
// =============================================================================

#[test]
fn test_f64_price_times_amount() {
    let price = PriceF64::new(0.5);
    let amount = TokenAmountF64::new(1000.0);

    // TokenAmountF64 × PriceF64 = YoctoValueF64
    let value1: YoctoValueF64 = amount * price;
    assert!((value1.as_f64() - 500.0).abs() < 1e-10);

    // PriceF64 × TokenAmountF64 = YoctoValueF64
    let value2: YoctoValueF64 = price * amount;
    assert!((value2.as_f64() - 500.0).abs() < 1e-10);
}

#[test]
fn test_f64_value_divided_by_price() {
    let value = YoctoValueF64::new(1000.0);
    let price = PriceF64::new(2.0);

    // YoctoValueF64 / PriceF64 = TokenAmountF64
    let amount: TokenAmountF64 = value / price;
    assert!((amount.as_f64() - 500.0).abs() < 1e-10);

    // ゼロ除算
    let zero_div: TokenAmountF64 = value / PriceF64::zero();
    assert!(zero_div.is_zero());
}

#[test]
fn test_f64_value_divided_by_amount() {
    let value = YoctoValueF64::new(1000.0);
    let amount = TokenAmountF64::new(500.0);

    // YoctoValueF64 / TokenAmountF64 = PriceF64
    let price: PriceF64 = value / amount;
    assert!((price.as_f64() - 2.0).abs() < 1e-10);

    // ゼロ除算
    let zero_div: PriceF64 = value / TokenAmountF64::zero();
    assert!(zero_div.is_zero());
}

#[test]
fn test_price_f64_times_yocto_amount() {
    let price = PriceF64::new(0.5);
    let amount = YoctoAmount::new(1000);

    // PriceF64 × YoctoAmount = f64
    let value: f64 = price * amount;
    assert!((value - 500.0).abs() < 1e-10);
}

// =============================================================================
// その他欠落メソッドのテスト
// =============================================================================

#[test]
fn test_near_value_one() {
    let one = NearValue::one();
    assert_eq!(one.as_bigdecimal(), &BigDecimal::from(1));
}

#[test]
fn test_near_value_to_f64() {
    let value = NearValue::new(BigDecimal::from_str("123.456").unwrap());
    let f64_val = value.to_f64();
    assert!((f64_val.as_f64() - 123.456).abs() < 0.001);
}

#[test]
fn test_yocto_value_into_bigdecimal() {
    let value = YoctoValue::new(BigDecimal::from(12345));
    let bd = value.into_bigdecimal();
    assert_eq!(bd, BigDecimal::from(12345));
}

#[test]
fn test_near_value_into_bigdecimal() {
    let value = NearValue::new(BigDecimal::from(12345));
    let bd = value.into_bigdecimal();
    assert_eq!(bd, BigDecimal::from(12345));
}

#[test]
fn test_yocto_amount_into_bigdecimal() {
    let amount = YoctoAmount::new(12345);
    let bd = amount.into_bigdecimal();
    assert_eq!(bd, BigDecimal::from(12345));
}

#[test]
fn test_yocto_amount_times_bigdecimal() {
    let amount = YoctoAmount::new(100);
    let scaled = amount * BigDecimal::from_str("2.5").unwrap();
    assert_eq!(
        scaled.as_bigdecimal(),
        &BigDecimal::from_str("250").unwrap()
    );
}

#[test]
fn test_near_amount_zero_division() {
    let a1 = NearAmount::new(BigDecimal::from(100));
    let a2 = NearAmount::zero();

    let ratio = a1 / a2;
    assert_eq!(ratio, BigDecimal::zero());
}

#[test]
fn test_yocto_value_zero_division() {
    let v1 = YoctoValue::new(BigDecimal::from(100));
    let v2 = YoctoValue::zero();

    let ratio = v1 / v2;
    assert_eq!(ratio, BigDecimal::zero());
}

#[test]
fn test_near_value_zero_division() {
    let v1 = NearValue::new(BigDecimal::from(100));
    let v2 = NearValue::zero();

    let ratio = v1 / v2;
    assert_eq!(ratio, BigDecimal::zero());
}

#[test]
fn test_yocto_value_div_zero_price() {
    let value = YoctoValue::new(BigDecimal::from(100));
    let price = TokenPrice::zero();

    let amount: YoctoAmount = value / price;
    assert!(amount.is_zero());
}

#[test]
fn test_yocto_value_div_zero_amount() {
    let value = YoctoValue::new(BigDecimal::from(100));
    let amount = YoctoAmount::zero();

    let price: TokenPrice = value / amount;
    assert!(price.is_zero());
}

#[test]
fn test_near_value_div_zero_price() {
    let value = NearValue::new(BigDecimal::from(100));
    let price = TokenPrice::zero();

    let amount: NearAmount = value / price;
    assert!(amount.is_zero());
}

#[test]
fn test_near_value_div_zero_amount() {
    let value = NearValue::new(BigDecimal::from(100));
    let amount = NearAmount::zero();

    let price: TokenPrice = value / amount;
    assert!(price.is_zero());
}

/// YoctoAmount (BigDecimal) の精度保持と整数部取得の動作を検証
///
/// ## 設計上の注意点
/// - YoctoAmount は最小単位（yoctoNEAR = 10^-24 NEAR）を表す
/// - 内部は BigDecimal なので計算途中の精度損失がない
/// - `to_u128()` でブロックチェーン用に整数部を取得する
#[test]
fn test_yocto_amount_truncation_behavior() {
    // ケース1: 割り切れる場合
    let value = YoctoValue::new(BigDecimal::from(1000));
    let price = TokenPrice::new(BigDecimal::from(2));
    let amount: YoctoAmount = value / price;
    assert_eq!(amount.as_bigdecimal(), &BigDecimal::from(500));
    assert_eq!(amount.to_u128(), 500);

    // ケース2: 割り切れない場合 - 精度を保持
    let value = YoctoValue::new(BigDecimal::from(1001));
    let price = TokenPrice::new(BigDecimal::from(2));
    let amount: YoctoAmount = value / price;
    // 1001 / 2 = 500.5（精度を保持）
    assert_eq!(
        amount.as_bigdecimal(),
        &BigDecimal::from_str("500.5").unwrap()
    );
    // ブロックチェーン用に整数部を取得すると切り捨て
    assert_eq!(amount.to_u128(), 500);

    // ケース3: 小数価格での除算 - 精度を保持
    let value = YoctoValue::new(BigDecimal::from(100));
    let price = TokenPrice::new(BigDecimal::from_str("0.3").unwrap());
    let amount: YoctoAmount = value / price;
    // 100 / 0.3 = 333.333...（精度を保持）
    // BigDecimal での除算結果を直接比較
    let expected = BigDecimal::from(100) / BigDecimal::from_str("0.3").unwrap();
    assert_eq!(amount.as_bigdecimal(), &expected);
    // ブロックチェーン用に整数部を取得
    assert_eq!(amount.to_u128(), 333);

    // ケース4: from_bigdecimal で直接作成
    let amount = YoctoAmount::from_bigdecimal(BigDecimal::from_str("123.456").unwrap());
    assert_eq!(
        amount.as_bigdecimal(),
        &BigDecimal::from_str("123.456").unwrap()
    );
    assert_eq!(amount.to_u128(), 123);

    // ケース5: NearAmount::to_yocto() - 精度を保持
    let near_amount = NearAmount::new(BigDecimal::from_str("1.5").unwrap());
    let yocto = near_amount.to_yocto();
    // 1.5 NEAR = 1.5 * 10^24 yoctoNEAR（精度を保持）
    let expected = BigDecimal::from_str("1.5").unwrap() * BigDecimal::from(YOCTO_PER_NEAR);
    assert_eq!(yocto.as_bigdecimal(), &expected);
    // ブロックチェーン用に整数部を取得
    assert_eq!(yocto.to_u128(), expected.to_u128().unwrap());
}
