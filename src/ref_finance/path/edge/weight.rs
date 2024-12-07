use crate::ref_finance::pool_info::TokenPairId;
use num_bigint::{BigUint, ToBigUint};
use num_rational::Ratio;
use num_traits::{one, zero, ToPrimitive};
use std::cmp::Ordering;
use std::ops::Add;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub struct EdgeWeight {
    pair_id: Option<TokenPairId>,
    estimated_rate: Ratio<u128>,
}

impl EdgeWeight {
    fn calc_rate(input_value: u128, estimated_return: u128) -> Ratio<u128> {
        if input_value == 0 {
            zero()
        } else {
            Ratio::new(estimated_return, input_value)
        }
    }

    pub fn new(pair_id: TokenPairId, input_value: u128, estimated_return: u128) -> Self {
        Self {
            pair_id: Some(pair_id),
            estimated_rate: Self::calc_rate(input_value, estimated_return),
        }
    }

    pub fn pair_id(&self) -> Option<TokenPairId> {
        self.pair_id
    }
}

impl Ord for EdgeWeight {
    fn cmp(&self, other: &Self) -> Ordering {
        self.estimated_rate.cmp(&other.estimated_rate).reverse() // レートが大きいほど望ましい
    }
}

impl PartialOrd for EdgeWeight {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Default for EdgeWeight {
    fn default() -> Self {
        EdgeWeight {
            pair_id: None,
            estimated_rate: one(),
        }
    }
}

#[allow(clippy::suspicious_arithmetic_impl)]
impl Add<EdgeWeight> for EdgeWeight {
    type Output = Self;
    fn add(self, rhs: EdgeWeight) -> Self::Output {
        fn to_big_rational(src: Ratio<u128>) -> Ratio<BigUint> {
            Ratio::new(
                src.numer().to_biguint().unwrap(),
                src.denom().to_biguint().unwrap(),
            )
        }
        fn to_u128(src: Ratio<BigUint>) -> (u128, u128) {
            let fv = src.to_f64().expect("should be valid");
            let src: Ratio<i128> = Ratio::approximate_float(fv).expect("should be valid");
            (
                src.numer().to_u128().expect("should be valid"),
                src.denom().to_u128().expect("should be valid"),
            )
        }
        let (n, d) =
            to_u128(to_big_rational(self.estimated_rate) * to_big_rational(rhs.estimated_rate));
        EdgeWeight {
            pair_id: None,
            estimated_rate: Self::calc_rate(d, n),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_calc_rate() {
        assert_eq!(EdgeWeight::calc_rate(1, 1), Ratio::new(1, 1));
        assert_eq!(EdgeWeight::calc_rate(1, 2), Ratio::new(2, 1));
        assert_eq!(EdgeWeight::calc_rate(2, 2), Ratio::new(2, 2));
        assert_eq!(EdgeWeight::calc_rate(2, 0), Ratio::new(0, 1));
    }

    #[test]
    fn test_add() {
        fn weight(d: u128, n: u128) -> EdgeWeight {
            EdgeWeight {
                pair_id: None,
                estimated_rate: EdgeWeight::calc_rate(d, n),
            }
        }

        assert_eq!(
            (weight(1, 1) + weight(1, 1)).estimated_rate,
            EdgeWeight::calc_rate(1, 1)
        );
        assert_eq!(
            (weight(1, 2) + weight(2, 1)).estimated_rate,
            EdgeWeight::calc_rate(1, 1)
        );
    }
}
