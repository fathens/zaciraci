use super::*;
use bigdecimal::{FromPrimitive, ToPrimitive};
use std::str::FromStr;

#[test]
fn test_exchange_rate_to_price() {
    // USDT: 1 NEAR = 5 USDT, decimals=6
    // raw_rate = 5_000_000
    let rate = ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6);
    let price = rate.to_price();

    // TokenPrice = 10^6 / 5_000_000 = 0.2 NEAR/USDT
    assert_eq!(price.to_f64(), 0.2);
}

#[test]
fn test_token_amount_div_exchange_rate() {
    // 100 USDT を保有
    let holdings = TokenAmount {
        smallest_units: BigDecimal::from(100_000_000),
        decimals: 6,
    }; // 100 × 10^6

    // 1 NEAR = 5 USDT
    let rate = ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6);

    // 100 USDT = 20 NEAR
    let value: NearValue = holdings / &rate;
    assert_eq!(value.as_bigdecimal().to_f64().unwrap(), 20.0);
}

#[test]
fn test_token_amount_mul_price() {
    // 100 USDT を保有
    let holdings = TokenAmount {
        smallest_units: BigDecimal::from(100_000_000),
        decimals: 6,
    };

    // 1 USDT = 0.2 NEAR
    let price = TokenPrice::from_near_per_token(BigDecimal::from_f64(0.2).unwrap());

    // 100 USDT × 0.2 = 20 NEAR
    let value: NearValue = holdings * &price;
    assert_eq!(value.as_bigdecimal().to_f64().unwrap(), 20.0);
}

#[test]
fn test_expected_return() {
    let current = TokenPrice::from_near_per_token(BigDecimal::from_f64(0.2).unwrap());
    let predicted = TokenPrice::from_near_per_token(BigDecimal::from_f64(0.24).unwrap());

    // (0.24 - 0.2) / 0.2 = 0.2 = 20%
    let ret = current.expected_return(&predicted);
    assert!((ret - 0.2).abs() < 1e-10);
}

#[test]
fn test_wnear_rate() {
    // wNEAR: 1 NEAR = 1 wNEAR, decimals=24
    // raw_rate = 10^24
    let rate =
        ExchangeRate::from_raw_rate(BigDecimal::from(1_000_000_000_000_000_000_000_000u128), 24);
    let price = rate.to_price();

    // TokenPrice = 10^24 / 10^24 = 1.0 NEAR/wNEAR
    assert_eq!(price.to_f64(), 1.0);
}

// =============================================================================
// ExchangeRate 追加テスト
// =============================================================================

#[test]
fn test_exchange_rate_accessors() {
    let rate = ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6);

    // raw_rate()
    assert_eq!(rate.raw_rate(), &BigDecimal::from(5_000_000));

    // decimals()
    assert_eq!(rate.decimals(), 6);
}

#[test]
fn test_exchange_rate_is_zero() {
    let zero_rate = ExchangeRate::from_raw_rate(BigDecimal::zero(), 6);
    assert!(zero_rate.is_zero());

    let non_zero_rate = ExchangeRate::from_raw_rate(BigDecimal::from(100), 6);
    assert!(!non_zero_rate.is_zero());
}

#[test]
fn test_exchange_rate_zero_to_price() {
    // ゼロレートからの価格変換
    let zero_rate = ExchangeRate::from_raw_rate(BigDecimal::zero(), 6);
    let price = zero_rate.to_price();
    assert!(price.is_zero());
}

#[test]
fn test_exchange_rate_display() {
    let rate = ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6);
    let display = format!("{}", rate);
    assert!(display.contains("5000000"));
    assert!(display.contains("decimals=6"));
}

#[test]
fn test_exchange_rate_serialization() {
    let rate = ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6);
    let json = serde_json::to_string(&rate).unwrap();
    let deserialized: ExchangeRate = serde_json::from_str(&json).unwrap();
    assert_eq!(rate, deserialized);
}

// =============================================================================
// TokenAmount 追加テスト
// =============================================================================

#[test]
fn test_token_amount_basic() {
    let amount = TokenAmount {
        smallest_units: BigDecimal::from(100_000_000),
        decimals: 6,
    };

    // smallest_units()
    assert_eq!(amount.smallest_units(), &BigDecimal::from(100_000_000));

    // decimals()
    assert_eq!(amount.decimals(), 6);

    // is_zero()
    assert!(!amount.is_zero());

    // to_whole()
    let whole = amount.to_whole();
    assert_eq!(whole.to_f64().unwrap(), 100.0); // 100_000_000 / 10^6 = 100
}

#[test]
fn test_token_amount_zero() {
    let zero = TokenAmount::zero(6);
    assert!(zero.is_zero());
    assert_eq!(zero.decimals(), 6);
    assert_eq!(zero.smallest_units(), &BigDecimal::zero());
}

#[test]
fn test_token_amount_display() {
    let amount = TokenAmount {
        smallest_units: BigDecimal::from(100_000_000),
        decimals: 6,
    };
    let display = format!("{}", amount);
    assert!(display.contains("100")); // whole tokens
    assert!(display.contains("decimals=6"));
}

#[test]
fn test_token_amount_serialization() {
    let amount = TokenAmount {
        smallest_units: BigDecimal::from(100_000_000),
        decimals: 6,
    };
    let json = serde_json::to_string(&amount).unwrap();
    let deserialized: TokenAmount = serde_json::from_str(&json).unwrap();
    assert_eq!(amount, deserialized);
}

#[test]
fn test_token_amount_div_zero_rate() {
    let amount = TokenAmount {
        smallest_units: BigDecimal::from(100_000_000),
        decimals: 6,
    };
    let zero_rate = ExchangeRate::from_raw_rate(BigDecimal::zero(), 6);

    // ゼロレートでの除算 → NearValue::zero()
    let value: NearValue = amount / &zero_rate;
    assert!(value.is_zero());
}

#[test]
fn test_token_amount_reference_div_rate() {
    let amount = TokenAmount {
        smallest_units: BigDecimal::from(100_000_000),
        decimals: 6,
    };
    let rate = ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6);

    // &TokenAmount / &ExchangeRate
    let value: NearValue = &amount / &rate;
    assert_eq!(value.as_bigdecimal().to_f64().unwrap(), 20.0);
}

#[test]
fn test_token_amount_reference_mul_price() {
    let amount = TokenAmount {
        smallest_units: BigDecimal::from(100_000_000),
        decimals: 6,
    };
    let price = TokenPrice::from_near_per_token(BigDecimal::from_f64(0.2).unwrap());

    // &TokenAmount × &TokenPrice
    let value: NearValue = &amount * &price;
    assert_eq!(value.as_bigdecimal().to_f64().unwrap(), 20.0);
}

#[test]
fn test_token_amount_new_with_bigdecimal() {
    let amount = TokenAmount {
        smallest_units: BigDecimal::from_f64(100.5).unwrap(),
        decimals: 6,
    };

    // 小数も保持できる
    assert_eq!(amount.decimals(), 6);
    assert!(!amount.is_zero());
}

// =============================================================================
// DB整合性テスト
// =============================================================================

/// DBに格納されるrateの形式を検証
///
/// ## DB格納形式（migration適用後）
///
/// `rate = yocto_tokens_per_near` (yoctoトークン数 / 1 NEAR)
///
/// 例: USDT (decimals=6) で 1 NEAR = 1.5 USDT の場合
/// - 1 NEAR → 1,500,000 smallest_units (1.5 × 10^6)
/// - rate = 1,500,000
///
/// ## TokenPrice への変換
///
/// TokenPrice = 10^decimals / rate = 10^6 / 1,500,000 = 0.666... NEAR/USDT
///
/// ## migration
///
/// 2026-01-08-073422_scale_token_rates_to_yocto で既存データを × 10^24
#[test]
fn test_db_rate_format_after_migration() {
    // シナリオ: 1 NEAR = 1.5 USDT, USDT decimals=6
    // DB格納: rate = 1,500,000 (yocto tokens per 1 NEAR)
    let db_rate = BigDecimal::from(1_500_000);
    let rate = ExchangeRate::from_raw_rate(db_rate, 6);

    let price = rate.to_price();
    // TokenPrice ≈ 0.666... NEAR/USDT (1/1.5)
    assert!((price.to_f64() - 0.666666).abs() < 0.001);

    // 逆に: 1 USDT = 0.666 NEAR なので 1 NEAR = 1.5 USDT
    let near_per_usdt = 1.0 / price.to_f64();
    assert!((near_per_usdt - 1.5).abs() < 0.001);
}

/// migration前の旧形式データを検証（参考用）
///
/// 旧形式: rate = value / (initial_value) where initial_value = 100 × 10^24
/// 新形式: rate = value / 100
/// 変換: new_rate = old_rate × 10^24
#[test]
fn test_db_rate_migration_conversion() {
    use std::str::FromStr;

    // 旧形式の実際のDB値（USDT例）
    let old_rate = BigDecimal::from_str("0.00000000000000000151200709").unwrap();

    // migration後（× 10^24）
    let yocto_per_near = BigDecimal::from(1_000_000_000_000_000_000_000_000u128);
    let new_rate = &old_rate * &yocto_per_near;

    // 新形式レート ≈ 1,512,007
    let new_rate_f64 = new_rate.to_f64().unwrap();
    assert!((new_rate_f64 - 1_512_007.0).abs() < 1.0);

    // ExchangeRate として解釈
    let rate = ExchangeRate::from_raw_rate(new_rate, 6);
    let price = rate.to_price();

    // TokenPrice ≈ 0.66 NEAR/USDT
    assert!((price.to_f64() - 0.66).abs() < 0.01);
}

/// 期待リターンの計算検証
///
/// rate と price は逆関係:
/// - rate 増加 = price 減少 = 負のリターン
/// - rate 減少 = price 増加 = 正のリターン
#[test]
fn test_expected_return_from_rates() {
    // 現在: 1 NEAR = 1.5 USDT (rate = 1,500,000)
    let current_rate = ExchangeRate::from_raw_rate(BigDecimal::from(1_500_000), 6);
    let current_price = current_rate.to_price();

    // 予測: 1 NEAR = 1.8 USDT (rate = 1,800,000)
    // rate 増加 = トークンが安くなった = 価格下落
    let predicted_rate = ExchangeRate::from_raw_rate(BigDecimal::from(1_800_000), 6);
    let predicted_price = predicted_rate.to_price();

    // current_price ≈ 0.666, predicted_price ≈ 0.555
    // expected_return = (predicted - current) / current = (0.555 - 0.666) / 0.666 ≈ -0.166
    let ret = current_price.expected_return(&predicted_price);
    assert!(
        (ret - (-0.166)).abs() < 0.01,
        "Expected ≈-0.166, got {}",
        ret
    );

    // 逆のケース: rate 減少 = 価格上昇 = 正のリターン
    // 予測: 1 NEAR = 1.2 USDT (rate = 1,200,000)
    let predicted_rate_up = ExchangeRate::from_raw_rate(BigDecimal::from(1_200_000), 6);
    let predicted_price_up = predicted_rate_up.to_price();

    // expected_return = (0.833 - 0.666) / 0.666 ≈ 0.25
    let ret_up = current_price.expected_return(&predicted_price_up);
    assert!(
        (ret_up - 0.25).abs() < 0.01,
        "Expected ≈0.25, got {}",
        ret_up
    );
}

// =============================================================================
// データフロー整合性テスト
// =============================================================================

/// predict.rs での予測データフローを検証
///
/// ## 現在の実装（型安全な設計）
///
/// predict.rs では forecast_values (price 形式) を `TokenPrice::from_near_per_token()` で
/// `PredictedPrice.price: TokenPrice` に格納している。
///
/// ## データフロー
///
/// 1. Chronos API から forecast_values (price 形式: NEAR/token) を取得
/// 2. predict.rs: `TokenPrice::from_near_per_token(price_value)` で `PredictedPrice.price` に格納
/// 3. algorithm/types.rs: `PredictionData.predicted_price_24h` として使用
/// 4. portfolio.rs: TokenPrice をそのまま使用
///
/// ## 設計意図
///
/// - rate と price の型を明確に区別
/// - forecast_values が price 形式であることを型で保証
/// - 型による誤用防止
#[test]
fn test_predict_rs_data_flow() {
    // Chronos API からの予測値 (price 形式: NEAR/token)
    // 例: 1 USDT = 0.2 NEAR
    let forecast_price_value = BigDecimal::from_str("0.2").unwrap();

    // predict.rs: TokenPrice として格納（型安全）
    let predicted_price = TokenPrice::from_near_per_token(forecast_price_value.clone());

    // 検証: TokenPrice は正しい値を保持
    assert!(
        (predicted_price.to_f64() - 0.2).abs() < 0.001,
        "Expected ≈0.2, got {}",
        predicted_price.to_f64()
    );

    // 期待リターンの計算例
    // 現在価格: 1 USDT = 0.2 NEAR, 予測価格: 1 USDT = 0.24 NEAR (+20%)
    let current_price = TokenPrice::from_near_per_token(BigDecimal::from_str("0.2").unwrap());
    let predicted_higher = TokenPrice::from_near_per_token(BigDecimal::from_str("0.24").unwrap());
    let expected_return = current_price.expected_return(&predicted_higher);

    assert!(
        (expected_return - 0.2).abs() < 0.001,
        "Expected return ≈0.2 (20%), got {}",
        expected_return
    );
}

/// decimals が異なるトークン間での正しい価格計算を検証
///
/// DB の rate は decimals を考慮していない（smallest_units 数のみ）。
/// ExchangeRate に decimals を渡すことで正しい価格に変換できる。
#[test]
fn test_decimals_agnostic_rate_storage() {
    // シナリオ: 1 NEAR = 1.5 トークン
    // decimals=6 の場合: rate = 1,500,000
    // decimals=18 の場合: rate = 1,500,000,000,000,000,000

    // decimals=6 (USDT風)
    let rate_6 = BigDecimal::from(1_500_000u64);
    let exchange_rate_6 = ExchangeRate::from_raw_rate(rate_6, 6);
    let price_6 = exchange_rate_6.to_price();

    // decimals=18 (ETH風)
    let rate_18 = BigDecimal::from_str("1500000000000000000").unwrap();
    let exchange_rate_18 = ExchangeRate::from_raw_rate(rate_18, 18);
    let price_18 = exchange_rate_18.to_price();

    // 両方とも同じ価格になるべき: 0.666... NEAR/token
    assert!(
        (price_6.to_f64() - price_18.to_f64()).abs() < 0.001,
        "Different decimals should yield same price: {} vs {}",
        price_6.to_f64(),
        price_18.to_f64()
    );

    assert!(
        (price_6.to_f64() - 0.666666).abs() < 0.001,
        "Expected ≈0.666, got {}",
        price_6.to_f64()
    );
}

/// wrap.near (wNEAR) の rate 検証
///
/// wrap.near は 1:1 で NEAR と等価なので rate = 10^24
#[test]
fn test_wnear_rate_with_decimals() {
    // wNEAR: decimals=24, rate = 10^24
    let wnear_rate = BigDecimal::from_str("1000000000000000000000000").unwrap();
    let exchange_rate = ExchangeRate::from_raw_rate(wnear_rate, 24);
    let price = exchange_rate.to_price();

    // 1 wNEAR = 1 NEAR
    assert!(
        (price.to_f64() - 1.0).abs() < 0.0001,
        "wNEAR should be 1:1 with NEAR, got {}",
        price.to_f64()
    );
}

// =============================================================================
// TokenAmount / NearAmount → ExchangeRate テスト
// =============================================================================

use super::NearAmount;

/// 基本テスト: 100 USDT / 20 NEAR = rate (1 NEAR = 5 USDT)
#[test]
fn test_token_amount_div_near_amount() {
    // 100 USDT = 100_000_000 smallest_units (decimals=6)
    let amount = TokenAmount::from_smallest_units(BigDecimal::from(100_000_000), 6);
    // 20 NEAR
    let near = NearAmount::from_str("20").unwrap();

    // rate = 100_000_000 / 20 = 5_000_000
    let rate: ExchangeRate = &amount / &near;

    // 検証: raw_rate = 5_000_000
    assert_eq!(rate.raw_rate(), &BigDecimal::from(5_000_000));
    assert_eq!(rate.decimals(), 6);

    // 検証: 1 NEAR = 5 USDT → price = 0.2 NEAR/USDT
    let price = rate.to_price();
    assert!((price.to_f64() - 0.2).abs() < 0.001);
}

/// ゼロ除算テスト
#[test]
fn test_token_amount_div_zero_near() {
    let amount = TokenAmount::from_smallest_units(BigDecimal::from(100_000_000), 6);
    let zero_near = NearAmount::zero();

    let rate: ExchangeRate = &amount / &zero_near;
    assert!(rate.is_zero());
    assert_eq!(rate.decimals(), 6); // decimals は保持
}

/// wNEAR テスト: 1e24 wNEAR / 1 NEAR = rate (1 NEAR = 1 wNEAR)
#[test]
fn test_wnear_token_amount_div_near_amount() {
    // 1 wNEAR = 10^24 smallest_units (decimals=24)
    let amount = TokenAmount::from_smallest_units(
        BigDecimal::from(1_000_000_000_000_000_000_000_000u128),
        24,
    );
    // 1 NEAR
    let near = NearAmount::from_str("1").unwrap();

    let rate: ExchangeRate = &amount / &near;

    // 検証: 1 NEAR = 1 wNEAR
    assert_eq!(
        rate.raw_rate(),
        &BigDecimal::from(1_000_000_000_000_000_000_000_000u128)
    );
    assert_eq!(rate.decimals(), 24);

    // rate.to_price() = 10^24 / 10^24 = 1.0 NEAR/wNEAR
    let price = rate.to_price();
    assert!((price.to_f64() - 1.0).abs() < 0.0001);
}

/// ラウンドトリップ: TokenAmount → ExchangeRate → TokenAmount
///
/// TokenAmount / NearAmount → ExchangeRate
/// NearValue * ExchangeRate → TokenAmount （既存演算の逆）
#[test]
fn test_token_amount_exchange_rate_roundtrip() {
    // 開始: 100 USDT
    let original_amount = TokenAmount::from_smallest_units(BigDecimal::from(100_000_000), 6);
    let near = NearAmount::from_str("20").unwrap();

    // TokenAmount / NearAmount → ExchangeRate
    let rate: ExchangeRate = &original_amount / &near;

    // NearValue * ExchangeRate → TokenAmount
    // 既存: &NearValue * &ExchangeRate → TokenAmount
    let near_value = NearValue::from_near(BigDecimal::from(20));
    let recovered_amount: TokenAmount = &near_value * &rate;

    // 検証: 元の値に戻る
    assert_eq!(
        recovered_amount.smallest_units(),
        original_amount.smallest_units()
    );
    assert_eq!(recovered_amount.decimals(), original_amount.decimals());
}

/// 既存テストとの整合性: TokenAmount / ExchangeRate = NearValue の逆
///
/// 既存: TokenAmount / ExchangeRate → NearValue
/// 新規: TokenAmount / NearAmount → ExchangeRate
/// これらは逆演算の関係にある
#[test]
fn test_token_amount_near_amount_inverse_of_existing() {
    // 既存テスト (test_token_amount_div_exchange_rate) の逆演算を検証
    // 100 USDT / rate(5_000_000) = 20 NEAR
    // 逆: 100 USDT / 20 NEAR = rate(5_000_000)

    let amount = TokenAmount::from_smallest_units(BigDecimal::from(100_000_000), 6);
    let expected_rate = ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6);

    // 既存: TokenAmount / ExchangeRate → NearValue
    let near_value: NearValue = &amount / &expected_rate;
    assert_eq!(near_value.as_bigdecimal().to_f64().unwrap(), 20.0);

    // 新規: TokenAmount / NearAmount → ExchangeRate
    let near_amount = NearAmount::from_str("20").unwrap();
    let actual_rate: ExchangeRate = &amount / &near_amount;

    // 検証: 同じ ExchangeRate になる
    assert_eq!(actual_rate.raw_rate(), expected_rate.raw_rate());
    assert_eq!(actual_rate.decimals(), expected_rate.decimals());
}
