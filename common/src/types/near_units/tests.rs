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
