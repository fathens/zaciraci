use num_traits::{One, Zero};
use std::ops::{Add, Div, Mul, Sub};

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct MilliNear(u32);

impl MilliNear {
    const IN_YOCTO: u128 = 1_000_000_000_000_000_000_000_000; // 1e24

    pub fn from_yocto(yocto: u128) -> Self {
        MilliNear((yocto / Self::IN_YOCTO) as u32)
    }

    pub fn to_yocto(&self) -> u128 {
        self.0 as u128 * Self::IN_YOCTO
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
        MilliNear(self.0 - other.0)
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
        MilliNear((self.0 as u64 / other.0 as u64) as u32)
    }
}
