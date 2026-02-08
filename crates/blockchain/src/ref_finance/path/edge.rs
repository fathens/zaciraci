use crate::ref_finance::pool_info::TokenPair;
use logging::*;
use std::sync::{Arc, Mutex};

pub mod one_step;
pub mod same_pool;
pub mod weight;

pub use weight::EdgeWeight;

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
