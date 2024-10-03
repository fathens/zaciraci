use crate::ref_finance::pool_info::PoolInfoList;
use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount};
use crate::Result;
use near_primitives::num_rational::BigRational;

mod by_token;
mod edge;
mod graph;

pub fn sorted_returns(
    pools: PoolInfoList,
    start: TokenInAccount,
) -> Result<Vec<(TokenOutAccount, BigRational)>> {
    let by_tokens = by_token::PoolsByToken::new(pools);
    let graph = graph::TokenGraph::new(by_tokens);
    graph.list_returns(start)
}
