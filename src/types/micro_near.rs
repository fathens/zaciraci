use crate::types::MilliNear;
use near_primitives::types::Balance;
use near_sdk::NearToken;
use num_traits::{One, Zero};
use std::ops::{Add, Div, Mul, Sub};

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct MicroNear(u64);

const ONE_MICRONEAR: u128 = 10_u128.pow(18);

#[allow(dead_code)]
impl MicroNear {
    pub const fn of(value: u64) -> Self {
        MicroNear(value)
    }

    pub const fn from_yocto(yocto: u128) -> Self {
        let m = yocto / ONE_MICRONEAR;
        MicroNear(m as u64)
    }

    pub const fn to_yocto(self) -> u128 {
        self.0 as u128 * ONE_MICRONEAR
    }

    pub const fn from_near(near: u128) -> Self {
        let n = NearToken::from_near(near);
        Self::from_yocto(n.as_yoctonear())
    }

    pub const fn from_milli(v: MilliNear) -> Self {
        Self::from_yocto(v.to_yocto())
    }
}

impl From<Balance> for MicroNear {
    fn from(yocto: u128) -> Self {
        MicroNear::from_yocto(yocto)
    }
}

impl From<MicroNear> for Balance {
    fn from(micro: MicroNear) -> Self {
        micro.to_yocto()
    }
}

impl Zero for MicroNear {
    fn zero() -> Self {
        MicroNear(0)
    }

    fn is_zero(&self) -> bool {
        self.0 == 0
    }
}

impl One for MicroNear {
    fn one() -> Self {
        MicroNear(1)
    }
}

impl Add<MicroNear> for MicroNear {
    type Output = MicroNear;

    fn add(self, other: MicroNear) -> Self::Output {
        MicroNear(self.0 + other.0)
    }
}

impl Sub<MicroNear> for MicroNear {
    type Output = MicroNear;

    fn sub(self, other: MicroNear) -> Self::Output {
        if self.0 < other.0 {
            MicroNear(0)
        } else {
            MicroNear(self.0 - other.0)
        }
    }
}

impl Mul<MicroNear> for MicroNear {
    type Output = MicroNear;

    fn mul(self, other: MicroNear) -> Self::Output {
        MicroNear(self.0 * other.0)
    }
}

impl Div<MicroNear> for MicroNear {
    type Output = MicroNear;

    fn div(self, other: MicroNear) -> Self::Output {
        if other.0 == 0 {
            MicroNear::zero()
        } else {
            MicroNear(self.0 / other.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_micro_near_transform() {
        // copy from near-token-0.3.0
        const ONE_NEAR: u128 = 10_u128.pow(24);
        const ONE_MILLINEAR: u128 = 10_u128.pow(21);

        let zero = MicroNear::zero();
        assert_eq!(zero, MicroNear::from_yocto(0));
        assert_eq!(zero.to_yocto(), 0);

        let one_yocto = MicroNear::from_yocto(1);
        assert_eq!(one_yocto, MicroNear::zero());
        assert_eq!(one_yocto.to_yocto(), 0);

        let one_micro = MicroNear::one();
        assert_eq!(one_micro, MicroNear::one());
        assert_eq!(one_micro, MicroNear::from_yocto(ONE_MICRONEAR));
        assert_eq!(one_micro.to_yocto(), ONE_MICRONEAR);

        let one_near = MicroNear::from_near(1);
        assert_eq!(one_near, MicroNear::of(1_000_000));
        assert_eq!(one_near.to_yocto(), ONE_NEAR);

        let one_milli = MicroNear::from_milli(MilliNear::one());
        assert_eq!(one_milli, MicroNear::of(1_000));
        assert_eq!(one_milli.to_yocto(), ONE_MILLINEAR);
    }

    #[test]
    fn test_micro_near_add() {
        assert_eq!(MicroNear::zero() + MicroNear::zero(), MicroNear::zero());
        assert_eq!(MicroNear::zero() + MicroNear::one(), MicroNear::one());
        assert_eq!(MicroNear::one() + MicroNear::zero(), MicroNear::one());
        assert_eq!(MicroNear::one() + MicroNear::one(), MicroNear::of(2));
    }

    #[test]
    fn test_micro_near_sub() {
        assert_eq!(MicroNear::zero() - MicroNear::zero(), MicroNear::zero());
        assert_eq!(MicroNear::zero() - MicroNear::one(), MicroNear::zero());
        assert_eq!(MicroNear::one() - MicroNear::zero(), MicroNear::one());
        assert_eq!(MicroNear::one() - MicroNear::one(), MicroNear::zero());
    }

    #[test]
    fn test_micro_near_mul() {
        assert_eq!(MicroNear::zero() * MicroNear::zero(), MicroNear::zero());
        assert_eq!(MicroNear::zero() * MicroNear::one(), MicroNear::zero());
        assert_eq!(MicroNear::one() * MicroNear::zero(), MicroNear::zero());
        assert_eq!(MicroNear::one() * MicroNear::one(), MicroNear::one());
        assert_eq!(MicroNear::from(5) * MicroNear::from(2), MicroNear::from(10));
    }

    #[test]
    fn test_micro_near_div() {
        assert_eq!(MicroNear::zero() / MicroNear::one(), MicroNear::zero());
        assert_eq!(MicroNear::one() / MicroNear::one(), MicroNear::one());
        assert_eq!(MicroNear::one() / MicroNear::zero(), MicroNear::zero());
        assert_eq!(MicroNear::from(10) / MicroNear::from(2), MicroNear::from(5));
    }
}
