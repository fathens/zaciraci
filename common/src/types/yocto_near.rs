use bigdecimal::BigDecimal;
use bigdecimal::One;
use bigdecimal::ToPrimitive;
use bigdecimal::Zero;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::fmt;
use std::ops::Add;
use std::ops::Div;
use std::ops::Mul;
use std::ops::Sub;
use std::str::FromStr;

// copy from near-token-0.3.0
const ONE_NEAR: u128 = 10_u128.pow(24);
const ONE_MILLINEAR: u128 = 10_u128.pow(21);

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct YoctoNearToken(pub u128);

impl YoctoNearToken {
    pub fn from_near(near: BigDecimal) -> Self {
        let one = BigDecimal::from(ONE_NEAR);
        let v = (near * one).to_u128().unwrap();
        YoctoNearToken(v)
    }

    pub fn from_millinear(millinear: BigDecimal) -> Self {
        let one = BigDecimal::from(ONE_MILLINEAR);
        let v = (millinear * one).to_u128().unwrap();
        YoctoNearToken(v)
    }

    pub fn from_yocto(yocto: u128) -> Self {
        YoctoNearToken(yocto)
    }

    pub fn as_near(&self) -> BigDecimal {
        BigDecimal::from(self.0) / BigDecimal::from(ONE_NEAR)
    }

    pub fn as_millinear(&self) -> BigDecimal {
        BigDecimal::from(self.0) / BigDecimal::from(ONE_MILLINEAR)
    }

    pub fn as_yoctonear(&self) -> u128 {
        self.0
    }
}

impl From<u128> for YoctoNearToken {
    fn from(yocto: u128) -> Self {
        YoctoNearToken::from_yocto(yocto)
    }
}

impl From<YoctoNearToken> for u128 {
    fn from(token: YoctoNearToken) -> Self {
        token.0
    }
}

impl Zero for YoctoNearToken {
    fn zero() -> Self {
        YoctoNearToken(0)
    }

    fn is_zero(&self) -> bool {
        self.0 == 0
    }
}

impl One for YoctoNearToken {
    fn one() -> Self {
        YoctoNearToken(1)
    }
}

impl Add<YoctoNearToken> for YoctoNearToken {
    type Output = YoctoNearToken;

    fn add(self, other: YoctoNearToken) -> Self::Output {
        YoctoNearToken(self.0 + other.0)
    }
}

impl Sub<YoctoNearToken> for YoctoNearToken {
    type Output = YoctoNearToken;

    fn sub(self, other: YoctoNearToken) -> Self::Output {
        if self.0 < other.0 {
            YoctoNearToken(0)
        } else {
            YoctoNearToken(self.0 - other.0)
        }
    }
}

impl Mul<YoctoNearToken> for YoctoNearToken {
    type Output = YoctoNearToken;

    fn mul(self, other: YoctoNearToken) -> Self::Output {
        YoctoNearToken(self.0 * other.0)
    }
}

impl Div<YoctoNearToken> for YoctoNearToken {
    type Output = YoctoNearToken;

    fn div(self, other: YoctoNearToken) -> Self::Output {
        if other.0 == 0 {
            YoctoNearToken::zero()
        } else {
            YoctoNearToken(self.0 / other.0)
        }
    }
}

impl Serialize for YoctoNearToken {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for YoctoNearToken {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse()
            .map_err(|e| de::Error::custom(format!("Invalid YoctoNEAR value: {}", e)))
    }
}

impl fmt::Display for YoctoNearToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for YoctoNearToken {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(YoctoNearToken(s.parse()?))
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum NearUnit {
    Near,
    MilliNear,
    YoctoNear,
}

impl std::fmt::Display for NearUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NearUnit::Near => write!(f, "NEAR"),
            NearUnit::MilliNear => write!(f, "mNEAR"),
            NearUnit::YoctoNear => write!(f, "yNEAR"),
        }
    }
}

impl std::str::FromStr for NearUnit {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "NEAR" => Ok(NearUnit::Near),
            "mNEAR" => Ok(NearUnit::MilliNear),
            "yNEAR" => Ok(NearUnit::YoctoNear),
            _ => Err(anyhow::anyhow!("Invalid amount unit: {}", s)),
        }
    }
}

impl NearUnit {
    pub fn to_yocto(&self, amount: BigDecimal) -> YoctoNearToken {
        match self {
            NearUnit::Near => YoctoNearToken::from_near(amount),
            NearUnit::MilliNear => YoctoNearToken::from_millinear(amount),
            NearUnit::YoctoNear => YoctoNearToken::from_yocto(amount.to_u128().unwrap()),
        }
    }

    pub fn from_yocto(&self, amount: YoctoNearToken) -> BigDecimal {
        match self {
            NearUnit::Near => amount.as_near(),
            NearUnit::MilliNear => amount.as_millinear(),
            NearUnit::YoctoNear => BigDecimal::from(amount.as_yoctonear()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_near_token_transform() {
        // copy from near-token-0.3.0
        const ONE_NEAR: u128 = 10_u128.pow(24);
        const ONE_MILLINEAR: u128 = 10_u128.pow(21);

        let zero = YoctoNearToken::zero();
        assert_eq!(zero, YoctoNearToken::from_yocto(0));
        assert_eq!(zero.as_yoctonear(), 0);
        assert!(zero.is_zero());
        assert!(!zero.is_one());

        let one_yocto = YoctoNearToken::one();
        assert_eq!(one_yocto, YoctoNearToken::from_yocto(1));
        assert_eq!(one_yocto.as_yoctonear(), 1);
        assert!(one_yocto.is_one());
        assert!(!one_yocto.is_zero());

        let one_near = YoctoNearToken::from_near(BigDecimal::from(1));
        assert_eq!(one_near, YoctoNearToken::from_yocto(ONE_NEAR));
        assert_eq!(one_near.as_yoctonear(), ONE_NEAR);
        assert_eq!(one_near.as_near(), BigDecimal::from(1));
        assert_eq!(one_near.as_millinear(), BigDecimal::from(1000));
        assert!(!one_near.is_one());
        assert!(!one_near.is_zero());

        let one_milli = YoctoNearToken::from_millinear(BigDecimal::from(1));
        assert_eq!(one_milli, YoctoNearToken::from_yocto(ONE_MILLINEAR));
        assert_eq!(one_milli.as_yoctonear(), ONE_MILLINEAR);
        assert_eq!(
            one_milli.as_near(),
            BigDecimal::from(1) / BigDecimal::from(1000)
        );
        assert_eq!(one_milli.as_millinear(), BigDecimal::from(1));
        assert!(!one_milli.is_one());
        assert!(!one_milli.is_zero());

        let n11 = YoctoNearToken::from_near(BigDecimal::from(11) / BigDecimal::from(10));
        assert_eq!(n11.as_yoctonear(), 11 * ONE_NEAR / 10);
        assert_eq!(n11.as_near(), BigDecimal::from(11) / BigDecimal::from(10));
        assert_eq!(n11.as_millinear(), BigDecimal::from(1100));
        assert!(!n11.is_one());
        assert!(!n11.is_zero());
    }

    #[test]
    fn test_near_token_add() {
        assert_eq!(
            YoctoNearToken::zero() + YoctoNearToken::zero(),
            YoctoNearToken::zero()
        );
        assert_eq!(
            YoctoNearToken::zero() + YoctoNearToken::one(),
            YoctoNearToken::one()
        );
        assert_eq!(
            YoctoNearToken::one() + YoctoNearToken::zero(),
            YoctoNearToken::one()
        );
        assert_eq!(
            YoctoNearToken::one() + YoctoNearToken::one(),
            YoctoNearToken::from_yocto(2)
        );
    }

    #[test]
    fn test_near_token_sub() {
        assert_eq!(
            YoctoNearToken::zero() - YoctoNearToken::zero(),
            YoctoNearToken::zero()
        );
        assert_eq!(
            YoctoNearToken::zero() - YoctoNearToken::one(),
            YoctoNearToken::zero()
        );
        assert_eq!(
            YoctoNearToken::one() - YoctoNearToken::zero(),
            YoctoNearToken::one()
        );
        assert_eq!(
            YoctoNearToken::one() - YoctoNearToken::one(),
            YoctoNearToken::zero()
        );
    }

    #[test]
    fn test_near_token_mul() {
        assert_eq!(
            YoctoNearToken::zero() * YoctoNearToken::zero(),
            YoctoNearToken::zero()
        );
        assert_eq!(
            YoctoNearToken::zero() * YoctoNearToken::one(),
            YoctoNearToken::zero()
        );
        assert_eq!(
            YoctoNearToken::one() * YoctoNearToken::zero(),
            YoctoNearToken::zero()
        );
        assert_eq!(
            YoctoNearToken::one() * YoctoNearToken::one(),
            YoctoNearToken::one()
        );
        assert_eq!(
            YoctoNearToken::from(5) * YoctoNearToken::from(2),
            YoctoNearToken::from(10)
        );
    }

    #[test]
    fn test_near_token_div() {
        assert_eq!(
            YoctoNearToken::zero() / YoctoNearToken::one(),
            YoctoNearToken::zero()
        );
        assert_eq!(
            YoctoNearToken::one() / YoctoNearToken::one(),
            YoctoNearToken::one()
        );
        assert_eq!(
            YoctoNearToken::one() / YoctoNearToken::zero(),
            YoctoNearToken::zero()
        );
        assert_eq!(
            YoctoNearToken::from(10) / YoctoNearToken::from(2),
            YoctoNearToken::from(5)
        );
        assert_eq!(
            YoctoNearToken::from(10) / YoctoNearToken::from(3),
            YoctoNearToken::from(3)
        );
    }

    #[test]
    fn test_near_token_json_serialization() {
        // ゼロ値のテスト
        let token = YoctoNearToken::zero();
        let json = serde_json::to_string(&token).unwrap();
        assert_eq!(json, "\"0\"");
        let deserialized: YoctoNearToken = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, token);

        // 1 Yocto NEAR のテスト
        let token = YoctoNearToken::one();
        let json = serde_json::to_string(&token).unwrap();
        assert_eq!(json, "\"1\"");
        let deserialized: YoctoNearToken = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, token);

        // 1 NEAR のテスト
        let token = YoctoNearToken::from_near(BigDecimal::from(1));
        let json = serde_json::to_string(&token).unwrap();
        assert_eq!(json, "\"1000000000000000000000000\"");
        let deserialized: YoctoNearToken = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, token);

        // 大きな値のテスト
        let token = YoctoNearToken::from_yocto(u128::MAX);
        let json = serde_json::to_string(&token).unwrap();
        assert_eq!(json, "\"340282366920938463463374607431768211455\"");
        let deserialized: YoctoNearToken = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, token);
    }

    #[test]
    fn test_near_token_json_deserialization_error() {
        // 不正な入力値のテスト
        let result: Result<YoctoNearToken, _> = serde_json::from_str("\"invalid\"");
        assert!(result.is_err());

        // 負の値のテスト（u128 では負の値は表現できない）
        let result: Result<YoctoNearToken, _> = serde_json::from_str("\"-1\"");
        assert!(result.is_err());
    }

    #[test]
    fn test_near_token_string_conversion() {
        // ゼロ値のテスト
        let token = YoctoNearToken::zero();
        let s = token.to_string();
        assert_eq!(s, "0");
        let parsed = YoctoNearToken::from_str(&s).unwrap();
        assert_eq!(parsed, token);

        // 1 Yocto NEAR のテスト
        let token = YoctoNearToken::one();
        let s = token.to_string();
        assert_eq!(s, "1");
        let parsed = YoctoNearToken::from_str(&s).unwrap();
        assert_eq!(parsed, token);

        // 1 NEAR のテスト
        let token = YoctoNearToken::from_near(BigDecimal::from(1));
        let s = token.to_string();
        assert_eq!(s, "1000000000000000000000000");
        let parsed = YoctoNearToken::from_str(&s).unwrap();
        assert_eq!(parsed, token);

        // 大きな値のテスト
        let token = YoctoNearToken::from_yocto(u128::MAX);
        let s = token.to_string();
        assert_eq!(s, "340282366920938463463374607431768211455");
        let parsed = YoctoNearToken::from_str(&s).unwrap();
        assert_eq!(parsed, token);

        // パース失敗のテスト
        assert!(YoctoNearToken::from_str("invalid").is_err());
        assert!(YoctoNearToken::from_str("-1").is_err());
    }
}
