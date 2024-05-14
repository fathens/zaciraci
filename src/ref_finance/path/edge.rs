use crate::ref_finance::errors::Error;
use crate::ref_finance::pool_info::{PoolInfo, TokenPair};
use near_primitives::types::AccountId;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::sync::{Arc, Mutex};

const AMOUNT_IN: u128 = 1_000_000_000_000_000_000; // 1e18

#[derive(Debug, Clone)]
pub struct Edge {
    cache: Arc<same_pool::CachedEdges>,
    pair: TokenPair,
    estimated_return: Option<u128>,
}

impl Edge {
    fn reversed(&self) -> Arc<Self> {
        self.cache
            .get(self.pair.token_out, self.pair.token_in)
            .expect("should be valid index")
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

    #[derive(Debug)]
    pub struct CachedEdges {
        pub pool: Arc<PoolInfo>,
        cached_edges: Mutex<HashMap<(usize, usize), Arc<Edge>>>,
    }

    impl CachedEdges {
        pub fn new(pool: Arc<PoolInfo>) -> Arc<Self> {
            Arc::new(Self {
                pool,
                cached_edges: Mutex::new(HashMap::new()),
            })
        }

        pub fn get_token_id(&self, token: &AccountId) -> Option<usize> {
            self.pool.tokens().position(|t| t == token)
        }

        pub fn get_by_ids(
            self: &Arc<Self>,
            token_in: &AccountId,
            token_out: &AccountId,
        ) -> Option<Arc<Edge>> {
            let token_in = self.get_token_id(token_in)?;
            let token_out = self.get_token_id(token_out)?;
            self.get(token_in, token_out).ok()
        }

        pub fn get(
            self: &Arc<Self>,
            token_in: usize,
            token_out: usize,
        ) -> crate::Result<Arc<Edge>> {
            let mut cached_edges = self.cached_edges.lock().unwrap();
            let key = (token_in, token_out);
            if let Some(path) = cached_edges.get(&key) {
                return Ok(Arc::clone(path));
            }
            let pair = self.pool.get_pair(token_in, token_out)?;
            let er = pair.estimate_return(AMOUNT_IN);
            let path = Arc::new(Edge {
                cache: Arc::clone(self),
                pair,
                estimated_return: er.ok(),
            });
            cached_edges.insert(key, Arc::clone(&path));
            Ok(path)
        }
    }
}

pub mod one_step {
    use super::*;

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
        pub token_in_out: (AccountId, AccountId),
        pairs: BinaryHeap<SamePathEdge>,

        cached_is_stop: Arc<Mutex<Option<bool>>>,
    }

    impl PathEdges {
        pub fn new(token_in_id: AccountId, token_out_id: AccountId) -> Self {
            Self {
                token_in_out: (token_in_id, token_out_id),
                pairs: BinaryHeap::new(),
                cached_is_stop: Arc::new(Mutex::new(None)),
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

        #[allow(dead_code)]
        pub fn is_stop(&self) -> bool {
            let mut cached_is_stop = self.cached_is_stop.lock().unwrap();
            if let Some(is_stop) = *cached_is_stop {
                return is_stop;
            }
            let calc = || -> bool {
                if self.pairs.len() <= 1 {
                    return true;
                }
                let top = self.pairs.peek().unwrap();
                let bottom = self.pairs.iter().last().unwrap();
                top.0.estimated_return == bottom.0.estimated_return
            };
            let reslut = calc();
            *cached_is_stop = Some(reslut);
            reslut
        }

        #[allow(dead_code)]
        pub fn at_top(&self) -> Option<Arc<Edge>> {
            self.pairs.peek().map(|e| {
                let edge = &e.0;
                Arc::clone(edge)
            })
        }

        #[allow(dead_code)]
        pub fn reversed(&self) -> Self {
            Self {
                token_in_out: (self.token_in_out.1.clone(), self.token_in_out.0.clone()),
                pairs: self
                    .pairs
                    .iter()
                    .map(|p| SamePathEdge(p.0.reversed()))
                    .collect(),
                cached_is_stop: self.cached_is_stop.clone(),
            }
        }
    }
}
