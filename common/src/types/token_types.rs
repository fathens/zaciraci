//! 任意トークン対応の型定義
//!
//! decimals が異なるトークン間で安全に計算するための型を提供する。
//!
//! ## 型の概要
//!
//! - [`ExchangeRate`]: tokens_smallest/NEAR のレート（DB保存用）
//! - [`TokenPrice`]: NEAR/token の価格（比較・分析用）
//! - [`TokenAmount`]: トークン量 + decimals
//!
//! 詳細は `README.md` を参照。

use bigdecimal::{BigDecimal, FromPrimitive, ToPrimitive, Zero};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Div, Mul};

use super::near_units::NearValue;

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
// TokenPrice（トークン価格）
// =============================================================================

/// トークン価格（NEAR / token）
///
/// decimals を考慮済みの「whole token あたりの NEAR」。
///
/// - `TokenPrice` が大きい = トークンが高い
/// - `TokenPrice` が小さい = トークンが安い
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TokenPrice(BigDecimal);

impl TokenPrice {
    /// ゼロ価格を作成
    pub fn zero() -> Self {
        TokenPrice(BigDecimal::zero())
    }

    /// BigDecimal から TokenPrice を作成
    pub fn new(value: BigDecimal) -> Self {
        TokenPrice(value)
    }

    /// 内部の BigDecimal への参照を取得
    pub fn as_bigdecimal(&self) -> &BigDecimal {
        &self.0
    }

    /// BigDecimal に変換
    pub fn into_bigdecimal(self) -> BigDecimal {
        self.0
    }

    /// f64 に変換（精度損失あり）
    pub fn to_f64(&self) -> f64 {
        self.0.to_f64().unwrap_or(0.0)
    }

    /// 価格がゼロかどうか
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    /// 期待リターンを計算
    ///
    /// ```text
    /// return = (predicted - current) / current
    /// ```
    ///
    /// # 注意
    ///
    /// `ExchangeRate` から直接リターンを計算すると符号が逆になる。
    /// `TokenPrice` を使えばこの混乱を防げる。
    pub fn expected_return(&self, predicted: &TokenPrice) -> f64 {
        let current = self.to_f64();
        let pred = predicted.to_f64();
        if current == 0.0 {
            return 0.0;
        }
        (pred - current) / current
    }
}

impl fmt::Display for TokenPrice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} NEAR/token", self.0)
    }
}

// TokenPrice 同士の減算 → BigDecimal
impl std::ops::Sub for TokenPrice {
    type Output = BigDecimal;
    fn sub(self, other: TokenPrice) -> BigDecimal {
        self.0 - other.0
    }
}

impl std::ops::Sub<&TokenPrice> for &TokenPrice {
    type Output = BigDecimal;
    fn sub(self, other: &TokenPrice) -> BigDecimal {
        &self.0 - &other.0
    }
}

// TokenPrice 同士の除算 → 比率
impl Div for TokenPrice {
    type Output = BigDecimal;
    fn div(self, other: TokenPrice) -> BigDecimal {
        if other.0.is_zero() {
            BigDecimal::zero()
        } else {
            self.0 / other.0
        }
    }
}

// TokenPrice × スカラー
impl Mul<f64> for TokenPrice {
    type Output = TokenPrice;
    fn mul(self, scalar: f64) -> TokenPrice {
        TokenPrice(self.0 * BigDecimal::from_f64(scalar).unwrap_or_default())
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
}
