use crate::jsonrpc::ViewContract;
use crate::ref_finance::CONTRACT_ADDRESS;
use anyhow::{Context, Result, bail};
use futures_util::future::join_all;
use logging::*;
use serde_json::{from_slice, json};
use std::sync::Arc;

use dex::pool_info::{PoolInfo, PoolInfoBared, PoolInfoList};

pub async fn read_pools_from_node<C: ViewContract>(client: &C) -> Result<Arc<PoolInfoList>> {
    let log = DEFAULT.new(o!("function" => "read_pools_from_node"));
    info!(log, "start");

    let number_of_pools: u32 = {
        let args = json!({});
        let res = client
            .view_contract(&CONTRACT_ADDRESS, "get_number_of_pools", &args)
            .await?;
        let raw = res.result;
        from_slice(&raw).context(format!(
            "failed to parse count of pools: {:?}",
            String::from_utf8(raw)
        ))?
    };
    info!(log, "number_of_pools"; "value" => number_of_pools);

    const METHOD_NAME: &str = "get_pools";
    const LIMIT: usize = 2 << 8;

    let results: Vec<_> = (0..number_of_pools)
        .step_by(LIMIT)
        .map(|index| async move {
            let log = DEFAULT.new(o!(
                "function" => "get_pools",
                "index" => index,
                "limit" => LIMIT,
            ));
            let args = json!({
                "from_index": index,
                "limit": LIMIT,
            });
            debug!(log, "requesting");
            let res = client
                .view_contract(&CONTRACT_ADDRESS, METHOD_NAME, &args)
                .await;
            let result: Result<Vec<PoolInfoBared>> = match res {
                Ok(v) => from_slice(&v.result).context("failed to parse"),
                Err(e) => bail!("failed to request: {:?}", e),
            };
            let count = result.as_ref().map(|v| v.len()).unwrap_or(0);
            debug!(log, "result"; "count" => count);
            result.map(move |list| {
                list.into_iter().enumerate().map(move |(i, bare)| {
                    let timestamp = chrono::Utc::now().naive_utc();
                    Arc::new(PoolInfo::new(i as u32 + index, bare, timestamp))
                })
            })
        })
        .collect();
    let lists = join_all(results).await;
    let oks: Result<Vec<_>> = lists.into_iter().collect();
    let pools: Vec<_> = oks?.into_iter().flatten().collect();

    info!(log, "finish"; "count" => pools.len());
    Ok(Arc::new(PoolInfoList::new(pools)))
}
