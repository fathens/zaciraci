use crate::logging::*;
use crate::ref_finance::errors::Error;
use crate::ref_finance::pool_info::{PoolInfo, TokenPair, TokenPairId};
use num_bigint::{BigUint, ToBigUint};
use num_rational::Ratio;
use num_traits::{zero, ToPrimitive};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::ops::Add;
use std::sync::{Arc, Mutex};

pub mod one_step;
pub mod same_pool;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub struct EdgeWeight {
    pair_id: Option<TokenPairId>,
    estimated_rate: Ratio<u128>,
}

impl EdgeWeight {
    pub fn new(pair_id: TokenPairId, input_value: u128, estimated_return: u128) -> Self {
        let estimated_rate = if input_value == 0 {
            zero()
        } else {
            Ratio::new(estimated_return, input_value)
        };
        Self {
            pair_id: Some(pair_id),
            estimated_rate,
        }
    }

    pub fn without_token(input_value: u128, estimated_rate: u128) -> Self {
        let estimated_rate = if input_value == 0 {
            zero()
        } else {
            Ratio::new(estimated_rate, input_value)
        };
        Self {
            pair_id: None,
            estimated_rate,
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
        EdgeWeight::without_token(1, 1)
    }
}

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
            to_u128(to_big_rational(self.estimated_rate) + to_big_rational(rhs.estimated_rate));
        EdgeWeight::without_token(d, n)
    }
}

#[derive(Debug, Clone)]
pub struct Edge {
    pair: TokenPair,

    input_value: u128,
    estimated_return: u128,

    cached_weight: Arc<Mutex<Option<EdgeWeight>>>,
}

impl Edge {
    pub fn weight(&self) -> EdgeWeight {
        let mut cached_weight = self.cached_weight.lock().unwrap();
        if let Some(weight) = *cached_weight {
            return weight;
        }
        let weight = EdgeWeight::new(self.pair.pair_id(), self.input_value, self.estimated_return);
        *cached_weight = Some(weight);
        weight
    }
}

impl PartialEq for Edge {
    fn eq(&self, other: &Self) -> bool {
        self.pair == other.pair
    }
}
impl Eq for Edge {}
