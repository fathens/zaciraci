use crate::logging::*;
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

pub fn run_swap(
    pools: PoolInfoList,
    start: TokenInAccount,
    goal: TokenOutAccount,
    initial: u128,
) -> Result<u128> {
    let log = DEFAULT.new(o!(
        "function" => "run_swap",
        "start" => format!("{}", start),
        "goal" => format!("{}", goal),
        "initial" => initial,
    ));
    let graph = graph::TokenGraph::new(pools);
    let path = graph.get_path_with_return(start, goal)?;
    debug!(log, "path"; "path" => format!("{:?}", path));
    todo!("run_swap")
}
