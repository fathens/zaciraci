use crate::ref_finance::pool_info::PoolInfoList;
use near_sdk::AccountId;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

mod edge;
mod graph;

type EdgesByToken = HashMap<AccountId, edge::one_step::PathEdges>;
pub struct PoolsByToken {
    by_in: HashMap<AccountId, Vec<Arc<edge::same_pool::CachedEdges>>>,
    cached_by_out: Mutex<HashMap<AccountId, Arc<EdgesByToken>>>,
}

#[allow(dead_code)]
impl PoolsByToken {
    pub fn new(pool_list: PoolInfoList) -> Self {
        let mut by_in = HashMap::new();
        pool_list.iter().for_each(|pool| {
            pool.tokens().for_each(|token| {
                by_in
                    .entry(token.clone())
                    .or_insert_with(Vec::new)
                    .push(edge::same_pool::CachedEdges::new(Arc::clone(pool)));
            });
        });
        Self {
            by_in,
            cached_by_out: Mutex::new(HashMap::new()),
        }
    }

    pub fn tokens(&self) -> Vec<AccountId> {
        self.by_in.keys().cloned().collect()
    }

    pub fn get_groups_by_out(&self, token_in: &AccountId) -> Arc<EdgesByToken> {
        self.cached_by_out
            .lock()
            .map(|mut cached_map| {
                if let Some(cached) = cached_map.get(token_in) {
                    return Arc::clone(cached);
                }
                let result = self.group_by_out(token_in).unwrap_or_default();
                let cache = Arc::new(result);
                cached_map.insert(token_in.clone(), Arc::clone(&cache));
                cache
            })
            .unwrap_or_default()
    }

    fn group_by_out(&self, token_in: &AccountId) -> Option<EdgesByToken> {
        self.by_in.get(token_in).map(|edges| {
            let mut edges_by_token_out = HashMap::new();
            edges.iter().for_each(|edge| {
                edge.pool
                    .tokens()
                    .filter(|&t| t != token_in)
                    .for_each(|token_out| {
                        edges_by_token_out
                            .entry(token_out.clone())
                            .or_insert_with(|| {
                                edge::one_step::PathEdges::new(token_in.clone(), token_out.clone())
                            })
                            .push(Arc::clone(
                                &edge
                                    .get_by_ids(token_in, token_out)
                                    .expect("should be valid tokens"),
                            ))
                            .expect("should be same path")
                    });
            });
            edges_by_token_out
        })
    }
}
