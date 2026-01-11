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
    /// let price = TokenPrice::from_near_per_token(BigDecimal::from_str("0.2").unwrap());
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
    /// smallest_units（最小単位）から TokenAmount を作成
    ///
    /// # 引数
    /// - `smallest_units`: 最小単位での量（例: USDT decimals=6 なら 1_000_000 = 1 USDT）
    /// - `decimals`: トークンの decimals
    pub fn from_smallest_units(smallest_units: BigDecimal, decimals: u8) -> Self {
        Self {
            smallest_units,
            decimals,
        }
    }

    /// whole tokens（整数単位）から TokenAmount を作成
    ///
    /// # 引数
    /// - `whole_tokens`: 整数単位での量（例: 1.5 USDT）
    /// - `decimals`: トークンの decimals
    pub fn from_whole_tokens(whole_tokens: BigDecimal, decimals: u8) -> Self {
        Self {
            smallest_units: whole_tokens * pow10(decimals),
            decimals,
        }
    }

    /// u128 から TokenAmount を作成（smallest_units として解釈）
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
        NearValue::from_near(&self.smallest_units / &rate.raw_rate)
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
        NearValue::from_near(&self.smallest_units / &rate.raw_rate)
    }
}

/// TokenAmount × TokenPrice = NearValue
///
/// decimals 変換を内部で行う。
impl Mul<&TokenPrice> for TokenAmount {
    type Output = NearValue;
    fn mul(self, price: &TokenPrice) -> NearValue {
        let whole_tokens = self.to_whole();
        NearValue::from_near(whole_tokens * price.as_bigdecimal())
    }
}

impl Mul<&TokenPrice> for &TokenAmount {
    type Output = NearValue;
    fn mul(self, price: &TokenPrice) -> NearValue {
        let whole_tokens = self.to_whole();
        NearValue::from_near(whole_tokens * price.as_bigdecimal())
    }
}

#[cfg(test)]
mod tests;
