//! 任意トークンの最小単位（smallest_units）での量を表す型。
//!
//! [`TokenAmount`] が `{smallest_units, decimals}` のペアであるのに対し、
//! [`TokenSmallestUnits`] は smallest_units 単体を型安全に表現する。
//! decimals 情報を持たないため、人間が読める形式への変換には
//! 別途 decimals が必要。

use bigdecimal::{BigDecimal, Zero};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, Div, Mul, Sub};
use std::str::FromStr;

use super::token_types::TokenAmount;

/// トークンの最小単位（smallest_units）での量。
///
/// # 用途
///
/// - JSONB に保存されるプールリザーブ量（`SwapPoolInfo.amount_in/out`）
/// - JSONB に保存されるトークン残高（`TokenHolding.balance`）
/// - ブロックチェーン RPC から取得した `U128` 値の型安全な保持
///
/// # シリアライズ
///
/// `#[serde(transparent)]` により、JSON では内部の `BigDecimal` と同じ形式
/// （文字列）でシリアライズされる。既存 JSONB データとの互換性あり。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TokenSmallestUnits(BigDecimal);

impl TokenSmallestUnits {
    /// ゼロ量を作成
    pub fn zero() -> Self {
        Self(BigDecimal::zero())
    }

    /// BigDecimal から作成
    pub fn from_bigdecimal(value: BigDecimal) -> Self {
        Self(value)
    }

    /// u128 から作成（ブロックチェーン SDK の U128 値向け）
    pub fn from_u128(value: u128) -> Self {
        Self(BigDecimal::from(value))
    }

    /// 内部の BigDecimal への参照を取得
    pub fn as_bigdecimal(&self) -> &BigDecimal {
        &self.0
    }

    /// BigDecimal を消費して取得
    pub fn into_bigdecimal(self) -> BigDecimal {
        self.0
    }

    /// decimals を付与して TokenAmount に変換
    pub fn with_decimals(self, decimals: u8) -> TokenAmount {
        TokenAmount::from_smallest_units(self.0, decimals)
    }

    /// 量がゼロかどうか
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
}

impl fmt::Display for TokenSmallestUnits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for TokenSmallestUnits {
    type Err = bigdecimal::ParseBigDecimalError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        BigDecimal::from_str(s).map(Self)
    }
}

impl From<BigDecimal> for TokenSmallestUnits {
    fn from(value: BigDecimal) -> Self {
        Self(value)
    }
}

impl From<TokenSmallestUnits> for BigDecimal {
    fn from(value: TokenSmallestUnits) -> Self {
        value.0
    }
}

impl From<u128> for TokenSmallestUnits {
    fn from(value: u128) -> Self {
        Self::from_u128(value)
    }
}

// =============================================================================
// 四則演算
// =============================================================================

impl Add for TokenSmallestUnits {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl Add for &TokenSmallestUnits {
    type Output = TokenSmallestUnits;
    fn add(self, rhs: Self) -> TokenSmallestUnits {
        TokenSmallestUnits(&self.0 + &rhs.0)
    }
}

impl Sub for TokenSmallestUnits {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl Sub for &TokenSmallestUnits {
    type Output = TokenSmallestUnits;
    fn sub(self, rhs: Self) -> TokenSmallestUnits {
        TokenSmallestUnits(&self.0 - &rhs.0)
    }
}

/// TokenSmallestUnits × BigDecimal → TokenSmallestUnits（スケーリング）
impl Mul<&BigDecimal> for &TokenSmallestUnits {
    type Output = TokenSmallestUnits;
    fn mul(self, rhs: &BigDecimal) -> TokenSmallestUnits {
        TokenSmallestUnits(&self.0 * rhs)
    }
}

/// TokenSmallestUnits / BigDecimal → TokenSmallestUnits（スケーリング）
impl Div<&BigDecimal> for &TokenSmallestUnits {
    type Output = TokenSmallestUnits;
    fn div(self, rhs: &BigDecimal) -> TokenSmallestUnits {
        TokenSmallestUnits(&self.0 / rhs)
    }
}

/// TokenSmallestUnits / TokenSmallestUnits → BigDecimal（比率）
impl Div for &TokenSmallestUnits {
    type Output = BigDecimal;
    fn div(self, rhs: Self) -> BigDecimal {
        &self.0 / &rhs.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_u128() {
        let amount = TokenSmallestUnits::from_u128(1_000_000);
        assert_eq!(amount.to_string(), "1000000");
    }

    #[test]
    fn test_from_str() {
        let amount: TokenSmallestUnits = "123456789012345678901234".parse().unwrap();
        assert_eq!(amount.to_string(), "123456789012345678901234");
    }

    #[test]
    fn test_serde_transparent() {
        let amount = TokenSmallestUnits::from_u128(42);
        let json = serde_json::to_string(&amount).unwrap();
        // transparent: BigDecimal と同じ形式（文字列としてシリアライズ）
        assert_eq!(json, r#""42""#);

        let deserialized: TokenSmallestUnits = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, amount);
    }

    #[test]
    fn test_serde_roundtrip_in_struct() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct TestStruct {
            balance: TokenSmallestUnits,
        }

        let original = TestStruct {
            balance: TokenSmallestUnits::from_u128(5_000_000_000_000_000_000_000_000),
        };
        let json = serde_json::to_string(&original).unwrap();
        // フィールド名は維持、値は BigDecimal と同じ形式
        assert!(json.contains("\"balance\""));

        let deserialized: TestStruct = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, original);
    }

    #[test]
    fn test_with_decimals() {
        let smallest = TokenSmallestUnits::from_u128(1_000_000);
        let amount = smallest.with_decimals(6);
        assert_eq!(amount.decimals(), 6);
        assert_eq!(amount.smallest_units(), &BigDecimal::from(1_000_000));
    }

    #[test]
    fn test_arithmetic() {
        let a = TokenSmallestUnits::from_u128(100);
        let b = TokenSmallestUnits::from_u128(30);

        assert_eq!(&a + &b, TokenSmallestUnits::from_u128(130));
        assert_eq!(&a - &b, TokenSmallestUnits::from_u128(70));

        let scale = BigDecimal::from(2);
        assert_eq!(&a * &scale, TokenSmallestUnits::from_u128(200));
        assert_eq!(&a / &scale, TokenSmallestUnits::from_u128(50));

        let ratio = &a / &b;
        assert!(ratio > 3_u32);
    }

    #[test]
    fn test_is_zero() {
        assert!(TokenSmallestUnits::zero().is_zero());
        assert!(!TokenSmallestUnits::from_u128(1).is_zero());
    }

    #[test]
    fn test_from_bigdecimal_and_back() {
        let bd = BigDecimal::from(42);
        let tsu = TokenSmallestUnits::from(bd.clone());
        let back: BigDecimal = tsu.into();
        assert_eq!(back, bd);
    }

    #[test]
    fn test_backward_compat_with_string_json() {
        // 既存 JSONB データとの互換性: 文字列からデシリアライズ可能
        let json = r#""5000000000000000000000000""#;
        let amount: TokenSmallestUnits = serde_json::from_str(json).unwrap();
        assert_eq!(
            amount,
            TokenSmallestUnits::from_bigdecimal(
                BigDecimal::from_str("5000000000000000000000000").unwrap()
            )
        );
    }
}
