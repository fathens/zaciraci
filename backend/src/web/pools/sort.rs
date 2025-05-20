use crate::Result;
use crate::ref_finance::path::graph::TokenGraph;
use crate::ref_finance::pool_info::{PoolInfo, PoolInfoList};
use crate::ref_finance::token_account::{TokenAccount, WNEAR_TOKEN};
use near_sdk::NearToken;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;

#[allow(dead_code)]
pub(super) struct WithWight<T> {
    pub value: T,
    pub weight: f64,
}

impl<T> PartialOrd for WithWight<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for WithWight<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.weight.partial_cmp(&other.weight).unwrap_or(Ordering::Equal)
    }
}

impl<T> PartialEq for WithWight<T> {
    fn eq(&self, other: &Self) -> bool {
        self.weight == other.weight
    }
}

impl<T> Eq for WithWight<T> {}

fn make_rates(pools: Arc<PoolInfoList>) -> Result<HashMap<TokenAccount, u128>> {
    const AMOUNT_IN: u128 = NearToken::from_near(1).as_yoctonear();
    let graph = TokenGraph::new(pools);
    let outs = graph.update_graph(&WNEAR_TOKEN.clone().into())?;
    let returns = graph.list_returns(AMOUNT_IN, &WNEAR_TOKEN.clone().into(), &outs)?;
    Ok(returns
        .into_iter()
        .map(|(out, value)| (out.into(), value))
        .collect())
}

fn amount_value(rates: HashMap<TokenAccount, u128>, pool: &Arc<PoolInfo>) -> f64 {
    let mut sum = 0_f64;
    let mut count = 0;
    for token in pool.tokens() {
        count += 1;
        if let Some(&value) = rates.get(token) {
            sum += value as f64;
        }
    }
    sum / count as f64
}

pub fn sort(pools: Arc<PoolInfoList>) -> Result<Vec<Arc<PoolInfo>>> {
    let rates = make_rates(Arc::clone(&pools))?;
    let mut ww: Vec<_> = pools
        .iter()
        .map(|src| {
            let weight = amount_value(rates.clone(), src);
            WithWight {
                value: Arc::clone(src),
                weight,
            }
        })
        .collect();
    ww.sort();
    let sorted = ww.iter().rev().map(|w| Arc::clone(&w.value)).collect();
    Ok(sorted)
}
