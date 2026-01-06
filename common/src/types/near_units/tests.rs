use super::*;
use std::str::FromStr;

#[test]
fn test_price_arithmetic() {
    let p1 = Price::new(BigDecimal::from(10));
    let p2 = Price::new(BigDecimal::from(3));

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
    let sum = a1 + a2;
    assert_eq!(sum.0, 1300);

    // 減算
    let diff = a1 - a2;
    assert_eq!(diff.0, 700);

    // 減算（アンダーフロー防止）
    let diff2 = a2 - a1;
    assert_eq!(diff2.0, 0);
}

#[test]
fn test_unit_conversion() {
    // 1 NEAR = 10^24 yoctoNEAR
    let yocto = YoctoAmount::new(YOCTO_PER_NEAR);
    let near = yocto.to_near();
    assert_eq!(near.0, BigDecimal::from(1));

    // 逆変換
    let back_to_yocto = near.to_yocto();
    assert_eq!(back_to_yocto.0, YOCTO_PER_NEAR);
}

#[test]
fn test_price_times_amount() {
    // Price × YoctoAmount = YoctoValue
    let price = Price::new(BigDecimal::from_str("0.5").unwrap());
    let amount = YoctoAmount::new(1000);
    let value: YoctoValue = price.clone() * amount;
    assert_eq!(value.0, BigDecimal::from(500));

    // Price × NearAmount = NearValue
    let near_amount = NearAmount::new(BigDecimal::from(2));
    let near_value: NearValue = price * near_amount;
    assert_eq!(near_value.0, BigDecimal::from(1));
}

#[test]
fn test_value_divided_by_price() {
    // YoctoValue / Price = YoctoAmount
    let value = YoctoValue::new(BigDecimal::from(1000));
    let price = Price::new(BigDecimal::from(2));
    let amount: YoctoAmount = value / price;
    assert_eq!(amount.0, 500);
}

#[test]
fn test_value_divided_by_amount() {
    // YoctoValue / YoctoAmount = Price
    let value = YoctoValue::new(BigDecimal::from(1000));
    let amount = YoctoAmount::new(500);
    let price: Price = value / amount;
    assert_eq!(price.0, BigDecimal::from(2));
}

#[test]
fn test_zero_division() {
    // ゼロ除算は安全にゼロを返す
    let p1 = Price::new(BigDecimal::from(10));
    let p2 = Price::zero();
    let ratio = p1 / p2;
    assert_eq!(ratio, BigDecimal::zero());

    let value = YoctoValue::new(BigDecimal::from(100));
    let zero_price = Price::zero();
    let amount: YoctoAmount = value / zero_price;
    assert_eq!(amount.0, 0);
}

#[test]
fn test_price_f64_conversion() {
    let price = Price::new(BigDecimal::from_str("123.456").unwrap());
    let price_f64 = price.to_f64();
    assert!((price_f64.0 - 123.456).abs() < 0.001);

    let back_to_price = price_f64.to_bigdecimal();
    // 精度損失があるため、完全一致はしない
    assert!((back_to_price.0.to_f64().unwrap() - 123.456).abs() < 0.001);
}

#[test]
fn test_price_serialization() {
    // Price のシリアライズ/デシリアライズ
    let price = Price::new(BigDecimal::from_str("123.456789").unwrap());
    let json = serde_json::to_string(&price).unwrap();
    let deserialized: Price = serde_json::from_str(&json).unwrap();
    assert_eq!(price, deserialized);

    // PriceF64 のシリアライズ/デシリアライズ
    let price_f64 = PriceF64::new(123.456);
    let json_f64 = serde_json::to_string(&price_f64).unwrap();
    let deserialized_f64: PriceF64 = serde_json::from_str(&json_f64).unwrap();
    assert!((price_f64.as_f64() - deserialized_f64.as_f64()).abs() < 1e-10);
}

#[test]
fn test_price_comparison() {
    let p1 = Price::new(BigDecimal::from(10));
    let p2 = Price::new(BigDecimal::from(20));
    let p3 = Price::new(BigDecimal::from(10));

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
    let price = Price::new(BigDecimal::from_str("123.456").unwrap());
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
    // Price
    assert!(Price::zero().is_zero());
    assert!(!Price::new(BigDecimal::from(1)).is_zero());

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

    // NearValue / Price = NearAmount
    let price = Price::new(BigDecimal::from(2));
    let amount: NearAmount = v1.clone() / price;
    assert_eq!(amount.as_bigdecimal(), &BigDecimal::from(50));

    // NearValue / NearAmount = Price
    let near_amount = NearAmount::new(BigDecimal::from(50));
    let price_result: Price = v1 / near_amount;
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
    // Price の参照演算
    let p1 = Price::new(BigDecimal::from(10));
    let p2 = Price::new(BigDecimal::from(3));

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
    let tiny = Price::new(BigDecimal::from_str("0.000000000001").unwrap());
    assert!(!tiny.is_zero());
    let doubled = tiny.clone() * 2.0;
    assert!(doubled.as_bigdecimal() > tiny.as_bigdecimal());

    // 非常に大きい価格
    let huge = Price::new(BigDecimal::from_str("999999999999999999").unwrap());
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
    let price = Price::new(BigDecimal::from(42));
    let bd = price.into_bigdecimal();
    assert_eq!(bd, BigDecimal::from(42));
}

#[test]
fn test_yocto_amount_scalar_mul() {
    let amount = YoctoAmount::new(100);
    let scaled = amount * 3u128;
    assert_eq!(scaled.as_u128(), 300);

    // オーバーフロー防止（saturating）
    let large = YoctoAmount::new(u128::MAX / 2);
    let result = large * 3u128;
    assert_eq!(result.as_u128(), u128::MAX); // saturating_mul
}
