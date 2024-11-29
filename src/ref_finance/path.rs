#![allow(dead_code)]

use crate::ref_finance::history;
use crate::ref_finance::pool_info::{PoolInfoList, TokenPair};
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use crate::Result;
use std::collections::HashMap;

mod by_token;
mod edge;
mod graph;

use graph::TokenGraph;

const DEFAULT_AMOUNT_IN: u128 = 1_000_000_000_000_000_000; // 1e18

pub fn all_tokens(pools: &PoolInfoList) -> Vec<TokenAccount> {
    let by_tokens = by_token::PoolsByToken::new(pools, DEFAULT_AMOUNT_IN);
    by_tokens.tokens()
}

pub async fn sorted_returns(
    start: TokenInAccount,
    initial: u128,
) -> Result<Vec<(TokenOutAccount, u128)>> {
    let pools = PoolInfoList::load_from_db().await?;
    let graph = TokenGraph::new(&pools, DEFAULT_AMOUNT_IN);
    graph.list_returns(initial, start)
}

pub async fn swap_path(start: TokenInAccount, goal: TokenOutAccount) -> Result<Vec<TokenPair>> {
    let pools = PoolInfoList::load_from_db().await?;
    let graph = TokenGraph::new(&pools, DEFAULT_AMOUNT_IN);
    graph.get_path_with_return(start, goal)
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Preview {
    pub input_value: u128,
    pub token: TokenOutAccount,
    pub depth: usize,
    pub output_value: u128,
}

#[derive(Clone, Debug)]
struct PreviewList {
    input_value: u128,
    list: Vec<Preview>,
    total_gain: u128,
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
}

impl PreviewList {
    fn new(input_value: u128, previews: &[Preview]) -> Self {
        if previews.is_empty() {
            return PreviewList {
                input_value,
                list: vec![],
                total_gain: 0,
            };
        }
        let gains: u128 = previews.iter().map(|p| p.gain()).sum();
        let total_gain = gains - MIN_GAIN;
        PreviewList {
            input_value,
            list: previews.to_vec(),
            total_gain,
        }
    }
}

const MIN_GAIN: u128 = 1_000_000_000_000_000_000_000_000;

pub async fn pick_pools(start: TokenInAccount, total_amount: u128) -> Result<Option<Vec<Preview>>> {
    let all_pools = PoolInfoList::read_from_node().await?;
    let stats_ave = history::get_history().read().unwrap().inputs.average();

    let do_pick = |value: u128| -> Result<Option<PreviewList>> {
        let limit = (total_amount / value) as usize;
        if limit > 0 {
            let graph = TokenGraph::new(&all_pools, value);
            let previews = pick_by_amount(&graph, start.clone(), value, limit)?;
            if previews.total_gain > 0 {
                return Ok(Some(previews));
            }
        }
        Ok(None)
    };

    let result = search_best_path(1, stats_ave, total_amount, &do_pick, &|a| a.total_gain)?;
    Ok(result.map(|a| a.list))
}

fn pick_by_amount(
    graph: &TokenGraph,
    start: TokenInAccount,
    amount: u128,
    limit: usize,
) -> Result<PreviewList> {
    let list = graph.list_returns(amount, start.clone())?;
    let mut goals = vec![];
    for (goal, output) in list.into_iter().take(limit) {
        let path = graph.get_path_with_return(start.clone(), goal.clone())?;
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
    Ok(PreviewList::new(amount, &goals))
}

fn search_best_path<A, C, G>(
    min: u128,
    average: u128,
    max: u128,
    calc_res: &C,
    get_gain: &G,
) -> Result<Option<A>>
where
    A: Clone,
    C: Fn(u128) -> Result<Option<A>>,
    G: Fn(&A) -> u128,
{
    let mut cached_calc: HashMap<u128, Box<Option<A>>> = HashMap::new();
    let mut calc = |value| -> Result<Box<Option<A>>> {
        if let Some(res) = cached_calc.get(&value) {
            return Ok(res.clone());
        }
        let res = Box::new(calc_res(value)?);
        cached_calc.insert(value, res.clone());
        Ok(res)
    };

    let mut in_a = min;
    let mut in_b = average;
    let mut in_c = max;
    while in_a < in_c {
        let res_a = calc(in_a)?;
        let res_b = calc(in_b)?;
        let res_c = calc(in_c)?;
        let a = (*res_a).as_ref().map(get_gain).unwrap_or(0);
        let b = (*res_b).as_ref().map(get_gain).unwrap_or(0);
        let c = (*res_c).as_ref().map(get_gain).unwrap_or(0);

        if a == b && b == c {
            // 全て等しい
            if a == 0 {
                return Ok(None);
            }
        } else if a <= b && c <= b {
            // b が最大
            in_a = (in_a + in_b) / 2;
            in_c = (in_b + in_c) / 2;
        } else if b <= a && c <= a {
            // a が最大
            let step = (in_b - in_a) / 2;
            if min < in_a {
                in_b = in_a;
                in_c = in_a + step;
                in_a = min.max(in_a - step);
            } else {
                in_b = in_a + step;
                in_c = in_a + 2 * step;
            }
        } else {
            // c が最大
            let step = (in_c - in_b) / 2;
            if in_c < max {
                in_b = in_c;
                in_a = in_c - step;
                in_c = max.min(in_c + step);
            } else {
                in_b = in_c - step;
                in_a = in_c - 2 * step;
            }
        }
    }
    Ok(*calc(in_a)?.clone())
}
