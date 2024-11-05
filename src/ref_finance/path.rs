use crate::ref_finance::pool_info::{PoolInfoList, TokenPair};
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use crate::Result;
use async_once_cell::Lazy;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

mod by_token;
mod edge;
mod graph;

use graph::TokenGraph;

struct FutureTokenGraph(Option<Pin<Box<dyn Future<Output = TokenGraph> + Send>>>);
impl Future for FutureTokenGraph {
    type Output = TokenGraph;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(self.0.get_or_insert_with(|| {
            Box::pin(async {
                let pools = PoolInfoList::load_from_db().await.unwrap();
                TokenGraph::new(pools)
            })
        }))
        .poll(cx)
    }
}

static TOKEN_GRAPH: Lazy<TokenGraph, FutureTokenGraph> = Lazy::new(FutureTokenGraph(None));

pub fn all_tokens(pools: PoolInfoList) -> Vec<TokenAccount> {
    let by_tokens = by_token::PoolsByToken::new(pools);
    by_tokens.tokens()
}

pub async fn sorted_returns(
    start: TokenInAccount,
    initial: u128,
) -> Result<Vec<(TokenOutAccount, u128)>> {
    let graph = TOKEN_GRAPH.get_unpin().await;
    graph.list_returns(initial, start)
}

pub async fn swap_path(start: TokenInAccount, goal: TokenOutAccount) -> Result<Vec<TokenPair>> {
    let graph = TOKEN_GRAPH.get_unpin().await;
    graph.get_path_with_return(start, goal)
}
