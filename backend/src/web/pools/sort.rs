use std::cmp::Ordering;
use std::sync::Arc;
use crate::ref_finance::pool_info::{PoolInfo, PoolInfoList};

#[allow(dead_code)]
pub(super) struct WithWight<T> {
    pub value: T,
    pub weight: u64
}

impl<T> PartialOrd for WithWight<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for WithWight<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.weight.cmp(&other.weight)
    }
}

impl<T> PartialEq for WithWight<T> {
    fn eq(&self, other: &Self) -> bool {
        self.weight == other.weight
    }
}

impl<T> Eq for WithWight<T> { }

pub fn sort(_pools: Arc<PoolInfoList>) -> Vec<Arc<PoolInfo>> {
    todo!()
}