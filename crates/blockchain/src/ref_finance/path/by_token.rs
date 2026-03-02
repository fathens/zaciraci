use crate::ref_finance::path::edge;
use common::types::TokenAccount;
use common::types::{TokenInAccount, TokenOutAccount};
use dex::PoolInfoList;
use logging::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

type EdgesByToken = HashMap<TokenOutAccount, edge::one_step::PathEdges>;
pub struct PoolsByToken {
    by_in: HashMap<TokenInAccount, Vec<Arc<edge::same_pool::CachedEdges>>>,
    cached_by_out: Mutex<HashMap<TokenInAccount, Arc<EdgesByToken>>>,
}

impl PoolsByToken {
    pub fn new(pool_list: Arc<PoolInfoList>) -> Self {
        let mut by_in = HashMap::new();
        for pool in pool_list.iter().filter(|pool| pool.is_simple()) {
            for token in pool.tokens() {
                by_in
                    .entry(token.to_in())
                    .or_insert_with(Vec::new)
                    .push(edge::same_pool::CachedEdges::new(Arc::clone(pool)));
            }
        }
        Self {
            by_in,
            cached_by_out: Mutex::new(HashMap::new()),
        }
    }

    pub fn tokens(&self) -> Vec<TokenAccount> {
        self.by_in.keys().map(|ta| ta.inner().clone()).collect()
    }

    pub fn get_groups_by_out(&self, token_in: &TokenInAccount) -> Arc<EdgesByToken> {
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

    fn group_by_out(&self, token_in: &TokenInAccount) -> Option<EdgesByToken> {
        let log = DEFAULT.new(o!(
            "function" => "group_by_out",
            "token_in" => token_in.to_string(),
        ));
        trace!(log, "finding edges");

        self.by_in.get(token_in).map(|edges| {
            let mut edges_by_token_out = HashMap::new();
            for edge in edges.iter() {
                for token_out in edge.pool.tokens().filter(|&t| t != token_in.inner()) {
                    let token_out: TokenOutAccount = token_out.to_out();
                    let log = log.new(o!(
                        "token_out" => token_out.to_string(),
                    ));
                    debug!(log, "finding edge");
                    match &edge.get_by_ids(token_in, &token_out) {
                        Ok(edge) => edges_by_token_out
                            .entry(token_out.clone())
                            .or_insert_with(|| {
                                edge::one_step::PathEdges::new(token_in.clone(), token_out.clone())
                            })
                            .push(Arc::clone(edge))
                            .expect("should be same path"),
                        Err(e) => trace!(log, "no edge found"; "error" => %e),
                    }
                }
            }
            edges_by_token_out
        })
    }
}
