use crate::ref_finance::pool_info::{PoolInfoList, TokenPairId};
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use crate::Result;

mod by_token;
mod edge;
mod graph;

pub fn all_tokens(pools: PoolInfoList) -> Vec<TokenAccount> {
    let by_tokens = by_token::PoolsByToken::new(pools);
    by_tokens.tokens()
}

pub fn sorted_returns(
    pools: PoolInfoList,
    start: TokenInAccount,
    initial: u128,
) -> Result<Vec<(TokenOutAccount, u128)>> {
    let graph = graph::TokenGraph::new(pools);
    graph.list_returns(initial, start)
}

pub fn swap_path(
    pools: PoolInfoList,
    start: TokenInAccount,
    goal: TokenOutAccount,
) -> Result<Vec<TokenPairId>> {
    let graph = graph::TokenGraph::new(pools);
    let path = graph.get_path_with_return(start, goal)?;
    Ok(path.into_iter().map(|pair| pair.pair_id()).collect())
}
