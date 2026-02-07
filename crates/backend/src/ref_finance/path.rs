mod by_token;
mod edge;
pub mod graph;
pub mod preview;

use super::pool_info::TokenPath;
use crate::Result;
use crate::logging::*;
use crate::ref_finance::history;
use crate::ref_finance::pool_info::PoolInfoList;
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use crate::types::gas_price::GasPrice;
use crate::types::{MicroNear, MilliNear};
use graph::TokenGraph;
use num_integer::Roots;
use num_traits::{One, Zero, one, zero};
use preview::{Preview, PreviewList};
use slog::trace;
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::ops::{Add, Div, Mul, Sub};
use std::sync::Arc;

#[allow(dead_code)]
pub fn all_tokens(pools: Arc<PoolInfoList>) -> Vec<TokenAccount> {
    let by_tokens = by_token::PoolsByToken::new(pools);
    by_tokens.tokens()
}

#[allow(dead_code)]
pub async fn sorted_returns(
    graph: &TokenGraph,
    start: &TokenInAccount,
    initial: MilliNear,
) -> Result<Vec<(TokenOutAccount, MilliNear, usize)>> {
    let goals = graph.update_graph(start)?;
    let returns = graph.list_returns(initial.to_yocto(), start, &goals)?;
    let mut in_milli = vec![];
    for (k, v) in returns.iter() {
        let depth = graph.get_path_with_return(start, k)?.len();
        in_milli.push((k.clone(), MilliNear::from_yocto(*v), depth));
    }
    Ok(in_milli)
}

pub async fn swap_path(
    graph: &TokenGraph,
    start: &TokenInAccount,
    goal: &TokenOutAccount,
) -> Result<TokenPath> {
    graph.get_path_with_return(start, goal)
}

#[allow(dead_code)]
pub async fn pick_goals(
    graph: &TokenGraph,
    start: &TokenInAccount,
    total_amount: MilliNear,
    gas_price: GasPrice,
) -> Result<Option<Vec<Preview<u128>>>> {
    let previews = pick_previews(graph, start, MicroNear::from_milli(total_amount), gas_price)?;

    const REPEAT: usize = 3;

    let result = previews
        .filter(|previews| {
            let total_gain = previews.total_gain * REPEAT as u128;
            total_gain >= MIN_GAIN
        })
        .into_iter()
        .map(|previews| previews.convert(|m| m.to_yocto()).list)
        .next();

    Ok(result)
}

#[allow(dead_code)]
const MIN_GAIN: u128 = MilliNear::of(1).to_yocto();

fn rate_average<M: Into<u128>>(min: M, max: M) -> u128 {
    let min = min.into();
    let max = max.into();
    let s = (max / min).sqrt();
    s * min
}

pub fn pick_previews<M>(
    graph: &TokenGraph,
    start: &TokenInAccount,
    total_amount: M,
    gas_price: GasPrice,
) -> Result<Option<PreviewList<M>>>
where
    M: Send + Sync + Copy + Hash + Debug,
    M: Eq + Ord + Zero + One,
    M: Add<Output = M> + Sub<Output = M> + Mul<Output = M> + Div<Output = M>,
    M: From<u128> + Into<u128>,
{
    let log = DEFAULT.new(o!(
        "function" => "pick_previews",
        "start" => format!("{:?}", start),
        "total_amount" => format!("{:?}", total_amount),
        "gas_price" => format!("{:?}", gas_price),
    ));
    trace!(log, "start");

    let min_input = one();
    let ave_input = {
        let ave = history::get_history().read().unwrap().inputs.average();
        if ave.is_zero() {
            rate_average(min_input, total_amount).into()
        } else {
            ave.into()
        }
    };
    let goals = graph.update_graph(start)?;

    let do_pick = |value: M| {
        debug!(log, "do_pick";
            "value" => format!("{:?}", value)
        );
        if value.is_zero() {
            return Ok(None);
        }
        let limit = (total_amount.into() / value.into()) as usize;
        if limit > 0 {
            let previews = pick_by_amount(graph, start, &goals, gas_price, value, limit)?;
            return Ok(previews.map(Arc::new));
        }
        Ok(None)
    };

    let result = search_best_path(min_input, ave_input, total_amount, do_pick, |a| {
        a.total_gain
    })?;
    trace!(log, "finish");
    Ok(result.map(|a| Arc::into_inner(a).expect("should be unwrapped")))
}

fn pick_by_amount<M>(
    graph: &TokenGraph,
    start: &TokenInAccount,
    goals: &[TokenOutAccount],
    gas_price: GasPrice,
    amount: M,
    limit: usize,
) -> Result<Option<PreviewList<M>>>
where
    M: Copy + Debug + Into<u128>,
{
    let log = DEFAULT.new(o!(
        "function" => "pick_by_amount",
        "start" => format!("{:?}", start),
        "gas_price" => format!("{:?}", gas_price),
        "amount" => format!("{:?}", amount),
        "limit" => limit
    ));
    trace!(log, "start");

    let list = graph.list_returns(amount.into(), start, goals)?;
    let mut goals = vec![];
    for (goal, output) in list.into_iter().take(limit) {
        let path = graph.get_path_with_return(start, &goal)?;
        let preview = Preview::new(gas_price, amount, goal, path.len(), output);
        let gain = preview.gain;
        if gain > 0 {
            goals.push(preview);
        } else {
            break;
        }
    }
    Ok(PreviewList::new(amount, goals))
}

fn search_best_path<A, M, C, G>(
    min: M,
    average: M,
    max: M,
    calc_res: C,
    get_gain: G,
) -> Result<Option<Arc<A>>>
where
    A: Send + Sync,
    C: Send + Sync,
    G: Copy,
    C: Fn(M) -> Result<Option<Arc<A>>>,
    G: Fn(Arc<A>) -> u128,
    M: Send + Sync + Copy + Hash + Debug,
    M: Eq + Ord + Zero,
    M: Add<Output = M> + Sub<Output = M> + Mul<Output = M> + Div<Output = M>,
    M: From<u128>,
{
    let log = DEFAULT.new(o!(
        "function" => "search_best_path",
        "min" => format!("{:?}", min),
        "average" => format!("{:?}", average),
        "max" => format!("{:?}", max)
    ));
    trace!(log, "start");

    #[derive(Debug, Clone)]
    struct InnerError(Arc<anyhow::Error>);
    impl InnerError {
        fn unwrap(self) -> anyhow::Error {
            Arc::try_unwrap(self.0).unwrap()
        }
    }

    let gain = |a| get_gain(a).into();
    let mut cache: HashMap<M, std::result::Result<Option<Arc<A>>, InnerError>> = HashMap::new();

    // キャッシュを利用した単一引数の評価関数
    let mut evaluate = |input: M| -> std::result::Result<M, InnerError> {
        if cache.contains_key(&input) {
            return Ok(cache
                .get(&input)
                .unwrap()
                .clone()?
                .map(gain)
                .unwrap_or(zero()));
        }

        // 新しい値を計算
        let result = calc_res(input).map_err(|e| InnerError(Arc::new(e)))?;
        cache.insert(input, Ok(result.clone()));
        let gain_value = result.clone().map(gain).unwrap_or(zero());

        Ok(gain_value)
    };

    let m2 = 2.into();

    let mut in_a = min;
    let mut in_b = average;
    let mut in_c = max;
    while in_a < in_c {
        // 3点を評価
        let a = evaluate(in_a).map_err(|e| e.unwrap())?;
        let b = evaluate(in_b).map_err(|e| e.unwrap())?;
        let c = evaluate(in_c).map_err(|e| e.unwrap())?;

        debug!(log, "evaluated points";
            "in_a" => format!("{:?}", in_a),
            "in_b" => format!("{:?}", in_b),
            "in_c" => format!("{:?}", in_c),
            "a" => format!("{:?}", a),
            "b" => format!("{:?}", b),
            "c" => format!("{:?}", c)
        );

        if a == b && b == c && a == zero() {
            /* 全てゼロ
               a - b - c (== 0)
            */
            return Ok(None);
        } else if b <= a && c <= a {
            /* a が最大 or 全て等しくゼロより大きい
               a - b - c (> 0)

               a
                 \
                   b - c

               a - b
                     \
                       c

               a       c
                 \   /
                   b
            */
            let step = (in_b - in_a) / m2;
            if min < in_a {
                in_b = in_a;
                in_c = in_a + step;
                in_a = min.max(in_a - step);
            } else {
                in_b = in_a + step;
                in_c = in_a + m2 * step;
            }
        } else if a <= b && c <= b {
            /* b が最大
                   b
                 /   \
               a       c

                   b - c
                 /
               a
            */
            in_a = {
                let step = (in_b - in_a) / m2;
                in_b - step
            };
            in_c = {
                let step = (in_c - in_b) / m2;
                in_b + step
            };
        } else {
            /* c が最大
                       c
                     /
               a - b
            */
            let step = (in_c - in_b) / m2;
            if in_c < max {
                in_b = in_c;
                in_a = in_c - step;
                in_c = max.min(in_c + step);
            } else {
                in_b = in_c - step;
                in_a = in_c - m2 * step;
            }
        }
    }

    trace!(log, "finish";
        "a" => format!("{:?}", in_a),
        "b" => format!("{:?}", in_b),
        "c" => format!("{:?}", in_c)
    );

    cache
        .get(&in_a)
        .cloned()
        .unwrap_or(Ok(None))
        .map_err(|ie| ie.unwrap())
}

#[cfg(test)]
mod tests;
