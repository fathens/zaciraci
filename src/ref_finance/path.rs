use crate::ref_finance::pool_info::{PoolInfo, TokenPair};
use crate::Result;
use std::collections::{BinaryHeap, HashMap};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
struct PoolEdges {
    pool: Arc<PoolInfo>,

    cached_edges: Mutex<HashMap<(usize, usize), Arc<Edge>>>,
}

#[derive(Debug, Clone)]
pub struct Edge {
    pool: Arc<PoolEdges>,
    pair: TokenPair,
    estimated_return: Option<u128>,
}

#[derive(Debug, Clone)]
pub struct PathGroup {
    pairs: BinaryHeap<Arc<Edge>>,
}

impl PoolEdges {
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
        let er = pair.estimate_return(Edge::AMOUNT_IN);
        let path = Arc::new(Edge {
            pool: Arc::clone(self),
            pair,
            estimated_return: er.ok(),
        });
        cached_edges.insert(key, Arc::clone(&path));
        Ok(path)
    }
}

impl Edge {
    const AMOUNT_IN: u128 = 1_000_000_000_000_000_000; // 1e18

    fn reversed(&self) -> Arc<Self> {
        self.pool
            .get_path(self.pair.token_out, self.pair.token_in)
            .expect("should be valid index")
    }
}

impl PartialEq for Edge {
    fn eq(&self, other: &Self) -> bool {
        self.estimated_return == other.estimated_return
    }
}
impl Eq for Edge {}

impl PartialOrd for Edge {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Edge {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.estimated_return.cmp(&other.estimated_return)
    }
}

#[allow(dead_code)]
impl PathGroup {
    pub fn new() -> Self {
        Self {
            pairs: BinaryHeap::new(),
        }
    }

    pub fn push(&mut self, path: Arc<Edge>) {
        self.pairs.push(path);
    }

    pub fn at_top(&self) -> Option<Arc<Edge>> {
        self.pairs.peek().cloned()
    }

    #[allow(dead_code)]
    pub fn reversed(&self) -> Self {
        Self {
            pairs: self.pairs.iter().map(|p| p.reversed()).collect(),
        }
    }
}
