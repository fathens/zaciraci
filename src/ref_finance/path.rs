use crate::ref_finance::pool_info::{PoolInfo, TokenPair};
use crate::Result;
use std::collections::{BinaryHeap, HashMap};
use std::sync::Mutex;

#[derive(Debug)]
struct PathPool<'a> {
    pool: &'a PoolInfo,

    cached_path: Mutex<HashMap<(usize, usize), Box<Path<'a>>>>,
}

#[derive(Debug, Clone)]
pub struct Path<'a> {
    pool: &'a PathPool<'a>,
    pair: TokenPair<'a>,
    estimated_return: Option<u128>,
}

#[derive(Debug)]
pub struct PathGroup<'a> {
    pairs: BinaryHeap<Box<Path<'a>>>,
}

impl<'a> PathPool<'a> {
    pub fn new(pool: &'a PoolInfo) -> Self {
        Self {
            pool,
            cached_path: Mutex::new(HashMap::new()),
        }
    }

    pub fn get_path(&'a self, token_in: usize, token_out: usize) -> Result<Box<Path<'a>>> {
        let mut cached_path = self.cached_path.lock().unwrap();
        let key = (token_in, token_out);
        if let Some(path) = cached_path.get(&key) {
            return Ok(path.clone());
        }
        let pair = self.pool.get_pair(token_in, token_out)?;
        let er = pair.estimate_return(Path::AMOUNT_IN);
        let path = Box::new(Path {
            pool: self,
            pair,
            estimated_return: er.ok(),
        });
        cached_path.insert(key, path.clone());
        Ok(path)
    }
}

impl<'a> Path<'a> {
    const AMOUNT_IN: u128 = 1_000_000_000_000_000_000; // 1e18

    fn reversed(&self) -> Box<Self> {
        self.pool
            .get_path(self.pair.token_out, self.pair.token_in)
            .expect("should be valid index")
    }
}

impl PartialEq for Path<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.estimated_return == other.estimated_return
    }
}
impl Eq for Path<'_> {}

impl PartialOrd for Path<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Path<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.estimated_return.cmp(&other.estimated_return)
    }
}

impl<'a> PathGroup<'a> {
    pub fn new() -> Self {
        Self {
            pairs: BinaryHeap::new(),
        }
    }

    pub fn push(&mut self, path: Box<Path<'a>>) {
        self.pairs.push(path);
    }

    pub fn at_top(&self) -> Option<Box<Path<'a>>> {
        self.pairs.peek().cloned()
    }

    pub fn reversed(&self) -> Self {
        Self {
            pairs: self.pairs.iter().map(|p| p.reversed()).collect(),
        }
    }
}
