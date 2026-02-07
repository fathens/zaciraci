use near_sdk::NearToken;
use num_traits::{One, Zero};
use std::ops::{Add, Div, Mul, Sub};

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct MilliNear(u32);

impl MilliNear {
    pub const fn of(value: u32) -> Self {
        MilliNear(value)
    }

    pub const fn from_yocto(yocto: u128) -> Self {
        let n = NearToken::from_yoctonear(yocto);
        MilliNear(n.as_millinear() as u32)
    }

    pub const fn to_yocto(self) -> u128 {
        let n = NearToken::from_millinear(self.0 as u128);
        n.as_yoctonear()
    }

    pub const fn from_near(near: u128) -> Self {
        let n = NearToken::from_near(near);
        MilliNear(n.as_millinear() as u32)
    }
}

impl From<NearToken> for MilliNear {
    fn from(token: NearToken) -> Self {
        MilliNear::from_yocto(token.as_yoctonear())
    }
}

impl From<MilliNear> for NearToken {
    fn from(milli: MilliNear) -> Self {
        NearToken::from_yoctonear(milli.to_yocto())
    }
}

impl From<u128> for MilliNear {
    fn from(yocto: u128) -> Self {
        MilliNear::from_yocto(yocto)
    }
}

impl From<MilliNear> for u128 {
    fn from(milli: MilliNear) -> Self {
        milli.to_yocto()
    }
}

impl Zero for MilliNear {
    fn zero() -> Self {
        MilliNear(0)
    }

    fn is_zero(&self) -> bool {
        self.0 == 0
    }
}

impl One for MilliNear {
    fn one() -> Self {
        MilliNear(1)
    }
}

impl Add<MilliNear> for MilliNear {
    type Output = MilliNear;

    fn add(self, other: MilliNear) -> Self::Output {
        MilliNear(self.0 + other.0)
    }
}

impl Sub<MilliNear> for MilliNear {
    type Output = MilliNear;

    fn sub(self, other: MilliNear) -> Self::Output {
        if self.0 < other.0 {
            MilliNear(0)
        } else {
            MilliNear(self.0 - other.0)
        }
    }
}

impl Mul<MilliNear> for MilliNear {
    type Output = MilliNear;

    fn mul(self, other: MilliNear) -> Self::Output {
        MilliNear((self.0 as u64 * other.0 as u64) as u32)
    }
}

impl Div<MilliNear> for MilliNear {
    type Output = MilliNear;

    fn div(self, other: MilliNear) -> Self::Output {
        if other.0 == 0 {
            MilliNear::zero()
        } else {
            MilliNear((self.0 as u64 / other.0 as u64) as u32)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_milli_near_transform() {
        // copy from near-token-0.3.0
        const ONE_NEAR: u128 = 10_u128.pow(24);
        const ONE_MILLINEAR: u128 = 10_u128.pow(21);

        let zero = MilliNear::zero();
        assert_eq!(zero, MilliNear::from_yocto(0));
        assert_eq!(zero.to_yocto(), 0);

        let one_yocto = MilliNear::from_yocto(1);
        assert_eq!(one_yocto, MilliNear::zero());
        assert_eq!(one_yocto.to_yocto(), 0);

        let one_milli = MilliNear::one();
        assert_eq!(one_milli, MilliNear::one());
        assert_eq!(one_milli, MilliNear::from_yocto(ONE_MILLINEAR));
        assert_eq!(one_milli.to_yocto(), ONE_MILLINEAR);

        let one_near = MilliNear::from_yocto(ONE_NEAR);
        assert_eq!(one_near, MilliNear::of(1_000));
        assert_eq!(one_near.to_yocto(), ONE_NEAR);

        let ten_near = MilliNear::from_near(10);
        assert_eq!(ten_near, MilliNear::of(10_000));
        assert_eq!(ten_near.to_yocto(), 10 * ONE_NEAR);
    }

    #[test]
    fn test_milli_near_add() {
        assert_eq!(MilliNear::zero() + MilliNear::zero(), MilliNear::zero());
        assert_eq!(MilliNear::zero() + MilliNear::one(), MilliNear::one());
        assert_eq!(MilliNear::one() + MilliNear::zero(), MilliNear::one());
        assert_eq!(MilliNear::one() + MilliNear::one(), MilliNear(2));
    }

    #[test]
    fn test_milli_near_sub() {
        assert_eq!(MilliNear::zero() - MilliNear::zero(), MilliNear::zero());
        assert_eq!(MilliNear::zero() - MilliNear::one(), MilliNear::zero());
        assert_eq!(MilliNear::one() - MilliNear::zero(), MilliNear::one());
        assert_eq!(MilliNear::one() - MilliNear::one(), MilliNear::zero());
    }

    #[test]
    fn test_milli_near_mul() {
        assert_eq!(MilliNear::zero() * MilliNear::zero(), MilliNear::zero());
        assert_eq!(MilliNear::zero() * MilliNear::one(), MilliNear::zero());
        assert_eq!(MilliNear::one() * MilliNear::zero(), MilliNear::zero());
        assert_eq!(MilliNear::one() * MilliNear::one(), MilliNear::one());
        assert_eq!(MilliNear(2) * MilliNear(3), MilliNear(6));
    }

    #[test]
    fn test_milli_near_div() {
        assert_eq!(MilliNear::zero() / MilliNear::one(), MilliNear::zero());
        assert_eq!(MilliNear::one() / MilliNear::one(), MilliNear::one());
        assert_eq!(MilliNear::one() / MilliNear::zero(), MilliNear::zero());
        assert_eq!(MilliNear(6) / MilliNear(3), MilliNear(2));
    }
}
