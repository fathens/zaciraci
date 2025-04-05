
use std::ops::Add;
use std::ops::Div;
use std::ops::Mul;
use std::ops::Sub;

use bigdecimal::BigDecimal;
use bigdecimal::One;
use bigdecimal::ToPrimitive;
use bigdecimal::Zero;

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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(one_milli.as_near(), BigDecimal::from(1) / BigDecimal::from(1000));
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
        assert_eq!(YoctoNearToken::zero() + YoctoNearToken::zero(), YoctoNearToken::zero());
        assert_eq!(YoctoNearToken::zero() + YoctoNearToken::one(), YoctoNearToken::one());
        assert_eq!(YoctoNearToken::one() + YoctoNearToken::zero(), YoctoNearToken::one());
        assert_eq!(YoctoNearToken::one() + YoctoNearToken::one(), YoctoNearToken::from_yocto(2));
    }

    #[test]
    fn test_near_token_sub() {
        assert_eq!(YoctoNearToken::zero() - YoctoNearToken::zero(), YoctoNearToken::zero());
        assert_eq!(YoctoNearToken::zero() - YoctoNearToken::one(), YoctoNearToken::zero());
        assert_eq!(YoctoNearToken::one() - YoctoNearToken::zero(), YoctoNearToken::one());
        assert_eq!(YoctoNearToken::one() - YoctoNearToken::one(), YoctoNearToken::zero());
    }

    #[test]
    fn test_near_token_mul() {
        assert_eq!(YoctoNearToken::zero() * YoctoNearToken::zero(), YoctoNearToken::zero());
        assert_eq!(YoctoNearToken::zero() * YoctoNearToken::one(), YoctoNearToken::zero());
        assert_eq!(YoctoNearToken::one() * YoctoNearToken::zero(), YoctoNearToken::zero());
        assert_eq!(YoctoNearToken::one() * YoctoNearToken::one(), YoctoNearToken::one());
        assert_eq!(YoctoNearToken::from(5) * YoctoNearToken::from(2), YoctoNearToken::from(10));
    }

    #[test]
    fn test_near_token_div() {
        assert_eq!(YoctoNearToken::zero() / YoctoNearToken::one(), YoctoNearToken::zero());
        assert_eq!(YoctoNearToken::one() / YoctoNearToken::one(), YoctoNearToken::one());
        assert_eq!(YoctoNearToken::one() / YoctoNearToken::zero(), YoctoNearToken::zero());
        assert_eq!(YoctoNearToken::from(10) / YoctoNearToken::from(2), YoctoNearToken::from(5));
        assert_eq!(YoctoNearToken::from(10) / YoctoNearToken::from(3), YoctoNearToken::from(3));
    }
}
