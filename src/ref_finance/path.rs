use crate::logging::*;
use crate::ref_finance::history;
use crate::ref_finance::pool_info::{PoolInfoList, TokenPair};
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use crate::types::{MicroNear, MilliNear};
use crate::Result;
use graph::TokenGraph;
use near_primitives::types::Balance;
use num_integer::Roots;
use num_traits::{one, zero, One, Zero};
use slog::info;
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::ops::{Add, Div, Mul, Sub};
use std::sync::Arc;

mod by_token;
mod cached_evaluate;
mod edge;
pub mod graph;
pub mod preview;

use crate::types::gas_price::GasPrice;
use preview::{Preview, PreviewList};

pub fn all_tokens(pools: Arc<PoolInfoList>) -> Vec<TokenAccount> {
    let by_tokens = by_token::PoolsByToken::new(pools);
    by_tokens.tokens()
}

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
) -> Result<Vec<TokenPair>> {
    graph.get_path_with_return(start, goal)
}

pub async fn pick_goals(
    graph: &TokenGraph,
    start: &TokenInAccount,
    total_amount: MilliNear,
    gas_price: GasPrice,
) -> Result<Option<Vec<Preview<Balance>>>> {
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
    info!(log, "start");

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
    info!(log, "finish");
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
    info!(log, "start");

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
    info!(log, "start");

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
            return Ok(cache.get(&input).unwrap().clone()?.map(gain).unwrap_or(zero()));
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

    info!(log, "finish";
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
                        let p0 = self.sorted_points[pos - 1];
                        let p1 = self.sorted_points[pos];
                        let (x0, y0) = (p0.0 as i128, p0.1 as i128);
                        let (x1, y1) = (p1.0 as i128, p1.1 as i128);
                        let x = self.input_value as i128;
                        let y = (x - x0) * (y1 - y0) / (x1 - x0) + y0;
                        y as u128
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
            let calc55 = maker(55);
            let calc60 = maker(60);
            assert_eq!(calc10.calc_gain(), 0);
            assert_eq!(calc30.calc_gain(), 30);
            assert_eq!(calc45.calc_gain(), 35);
            assert_eq!(calc55.calc_gain(), 35);
            assert_eq!(calc60.calc_gain(), 40);
        }
    }

    #[test]
    fn test_search_best_path() {
        let result_pair = |a: Arc<TestCalc>| (a.input_value, a.calc_gain());

        {
            let maker = TestCalc::maker(&[(1, 1), (2, 2), (3, 3)]);
            let calc = |value| {
                let calc = maker(value);
                Ok(Some(Arc::new(calc)))
            };
            let get_gain = |a: Arc<TestCalc>| a.calc_gain();
            let result = search_best_path(1, 2, 3, calc, get_gain).unwrap();
            assert_eq!(result.map(result_pair), Some((3, 3)));
        }
        {
            let maker = TestCalc::maker(&[(1, 1), (3, 3)]);
            let calc = |value| {
                let calc = maker(value);
                Ok(Some(Arc::new(calc)))
            };
            let get_gain = |a: Arc<TestCalc>| a.calc_gain();
            let result = search_best_path(1, 2, 3, calc, get_gain).unwrap();
            assert_eq!(result.map(result_pair), Some((3, 3)));
        }
        {
            let maker = TestCalc::maker(&[(1, 1), (2, 2)]);
            let calc = |value| {
                let calc = maker(value);
                Ok(Some(Arc::new(calc)))
            };
            let get_gain = |a: Arc<TestCalc>| a.calc_gain();
            let result = search_best_path(1, 2, 3, calc, get_gain).unwrap();
            assert_eq!(result.map(result_pair), Some((2, 2)));
        }
        {
            let maker = TestCalc::maker(&[(10, 20), (30, 50)]);
            let calc = |value| {
                let calc = maker(value);
                Ok(Some(Arc::new(calc)))
            };
            let get_gain = |a: Arc<TestCalc>| a.calc_gain();
            let result = search_best_path(1, 2, 30, calc, get_gain).unwrap();
            assert_eq!(result.map(result_pair), Some((30, 50)));
        }
        {
            let maker = TestCalc::maker(&[(20, 20), (40, 40), (50, 30), (70, 50)]);
            let calc = |value| {
                let calc = maker(value);
                Ok(Some(Arc::new(calc)))
            };
            let get_gain = |a: Arc<TestCalc>| a.calc_gain();
            let result = search_best_path(1, 30, 100, calc, get_gain).unwrap();
            assert_eq!(result.map(result_pair), Some((70, 50)));
        }
        {
            let maker = TestCalc::maker(&[(20, 0), (70, 0)]);
            let calc = |value| {
                let calc = maker(value);
                Ok(Some(Arc::new(calc)))
            };
            let get_gain = |a: Arc<TestCalc>| a.calc_gain();
            let result = search_best_path(1, 30, 100, calc, get_gain).unwrap();
            assert_eq!(result.map(result_pair), None);
        }
        {
            let maker = TestCalc::maker(&[(1, 10), (100, 10)]);
            let calc = |value| {
                let calc = maker(value);
                Ok(Some(Arc::new(calc)))
            };
            let get_gain = |a: Arc<TestCalc>| a.calc_gain();
            let result = search_best_path(1, 30, 100, calc, get_gain).unwrap();
            assert_eq!(result.map(result_pair), Some((1, 10)));
        }
        {
            let maker = TestCalc::maker(&[(1, 10), (70, 20), (100, 10)]);
            let calc = |value| {
                let calc = maker(value);
                Ok(Some(Arc::new(calc)))
            };
            let get_gain = |a: Arc<TestCalc>| a.calc_gain();
            let result = search_best_path(1, 40, 100, calc, get_gain).unwrap();
            assert_eq!(result.map(result_pair), Some((70, 20)));
        }
        {
            let maker = TestCalc::maker(&[(30, 20), (50, 10), (70, 10)]);
            let calc = |value| {
                let calc = maker(value);
                Ok(Some(Arc::new(calc)))
            };
            let get_gain = |a: Arc<TestCalc>| a.calc_gain();
            let result = search_best_path(1, 40, 100, calc, get_gain).unwrap();
            assert_eq!(result.map(result_pair), Some((30, 20)));
        }
        {
            let maker = TestCalc::maker(&[(30, 20), (50, 20), (70, 10)]);
            let calc = |value| {
                let calc = maker(value);
                Ok(Some(Arc::new(calc)))
            };
            let get_gain = |a: Arc<TestCalc>| a.calc_gain();
            let result = search_best_path(1, 40, 100, calc, get_gain).unwrap();
            assert_eq!(result.map(result_pair), Some((30, 20)));
        }
        {
            let maker = TestCalc::maker(&[(30, 10), (50, 20), (70, 10)]);
            let calc = |value| {
                let calc = maker(value);
                Ok(Some(Arc::new(calc)))
            };
            let get_gain = |a: Arc<TestCalc>| a.calc_gain();
            let result = search_best_path(1, 40, 100, calc, get_gain).unwrap();
            assert_eq!(result.map(result_pair), Some((50, 20)));
        }
        {
            let maker = TestCalc::maker(&[(30, 10), (50, 20), (70, 20)]);
            let calc = |value| {
                let calc = maker(value);
                Ok(Some(Arc::new(calc)))
            };
            let get_gain = |a: Arc<TestCalc>| a.calc_gain();
            let result = search_best_path(1, 40, 100, calc, get_gain).unwrap();
            assert_eq!(result.map(result_pair), Some((51, 20)));
        }
        {
            let maker = TestCalc::maker(&[(30, 10), (50, 10), (70, 20)]);
            let calc = |value| {
                let calc = maker(value);
                Ok(Some(Arc::new(calc)))
            };
            let get_gain = |a: Arc<TestCalc>| a.calc_gain();
            let result = search_best_path(1, 40, 100, calc, get_gain).unwrap();
            assert_eq!(result.map(result_pair), Some((70, 20)));
        }
        {
            let maker = TestCalc::maker(&[(30, 20), (50, 20), (70, 20)]);
            let calc = |value| {
                let calc = maker(value);
                Ok(Some(Arc::new(calc)))
            };
            let get_gain = |a: Arc<TestCalc>| a.calc_gain();
            let result = search_best_path(1, 40, 100, calc, get_gain).unwrap();
            assert_eq!(result.map(result_pair), Some((30, 20)));
        }
    }

    #[test]
    fn test_rate_averate() {
        assert_eq!(rate_average(1_u128, 1), 1);
        assert_eq!(rate_average(1_u128, 100), 10);
        assert_eq!(rate_average(10_u128, 1000), 100);
        assert_eq!(rate_average(10_u128, 100000), 1000);
    }

    mod test_static {
        use crate::Result;
        use async_once_cell::OnceCell;
        use std::ops::Deref;
        use std::sync::{LazyLock, Mutex};
        use tokio::time::sleep;

        static CACHED_STRING: OnceCell<String> = OnceCell::new();
        async fn get_cached_string() -> Result<&'static String> {
            CACHED_STRING.get_or_try_init(mk_string()).await
        }

        static LIST: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(vec![]));

        fn push_log(s: &str) {
            let mut list = LIST.lock().unwrap();
            list.push(s.to_string());
        }

        async fn mk_string() -> Result<String> {
            push_log("start mk_string");
            sleep(tokio::time::Duration::from_secs(1)).await;
            push_log("end mk_string");
            Ok("test".to_string())
        }

        #[tokio::test]
        async fn test_once_cell() {
            push_log("start 0");
            let r1 = get_cached_string().await.unwrap();
            push_log("end 0");

            push_log("start 1");
            let r2 = get_cached_string().await.unwrap();
            push_log("end 1");

            assert_eq!(r1, r2);
            let guard = LIST.lock().unwrap();
            let list = guard.deref();
            assert_eq!(
                list,
                &[
                    "start 0",
                    "start mk_string",
                    "end mk_string",
                    "end 0",
                    "start 1",
                    "end 1",
                ]
            );
        }
    }
}
