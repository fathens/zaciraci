//! 任意トークン対応の型定義
//!
//! decimals が異なるトークン間で安全に計算するための型を提供する。
//!
//! ## 型の概要
//!
//! - [`ExchangeRate`]: tokens_smallest/NEAR のレート（DB保存用）
//! - [`TokenPrice`]: NEAR/token の価格（比較・分析用） - `near_units` から再エクスポート
//! - [`TokenAmount`]: トークン量 + decimals
//!
//! 詳細は `README.md` を参照。

use bigdecimal::{BigDecimal, Zero};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Div, Mul};

use super::near_units::{NearValue, TokenPrice};

/// 10^n を BigDecimal で計算（オーバーフロー回避）
fn pow10(n: u8) -> BigDecimal {
    let mut result = BigDecimal::from(1);
    let ten = BigDecimal::from(10);
    for _ in 0..n {
        result *= &ten;
    }
    result
}

// =============================================================================
// ExchangeRate（交換レート）
// =============================================================================

/// 交換レート（tokens_smallest / NEAR）
///
/// DB の `token_rates.rate` カラムに対応。
///
/// # 注意
///
/// `raw_rate` は「価格」ではなく「レート」（価格の逆数）。
/// - `raw_rate` が大きい = トークンが安い
/// - `raw_rate` が小さい = トークンが高い
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExchangeRate {
    /// 1 NEAR あたりの smallest_unit 数
    raw_rate: BigDecimal,
    /// トークンの decimals
    decimals: u8,
}

impl ExchangeRate {
    /// raw_rate（tokens_smallest/NEAR）から ExchangeRate を作成
    ///
    /// DB から読み込んだ値や、計算結果の rate を渡す場合に使用。
    ///
    /// # 注意
    ///
    /// price（NEAR/token）を渡す場合は [`from_price`] を使うこと。
    /// 間違えると値が逆数になる。
    pub fn from_raw_rate(raw_rate: BigDecimal, decimals: u8) -> Self {
        Self { raw_rate, decimals }
    }

    /// TokenPrice（NEAR/token）から ExchangeRate を作成
    ///
    /// ```text
    /// raw_rate = 10^decimals / price
    /// ```
    ///
    /// # 例
    ///
    /// ```ignore
    /// use common::types::{ExchangeRate, TokenPrice};
    /// use bigdecimal::BigDecimal;
    /// use std::str::FromStr;
    ///
    /// // 1 USDT = 0.2 NEAR の場合
    /// let price = TokenPrice::new(BigDecimal::from_str("0.2").unwrap());
    /// let rate = ExchangeRate::from_price(&price, 6);
    ///
    /// // raw_rate = 10^6 / 0.2 = 5,000,000
    /// assert_eq!(rate.raw_rate(), &BigDecimal::from(5_000_000));
    /// ```
    pub fn from_price(price: &TokenPrice, decimals: u8) -> Self {
        if price.is_zero() {
            return Self {
                raw_rate: BigDecimal::zero(),
                decimals,
            };
        }
        let divisor = pow10(decimals);
        Self {
            raw_rate: divisor / price.as_bigdecimal(),
            decimals,
        }
    }

    /// 新しい ExchangeRate を作成
    ///
    /// # Deprecated
    ///
    /// `from_raw_rate` または `from_price` を使うこと。
    /// どちらを使うべきかは、渡す値が rate か price かで決まる。
    #[deprecated(
        since = "0.1.0",
        note = "use `from_raw_rate` for rate values or `from_price` for price values"
    )]
    pub fn new(raw_rate: BigDecimal, decimals: u8) -> Self {
        Self { raw_rate, decimals }
    }

    /// raw_rate への参照を取得
    pub fn raw_rate(&self) -> &BigDecimal {
        &self.raw_rate
    }

    /// decimals を取得
    pub fn decimals(&self) -> u8 {
        self.decimals
    }

    /// TokenPrice に変換
    ///
    /// ```text
    /// TokenPrice = 10^decimals / raw_rate
    /// ```
    pub fn to_price(&self) -> TokenPrice {
        if self.raw_rate.is_zero() {
            return TokenPrice::zero();
        }
        let divisor = pow10(self.decimals);
        TokenPrice(divisor / &self.raw_rate)
    }

    /// レートがゼロかどうか
    pub fn is_zero(&self) -> bool {
        self.raw_rate.is_zero()
    }
}

impl fmt::Display for ExchangeRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (decimals={})", self.raw_rate, self.decimals)
    }
}

// =============================================================================
// TokenAmount（トークン量）
// =============================================================================

/// トークン量（decimals 付き）
///
/// 任意のトークンの量を表す。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenAmount {
    /// 最小単位での量
    smallest_units: BigDecimal,
    /// トークンの decimals
    decimals: u8,
}

impl TokenAmount {
    /// 新しい TokenAmount を作成
    pub fn new(smallest_units: BigDecimal, decimals: u8) -> Self {
        Self {
            smallest_units,
            decimals,
        }
    }

    /// u128 から TokenAmount を作成
    pub fn from_u128(smallest_units: u128, decimals: u8) -> Self {
        Self {
            smallest_units: BigDecimal::from(smallest_units),
            decimals,
        }
    }

    /// ゼロ量を作成
    pub fn zero(decimals: u8) -> Self {
        Self {
            smallest_units: BigDecimal::zero(),
            decimals,
        }
    }

    /// smallest_units への参照を取得
    pub fn smallest_units(&self) -> &BigDecimal {
        &self.smallest_units
    }

    /// decimals を取得
    pub fn decimals(&self) -> u8 {
        self.decimals
    }

    /// whole tokens に変換
    pub fn to_whole(&self) -> BigDecimal {
        &self.smallest_units / pow10(self.decimals)
    }

    /// 量がゼロかどうか
    pub fn is_zero(&self) -> bool {
        self.smallest_units.is_zero()
    }
}

impl fmt::Display for TokenAmount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (decimals={})", self.to_whole(), self.decimals)
    }
}

// =============================================================================
// 演算
// =============================================================================

/// TokenAmount / ExchangeRate = NearValue
///
/// トークン保有量から NEAR 建て価値を計算。
impl Div<&ExchangeRate> for TokenAmount {
    type Output = NearValue;
    fn div(self, rate: &ExchangeRate) -> NearValue {
        debug_assert_eq!(
            self.decimals, rate.decimals,
            "decimals mismatch: TokenAmount={}, ExchangeRate={}",
            self.decimals, rate.decimals
        );
        if rate.raw_rate.is_zero() {
            return NearValue::zero();
        }
        NearValue::new(&self.smallest_units / &rate.raw_rate)
    }
}

impl Div<&ExchangeRate> for &TokenAmount {
    type Output = NearValue;
    fn div(self, rate: &ExchangeRate) -> NearValue {
        debug_assert_eq!(
            self.decimals, rate.decimals,
            "decimals mismatch: TokenAmount={}, ExchangeRate={}",
            self.decimals, rate.decimals
        );
        if rate.raw_rate.is_zero() {
            return NearValue::zero();
        }
        NearValue::new(&self.smallest_units / &rate.raw_rate)
    }
}

/// TokenAmount × TokenPrice = NearValue
///
/// decimals 変換を内部で行う。
impl Mul<&TokenPrice> for TokenAmount {
    type Output = NearValue;
    fn mul(self, price: &TokenPrice) -> NearValue {
        let whole_tokens = self.to_whole();
        NearValue::new(whole_tokens * price.as_bigdecimal())
    }
}

impl Mul<&TokenPrice> for &TokenAmount {
    type Output = NearValue;
    fn mul(self, price: &TokenPrice) -> NearValue {
        let whole_tokens = self.to_whole();
        NearValue::new(whole_tokens * price.as_bigdecimal())
    }
}

#[cfg(test)]
mod tests {
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
        let holdings = TokenAmount::from_u128(100_000_000, 6); // 100 × 10^6

        // 1 NEAR = 5 USDT
        let rate = ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6);

        // 100 USDT = 20 NEAR
        let value: NearValue = holdings / &rate;
        assert_eq!(value.as_bigdecimal().to_f64().unwrap(), 20.0);
    }

    #[test]
    fn test_token_amount_mul_price() {
        // 100 USDT を保有
        let holdings = TokenAmount::from_u128(100_000_000, 6);

        // 1 USDT = 0.2 NEAR
        let price = TokenPrice::new(BigDecimal::from_f64(0.2).unwrap());

        // 100 USDT × 0.2 = 20 NEAR
        let value: NearValue = holdings * &price;
        assert_eq!(value.as_bigdecimal().to_f64().unwrap(), 20.0);
    }

    #[test]
    fn test_expected_return() {
        let current = TokenPrice::new(BigDecimal::from_f64(0.2).unwrap());
        let predicted = TokenPrice::new(BigDecimal::from_f64(0.24).unwrap());

        // (0.24 - 0.2) / 0.2 = 0.2 = 20%
        let ret = current.expected_return(&predicted);
        assert!((ret - 0.2).abs() < 1e-10);
    }

    #[test]
    fn test_wnear_rate() {
        // wNEAR: 1 NEAR = 1 wNEAR, decimals=24
        // raw_rate = 10^24
        let rate = ExchangeRate::from_raw_rate(
            BigDecimal::from(1_000_000_000_000_000_000_000_000u128),
            24,
        );
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
        let amount = TokenAmount::from_u128(100_000_000, 6);

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
        let amount = TokenAmount::from_u128(100_000_000, 6);
        let display = format!("{}", amount);
        assert!(display.contains("100")); // whole tokens
        assert!(display.contains("decimals=6"));
    }

    #[test]
    fn test_token_amount_serialization() {
        let amount = TokenAmount::from_u128(100_000_000, 6);
        let json = serde_json::to_string(&amount).unwrap();
        let deserialized: TokenAmount = serde_json::from_str(&json).unwrap();
        assert_eq!(amount, deserialized);
    }

    #[test]
    fn test_token_amount_div_zero_rate() {
        let amount = TokenAmount::from_u128(100_000_000, 6);
        let zero_rate = ExchangeRate::from_raw_rate(BigDecimal::zero(), 6);

        // ゼロレートでの除算 → NearValue::zero()
        let value: NearValue = amount / &zero_rate;
        assert!(value.is_zero());
    }

    #[test]
    fn test_token_amount_reference_div_rate() {
        let amount = TokenAmount::from_u128(100_000_000, 6);
        let rate = ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6);

        // &TokenAmount / &ExchangeRate
        let value: NearValue = &amount / &rate;
        assert_eq!(value.as_bigdecimal().to_f64().unwrap(), 20.0);
    }

    #[test]
    fn test_token_amount_reference_mul_price() {
        let amount = TokenAmount::from_u128(100_000_000, 6);
        let price = TokenPrice::new(BigDecimal::from_f64(0.2).unwrap());

        // &TokenAmount × &TokenPrice
        let value: NearValue = &amount * &price;
        assert_eq!(value.as_bigdecimal().to_f64().unwrap(), 20.0);
    }

    #[test]
    fn test_token_amount_new_with_bigdecimal() {
        let amount = TokenAmount::new(BigDecimal::from_f64(100.5).unwrap(), 6);

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

    /// predict.rs での TokenPrice 使用パターンを検証
    ///
    /// ## 現状の問題
    ///
    /// predict.rs では `TokenPrice::new(rate.rate)` で DB の rate を直接格納している。
    /// これは意味的には誤り（TokenPrice は NEAR/token のはずだが、rate は tokens/NEAR）。
    ///
    /// ## 実際の動作
    ///
    /// 1. DB に rate = 1,500,000 (USDT, decimals=6) が格納
    /// 2. predict.rs: TokenPrice::new(1,500,000) で "price" として格納
    /// 3. stats.rs: この値を抽出し `ExchangeRate::from_raw_rate(1,500,000, 6)` で作成
    /// 4. portfolio.rs: `rate.to_price()` で正しい TokenPrice に変換
    ///
    /// ## 結論
    ///
    /// 現状のコードは数値的には正しく動作する。
    /// ただし、predict.rs の "price" は実際には "rate" である点に注意。
    #[test]
    fn test_predict_rs_data_flow() {
        // DB からの rate 値 (yocto_tokens_per_NEAR)
        let db_rate = BigDecimal::from(1_500_000);

        // predict.rs: TokenPrice として格納（意味的には誤りだが数値は正しい）
        let price_in_predict_rs = TokenPrice::new(db_rate.clone());

        // stats.rs: 値を抽出して ExchangeRate として解釈
        let extracted_value = price_in_predict_rs.as_bigdecimal().clone();
        let exchange_rate = ExchangeRate::from_raw_rate(extracted_value, 6);

        // portfolio.rs: 正しい TokenPrice に変換
        let correct_price = exchange_rate.to_price();

        // 検証: 最終的な TokenPrice は正しい値
        // 1 NEAR = 1.5 USDT → 1 USDT = 0.666 NEAR
        assert!(
            (correct_price.to_f64() - 0.666666).abs() < 0.001,
            "Expected ≈0.666, got {}",
            correct_price.to_f64()
        );

        // 注意: price_in_predict_rs は「価格」ではなく「レート」を保持している
        // 誤って価格として使うと間違った結果になる
        assert!(
            (price_in_predict_rs.to_f64() - 1_500_000.0).abs() < 0.001,
            "predict.rs の 'price' は実際には rate 値: {}",
            price_in_predict_rs.to_f64()
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
}
