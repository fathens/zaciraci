use crate::ref_finance::pool_info::PoolInfoList;
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use crate::Result;
use near_primitives::num_rational::BigRational;

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
) -> Result<Vec<(TokenOutAccount, BigRational)>> {
    let by_tokens = by_token::PoolsByToken::new(pools);
    let graph = graph::TokenGraph::new(by_tokens);
    graph.list_returns(start)
}
