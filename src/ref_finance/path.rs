use crate::ref_finance::pool_info::PoolInfoList;
use near_sdk::AccountId;
use std::collections::HashMap;
use std::sync::Arc;

mod edge;

pub struct PoolsByToken {
    pools: HashMap<AccountId, Vec<Arc<edge::same_pool::CachedEdges>>>,
}

#[allow(dead_code)]
impl PoolsByToken {
    pub fn new(pool_list: PoolInfoList) -> Self {
        let mut pools = HashMap::new();
        pool_list.iter().for_each(|pool| {
            pool.tokens().for_each(|token| {
                pools
                    .entry(token.clone())
                    .or_insert_with(Vec::new)
                    .push(edge::same_pool::CachedEdges::new(Arc::clone(pool)));
            });
        });
        Self { pools }
    }

    pub fn group_by_out(
        &self,
        token_in: &AccountId,
    ) -> HashMap<AccountId, edge::one_step::PathEdges> {
        self.pools
            .get(token_in)
            .map(|edges| {
                let mut edges_by_token_out = HashMap::new();
                edges.iter().for_each(|edge| {
                    edge.pool
                        .tokens()
                        .filter(|t| *t != token_in)
                        .for_each(|token_out| {
                            edges_by_token_out
                                .entry(token_out.clone())
                                .or_insert_with(|| {
                                    edge::one_step::PathEdges::new(
                                        token_in.clone(),
                                        token_out.clone(),
                                    )
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
            .unwrap_or_default()
    }
}
