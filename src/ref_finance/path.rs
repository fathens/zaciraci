#![allow(dead_code)]

use crate::ref_finance::history;
use crate::ref_finance::pool_info::{PoolInfoList, TokenPair};
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use crate::Result;
use moka::future::Cache;
use std::sync::Arc;

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

#[derive(Debug, Eq, PartialEq, Hash)]
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
    fn new(input_value: u128, previews: Vec<Preview>) -> Self {
        if previews.is_empty() {
            return PreviewList {
                input_value,
                list: previews,
                total_gain: 0,
            };
        }
        let gains: u128 = previews.iter().map(|p| p.gain()).sum();
        let total_gain = gains - MIN_GAIN;
        PreviewList {
            input_value,
            list: previews,
            total_gain,
        }
    }

    fn get_list(&self) -> Vec<Preview> {
        self.list.clone()
    }
}

const MIN_GAIN: u128 = 1_000_000_000_000_000_000_000_000;

pub async fn pick_pools(start: TokenInAccount, total_amount: u128) -> Result<Option<Vec<Preview>>> {
    let all_pools = PoolInfoList::read_from_node().await?;
    let stats_ave = history::get_history().read().unwrap().inputs.average();

    let do_pick = |value: u128| -> Result<Option<Arc<PreviewList>>> {
        let limit = (total_amount / value) as usize;
        if limit > 0 {
            let graph = TokenGraph::new(&all_pools, value);
            let previews = pick_by_amount(&graph, &start, value, limit)?;
            if previews.total_gain > 0 {
                return Ok(Some(Arc::new(previews)));
            }
        }
        Ok(None)
    };

    let result = search_best_path(1, stats_ave, total_amount, do_pick, |a| a.total_gain).await?;
    Ok(result.map(|a| a.get_list()))
}

fn pick_by_amount(
    graph: &TokenGraph,
    start: &TokenInAccount,
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
    Ok(PreviewList::new(amount, goals))
}

async fn search_best_path<A, C, G>(
    min: u128,
    average: u128,
    max: u128,
    calc_res: C,
    get_gain: G,
) -> Result<Option<Arc<A>>>
where
    A: Send + Sync + 'static,
    C: Send + Sync + Copy,
    G: Copy,
    C: Fn(u128) -> Result<Option<Arc<A>>>,
    G: Fn(Arc<A>) -> u128,
{
    let cache = Cache::new(1 << 16);
    let calc = |value| {
        let cache = cache.clone();
        async move { cache.get_with(value, async { calc_res(value) }).await }
    };

    let mut in_a = min;
    let mut in_b = average;
    let mut in_c = max;
    while in_a < in_c {
        let (res_a, res_b, res_c) =
            futures_util::future::join3(calc(in_a), calc(in_b), calc(in_c)).await;
        let a = res_a?.map(get_gain).unwrap_or(0);
        let b = res_b?.map(get_gain).unwrap_or(0);
        let c = res_c?.map(get_gain).unwrap_or(0);

        if a == b && b == c {
            // 全て等しい
            if a == 0 {
                return Ok(None);
            }
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
        } else if a <= c && b <= c {
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
        } else if a <= b && c <= b {
            // b が最大
            in_a = (in_a + in_b) / 2;
            in_c = (in_b + in_c) / 2;
        }
    }
    cache.get(&in_a).await.unwrap_or(Ok(None))
}

#[cfg(test)]
mod test {
    use super::*;

    struct TestCalc {
        sorted_points: Vec<(u128, u128)>,
        input_value: u128,
    }

    impl TestCalc {
        fn maker(points: &[(u128, u128)]) -> impl Fn(u128) -> Self {
            if points.len() < 2 {
                panic!("points must be more than 2");
            }
            let mut sorted_points = points.to_vec();
            sorted_points.sort_by_key(|(a, _)| *a);
            move |input_value| TestCalc {
                sorted_points: sorted_points.clone(),
                input_value,
            }
        }

        fn calc_gain(&self) -> u128 {
            let pos = self
                .sorted_points
                .binary_search_by_key(&self.input_value, |(a, _)| *a);
            match pos {
                Ok(pos) => self.sorted_points[pos].1,
                Err(pos) => {
                    if 0 < pos && pos < self.sorted_points.len() {
                        let a = self.sorted_points[pos - 1].1;
                        let b = self.sorted_points[pos].1;
                        (a + b) / 2
                    } else {
                        0
                    }
                }
            }
        }
    }

    #[test]
    fn test_test_calc() {
        {
            let maker = TestCalc::maker(&[(1, 1), (2, 2), (3, 3)]);
            let calc1 = maker(1);
            let calc2 = maker(2);
            let calc3 = maker(3);
            assert_eq!(calc1.calc_gain(), 1);
            assert_eq!(calc2.calc_gain(), 2);
            assert_eq!(calc3.calc_gain(), 3);
        }
        {
            let maker = TestCalc::maker(&[(1, 1), (3, 3)]);
            let calc1 = maker(1);
            let calc2 = maker(2);
            let calc3 = maker(3);
            assert_eq!(calc1.calc_gain(), 1);
            assert_eq!(calc2.calc_gain(), 2);
            assert_eq!(calc3.calc_gain(), 3);
        }
        {
            let maker = TestCalc::maker(&[(1, 1), (2, 2)]);
            let calc1 = maker(1);
            let calc2 = maker(2);
            let calc3 = maker(3);
            assert_eq!(calc1.calc_gain(), 1);
            assert_eq!(calc2.calc_gain(), 2);
            assert_eq!(calc3.calc_gain(), 0);
        }
        {
            let maker = TestCalc::maker(&[(10, 20), (30, 50)]);
            let calc1 = maker(1);
            let calc9 = maker(9);
            let calc20 = maker(20);
            assert_eq!(calc1.calc_gain(), 0);
            assert_eq!(calc9.calc_gain(), 0);
            assert_eq!(calc20.calc_gain(), 35);
        }
        {
            let maker = TestCalc::maker(&[(20, 20), (40, 40), (50, 30), (70, 50)]);
            let calc10 = maker(10);
            let calc30 = maker(30);
            let calc45 = maker(45);
            let calc60 = maker(60);
            assert_eq!(calc10.calc_gain(), 0);
            assert_eq!(calc30.calc_gain(), 30);
            assert_eq!(calc45.calc_gain(), 35);
            assert_eq!(calc60.calc_gain(), 40);
        }
    }

    #[tokio::test]
    async fn test_search_best_path() {
        {
            let maker = TestCalc::maker(&[(1, 1), (2, 2), (3, 3)]);
            let calc = |value| {
                let calc = maker(value);
                Ok(Some(Arc::new(calc)))
            };
            let get_gain = |a: Arc<TestCalc>| a.calc_gain();
            let result = search_best_path(1, 2, 3, calc, get_gain).await.unwrap();
            assert_eq!(result.map(|a| a.calc_gain()), Some(3));
        }
        {
            let maker = TestCalc::maker(&[(1, 1), (3, 3)]);
            let calc = |value| {
                let calc = maker(value);
                Ok(Some(Arc::new(calc)))
            };
            let get_gain = |a: Arc<TestCalc>| a.calc_gain();
            let result = search_best_path(1, 2, 3, calc, get_gain).await.unwrap();
            assert_eq!(result.map(|a| a.calc_gain()), Some(3));
        }
        {
            let maker = TestCalc::maker(&[(1, 1), (2, 2)]);
            let calc = |value| {
                let calc = maker(value);
                Ok(Some(Arc::new(calc)))
            };
            let get_gain = |a: Arc<TestCalc>| a.calc_gain();
            let f = search_best_path(1, 2, 3, calc, get_gain);
            let w = f.await;
            let result = w.unwrap();
            assert_eq!(result.map(|a| a.calc_gain()), Some(2));
        }
        {
            let maker = TestCalc::maker(&[(10, 20), (30, 50)]);
            let calc = |value| {
                let calc = maker(value);
                Ok(Some(Arc::new(calc)))
            };
            let get_gain = |a: Arc<TestCalc>| a.calc_gain();
            let result = search_best_path(1, 2, 30, calc, get_gain).await.unwrap();
            assert_eq!(result.map(|a| a.calc_gain()), Some(50));
        }
        {
            let maker = TestCalc::maker(&[(20, 20), (40, 40), (50, 30), (70, 50)]);
            let calc = |value| {
                let calc = maker(value);
                Ok(Some(Arc::new(calc)))
            };
            let get_gain = |a: Arc<TestCalc>| a.calc_gain();
            let result = search_best_path(1, 30, 70, calc, get_gain).await.unwrap();
            assert_eq!(result.map(|a| a.calc_gain()), Some(50));
        }
    }
}
