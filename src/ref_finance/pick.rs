#![allow(dead_code)]

use crate::ref_finance::history;
use crate::ref_finance::path::graph::TokenGraph;
use crate::ref_finance::pool_info::PoolInfoList;
use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount};
use crate::Result;

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

pub async fn pick_pools(start: TokenInAccount) -> Result<Vec<Preview>> {
    let all_pools = PoolInfoList::read_from_node().await?;
    let graph = TokenGraph::new(all_pools);
    let stats_ave = history::get_history().read().unwrap().inputs.average();
    let mut values_a = stats_ave / 2;
    let mut values_b = stats_ave * 2;
    let mut previews_a = vec![];
    let mut previews_b = vec![];

    while values_a < values_b {
        previews_a = pick_path(&graph, start.clone(), values_a)?;
        previews_b = pick_path(&graph, start.clone(), values_b)?;
    }
    todo!("Implement the rest of the function");
}

fn pick_path(pools: &TokenGraph, start: TokenInAccount, amount: u128) -> Result<Vec<Preview>> {
    let list = pools.list_returns(amount, start.clone())?;
    let mut goals = vec![];
    for (goal, output) in list.into_iter() {
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
