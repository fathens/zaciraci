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

pub mod same_pool {
    use super::*;
    use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
    use crate::ref_finance::token_index::{TokenIn, TokenIndex, TokenOut};

    #[derive(Debug)]
    pub struct CachedEdges {
        pub pool: Arc<PoolInfo>,
        cached_edges: Mutex<HashMap<(TokenIn, TokenOut), Arc<Edge>>>,
    }

    impl CachedEdges {
        pub fn new(pool: Arc<PoolInfo>) -> Arc<Self> {
            Arc::new(Self {
                pool,
                cached_edges: Mutex::new(HashMap::new()),
            })
        }

        pub fn get_token_id(&self, token: &TokenAccount) -> Option<TokenIndex> {
            self.pool
                .tokens()
                .position(|t| t == token)
                .map(|a| a.into())
        }

        pub fn get_by_ids(
            self: &Arc<Self>,
            token_in: &TokenInAccount,
            token_out: &TokenOutAccount,
        ) -> crate::Result<Arc<Edge>> {
            let log = DEFAULT.new(o!(
                "function" => "get_by_ids",
                "token_in" => token_in.to_string(),
                "token_out" => token_out.to_string(),
            ));
            debug!(log, "converting to index");
            let token_in = self
                .get_token_id(token_in.as_account())
                .ok_or_else(|| Error::TokenNotFound(token_in.as_account().clone()))?;
            let token_out = self
                .get_token_id(token_out.as_account())
                .ok_or_else(|| Error::TokenNotFound(token_out.as_account().clone()))?;
            debug!(log, "index";
                "token_in" => token_in.to_string(),
                "token_out" => token_out.to_string(),
            );
            self.get(token_in.into(), token_out.into())
        }

        pub(super) fn get(
            self: &Arc<Self>,
            token_in: TokenIn,
            token_out: TokenOut,
        ) -> crate::Result<Arc<Edge>> {
            let mut cached_edges = self.cached_edges.lock().unwrap();
            let key = (token_in, token_out);
            if let Some(path) = cached_edges.get(&key) {
                return Ok(Arc::clone(path));
            }
            let pair = self.pool.get_pair(token_in, token_out)?;
            pair.estimate_normal_return()
                .map(|(input_value, estimated_return)| {
                    let path = Arc::new(Edge {
                        pair,
                        input_value,
                        estimated_return,
                        cached_weight: Arc::new(Mutex::new(None)),
                    });
                    cached_edges.insert(key, Arc::clone(&path));
                    path
                })
        }
    }
}

pub mod one_step {
    use super::*;
    use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount};

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct SamePathEdge(Arc<Edge>);

    impl PartialOrd for SamePathEdge {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }

    impl Ord for SamePathEdge {
        fn cmp(&self, other: &Self) -> Ordering {
            self.0.estimated_return.cmp(&other.0.estimated_return)
        }
    }

    #[derive(Debug, Clone)]
    pub struct PathEdges {
        pub token_in_out: (TokenInAccount, TokenOutAccount),
        pairs: BinaryHeap<SamePathEdge>,
    }

    impl PathEdges {
        pub fn new(token_in_id: TokenInAccount, token_out_id: TokenOutAccount) -> Self {
            Self {
                token_in_out: (token_in_id, token_out_id),
                pairs: BinaryHeap::new(),
            }
        }

        pub fn push(&mut self, path: Arc<Edge>) -> crate::Result<()> {
            if self.token_in_out
                != (
                    path.pair.token_in_id().clone(),
                    path.pair.token_out_id().clone(),
                )
            {
                return Err(Error::UnmatchedTokenPath(
                    self.token_in_out.clone(),
                    (
                        path.pair.token_in_id().clone(),
                        path.pair.token_out_id().clone(),
                    ),
                )
                .into());
            }
            self.pairs.push(SamePathEdge(path));
            Ok(())
        }

        pub fn at_top(&self) -> Option<Arc<Edge>> {
            self.pairs.peek().map(|e| {
                let edge = &e.0;
                Arc::clone(edge)
            })
        }
    }
}
