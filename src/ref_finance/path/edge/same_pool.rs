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
