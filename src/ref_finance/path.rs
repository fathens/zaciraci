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

use crate::ref_finance::history;
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

pub struct Preview {
    pub input_value: u128,
    pub token: TokenOutAccount,
    pub depth: usize,
    pub output_value: u128,
}

impl Preview {
    const HEAD: u128 = 270_000_000_000_000_000_000_000;
    const BY_STEP: u128 = 260_000_000_000_000_000_000_000;

    fn cost(&self) -> u128 {
        Self::HEAD + Self::BY_STEP * (self.depth as u128)
    }

    fn gain(&self) -> u128 {
        self.output_value - self.input_value - self.cost()
    }

    fn total_gain(previews: &[Preview]) -> u128 {
        let gains: u128 = previews.iter().map(|p| p.gain()).sum();
        gains - MIN_GAIN
    }
}

const MIN_GAIN: u128 = 1_000_000_000_000_000_000_000_000;

pub async fn pick_pools(start: TokenInAccount, total_amount: u128) -> Result<Vec<Preview>> {
    let all_pools = PoolInfoList::read_from_node().await?;
    let graph = TokenGraph::new(all_pools);
    let stats_ave = history::get_history().read().unwrap().inputs.average();
    let mut values_a = stats_ave / 2;
    let mut values_b = stats_ave * 2;
    let mut previews_a = vec![];
    let mut previews_b = vec![];

    while values_a < values_b {
        previews_a = pick(
            &graph,
            start.clone(),
            values_a,
            (total_amount / values_a) as usize,
        )?;
        previews_b = pick(
            &graph,
            start.clone(),
            values_b,
            (total_amount / values_b) as usize,
        )?;
    }
    todo!("Implement the rest of the function");
}

fn pick(
    pools: &TokenGraph,
    start: TokenInAccount,
    amount: u128,
    limit: usize,
) -> Result<Vec<Preview>> {
    let list = pools.list_returns(amount, start.clone())?;
    let mut goals = vec![];
    for (goal, output) in list.into_iter().take(limit) {
        let path = pools.get_path_with_return(start.clone(), goal.clone())?;
        let preview = Preview {
            input_value: amount,
            token: goal,
            depth: path.len(),
            output_value: output,
        };
        let gain = preview.gain();
        if gain > 0 {
            goals.push(preview);
        } else {
            break;
        }
    }
    if Preview::total_gain(&goals) > 0 {
        Ok(goals)
    } else {
        Ok(vec![])
    }
}
