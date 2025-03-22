use crate::Result;
use crate::logging::*;
use crate::jsonrpc;
use crate::ref_finance;

use super::{get_initial_value, get_quote_token};

pub async fn start() -> Result<()> {
    let log = DEFAULT.new(o!("function" => "trade::start"));

    let quote_token = &get_quote_token();
    let initial_value = get_initial_value();

    let client = &jsonrpc::new_client();

    info!(log, "loading pools");
    let pools = ref_finance::pool_info::PoolInfoList::read_from_node(client).await?;
    let graph = ref_finance::path::graph::TokenGraph::new(pools);
    let goals = graph.update_graph(quote_token)?;
    info!(log, "found targets"; "goals" => %goals.len());
    let values = graph.list_values(initial_value, quote_token, &goals)?;

    info!(log, "values found"; "values" => %values.len());

    info!(log, "success");
    Ok(())
}
