use crate::ref_finance::errors::Error;
use crate::ref_finance::pool_info::{PoolInfo, TokenPair};
use crate::Result;
use near_primitives::types::AccountId;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::sync::{Arc, Mutex};

const AMOUNT_IN: u128 = 1_000_000_000_000_000_000; // 1e18

#[derive(Debug, Clone)]
pub struct Edge {
    pool: Arc<EdgesInSamePool>,
    pair: TokenPair,
    estimated_return: Option<u128>,
}

impl Edge {
    fn reversed(&self) -> Arc<Self> {
        self.pool
            .get_path(self.pair.token_out, self.pair.token_in)
            .expect("should be valid index")
    }
}

impl PartialEq for Edge {
    fn eq(&self, other: &Self) -> bool {
        self.pair == other.pair
    }
}
impl Eq for Edge {}

#[derive(Debug)]
struct EdgesInSamePool {
    pool: Arc<PoolInfo>,
    cached_edges: Mutex<HashMap<(usize, usize), Arc<Edge>>>,
}

impl EdgesInSamePool {
    #[allow(dead_code)]
    pub fn new(pool: Arc<PoolInfo>) -> Self {
        Self {
            pool,
            cached_edges: Mutex::new(HashMap::new()),
        }
    }

    pub fn get_path(self: &Arc<Self>, token_in: usize, token_out: usize) -> Result<Arc<Edge>> {
        let mut cached_edges = self.cached_edges.lock().unwrap();
        let key = (token_in, token_out);
        if let Some(path) = cached_edges.get(&key) {
            return Ok(Arc::clone(path));
        }
        let pair = self.pool.get_pair(token_in, token_out)?;
        let er = pair.estimate_return(AMOUNT_IN);
        let path = Arc::new(Edge {
            pool: Arc::clone(self),
            pair,
            estimated_return: er.ok(),
        });
        cached_edges.insert(key, Arc::clone(&path));
        Ok(path)
    }
}

mod one_step {
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
        token_in_id: AccountId,
        token_out_id: AccountId,
        pairs: BinaryHeap<SamePathEdge>,
    }

    impl PathEdges {
        #[allow(dead_code)]
        pub fn new(token_in_id: AccountId, token_out_id: AccountId) -> Self {
            Self {
                token_in_id,
                token_out_id,
                pairs: BinaryHeap::new(),
            }
        }

        #[allow(dead_code)]
        pub fn push(&mut self, path: Arc<Edge>) -> Result<()> {
            if path.pair.token_in_id() != self.token_in_id
                || path.pair.token_out_id() != self.token_out_id
            {
                return Err(Error::UnmatchedTokenPath(
                    (self.token_in_id.clone(), self.token_out_id.clone()),
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
        pub fn at_top(&self) -> Option<Arc<Edge>> {
            self.pairs.peek().map(|e| {
                let edge = &e.0;
                Arc::clone(edge)
            })
        }

        #[allow(dead_code)]
        pub fn reversed(&self) -> Self {
            Self {
                token_in_id: self.token_out_id.clone(),
                token_out_id: self.token_in_id.clone(),
                pairs: self
                    .pairs
                    .iter()
                    .map(|p| SamePathEdge(p.0.reversed()))
                    .collect(),
            }
        }
    }
}
