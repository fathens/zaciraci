use super::*;
use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount};
use dex::TokenPairLike;
use dex::errors::Error;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

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
