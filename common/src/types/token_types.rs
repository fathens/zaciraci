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
    /// 新しい ExchangeRate を作成
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

    #[test]
    fn test_exchange_rate_to_price() {
        // USDT: 1 NEAR = 5 USDT, decimals=6
        // raw_rate = 5_000_000
        let rate = ExchangeRate::new(BigDecimal::from(5_000_000), 6);
        let price = rate.to_price();

        // TokenPrice = 10^6 / 5_000_000 = 0.2 NEAR/USDT
        assert_eq!(price.to_f64(), 0.2);
    }

    #[test]
    fn test_token_amount_div_exchange_rate() {
        // 100 USDT を保有
        let holdings = TokenAmount::from_u128(100_000_000, 6); // 100 × 10^6

        // 1 NEAR = 5 USDT
        let rate = ExchangeRate::new(BigDecimal::from(5_000_000), 6);

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
        let rate = ExchangeRate::new(BigDecimal::from(1_000_000_000_000_000_000_000_000u128), 24);
        let price = rate.to_price();

        // TokenPrice = 10^24 / 10^24 = 1.0 NEAR/wNEAR
        assert_eq!(price.to_f64(), 1.0);
    }

    // =============================================================================
    // ExchangeRate 追加テスト
    // =============================================================================

    #[test]
    fn test_exchange_rate_accessors() {
        let rate = ExchangeRate::new(BigDecimal::from(5_000_000), 6);

        // raw_rate()
        assert_eq!(rate.raw_rate(), &BigDecimal::from(5_000_000));

        // decimals()
        assert_eq!(rate.decimals(), 6);
    }

    #[test]
    fn test_exchange_rate_is_zero() {
        let zero_rate = ExchangeRate::new(BigDecimal::zero(), 6);
        assert!(zero_rate.is_zero());

        let non_zero_rate = ExchangeRate::new(BigDecimal::from(100), 6);
        assert!(!non_zero_rate.is_zero());
    }

    #[test]
    fn test_exchange_rate_zero_to_price() {
        // ゼロレートからの価格変換
        let zero_rate = ExchangeRate::new(BigDecimal::zero(), 6);
        let price = zero_rate.to_price();
        assert!(price.is_zero());
    }

    #[test]
    fn test_exchange_rate_display() {
        let rate = ExchangeRate::new(BigDecimal::from(5_000_000), 6);
        let display = format!("{}", rate);
        assert!(display.contains("5000000"));
        assert!(display.contains("decimals=6"));
    }

    #[test]
    fn test_exchange_rate_serialization() {
        let rate = ExchangeRate::new(BigDecimal::from(5_000_000), 6);
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
        let zero_rate = ExchangeRate::new(BigDecimal::zero(), 6);

        // ゼロレートでの除算 → NearValue::zero()
        let value: NearValue = amount / &zero_rate;
        assert!(value.is_zero());
    }

    #[test]
    fn test_token_amount_reference_div_rate() {
        let amount = TokenAmount::from_u128(100_000_000, 6);
        let rate = ExchangeRate::new(BigDecimal::from(5_000_000), 6);

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
}
