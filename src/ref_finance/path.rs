use crate::ref_finance::pool_info::PoolInfoList;
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use crate::Result;

pub mod by_token;
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
