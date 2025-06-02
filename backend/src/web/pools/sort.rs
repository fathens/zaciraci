use crate::Result;
use crate::ref_finance::path::graph::TokenGraph;
use crate::ref_finance::pool_info::{PoolInfo, PoolInfoList, TokenPairLike};
use crate::ref_finance::token_account::{
    TokenAccount, TokenInAccount, TokenOutAccount, WNEAR_TOKEN,
};
use bigdecimal::BigDecimal;
use near_sdk::NearToken;
use num_traits::{ToPrimitive, zero};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;

const ONE_NEAR: u128 = NearToken::from_near(1).as_yoctonear();

#[derive(Debug)]
pub(super) struct WithWeight<T> {
    pub value: T,
    pub weight: f64,
}

impl<T> PartialOrd for WithWeight<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for WithWeight<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.weight
            .partial_cmp(&other.weight)
            .unwrap_or(Ordering::Equal)
    }
}

impl<T> PartialEq for WithWeight<T> {
    fn eq(&self, other: &Self) -> bool {
        self.weight == other.weight
    }
}

impl<T> Eq for WithWeight<T> {}

fn make_rates(
    quote: (&TokenInAccount, u128),
    graph: &TokenGraph,
    outs: &[TokenOutAccount],
) -> Result<HashMap<TokenAccount, BigDecimal>> {
    let values = graph.list_values(quote.1, quote.0, outs)?;
    let rates = values
        .into_iter()
        .filter_map(|(out, value)| {
            if value == 0 {
                return None;
            } else {
                let base = BigDecimal::from(value);
                let quote = BigDecimal::from(quote.1);
                Some((out.into(), quote / base))
            }
        })
        .collect();
    Ok(rates)
}

fn average_depth(rates: &HashMap<TokenAccount, BigDecimal>, pool: &Arc<PoolInfo>) -> BigDecimal {
    let mut sum: BigDecimal = zero();
    let mut count: BigDecimal = zero();
    for (index, token) in pool.tokens().enumerate() {
        count += 1;
        if let Some(rate) = rates.get(token) {
            if let Ok(amount) = pool.amount(index.into()) {
                let value = BigDecimal::from(amount) * rate;
                sum += value;
            }
        }
    }
    sum / count
}

pub fn sort(pools: Arc<PoolInfoList>) -> Result<Vec<Arc<PoolInfo>>> {
    let quote = WNEAR_TOKEN.clone().into();
    let graph = TokenGraph::new(Arc::clone(&pools));
    let outs = graph.update_graph(&quote)?;
    let rates = make_rates((&quote, ONE_NEAR), &graph, &outs)?;
    let mut ww: Vec<_> = pools
        .iter()
        .map(|src| {
            let weight = average_depth(&rates, src);
            WithWeight {
                value: Arc::clone(src),
                weight: weight.to_f64().unwrap(),
            }
        })
        .collect();
    ww.sort();
    let sorted = ww.iter().rev().map(|w| Arc::clone(&w.value)).collect();
    Ok(sorted)
}

#[allow(dead_code)]
pub fn tokens_with_depth(pools: Arc<PoolInfoList>) -> Result<HashMap<TokenAccount, f64>> {
    let quote = WNEAR_TOKEN.clone().into();
    let graph = TokenGraph::new(Arc::clone(&pools));
    let outs = graph.update_graph(&quote)?;
    let rates = make_rates((&quote, ONE_NEAR), &graph, &outs)?;
    let pools_depth: HashMap<_, _> = pools
        .iter()
        .map(|pool| {
            let depth = average_depth(&rates, pool);
            (pool.id, depth)
        })
        .collect();

    let result = HashMap::new();
    for out in &outs {
        let path = graph.get_path(&quote, out)?;
        let _depths = path
            .0
            .iter()
            .filter_map(|pair| pools_depth.get(&pair.pool_id()))
            .min();
    }

    Ok(result)
}

#[cfg(test)]
mod tests;
