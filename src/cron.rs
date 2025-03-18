use crate::persistence::token_rate::TokenRate;
use crate::Result;
use crate::config;
use crate::jsonrpc;
use crate::logging::*;
use crate::ref_finance;
use crate::ref_finance::token_account::TokenInAccount;
use crate::ref_finance::token_account::WNEAR_TOKEN;
use crate::types::MilliNear;
use bigdecimal::BigDecimal;
use chrono::Utc as TZ;

const CRON: &str = "0 0 * * * *";

pub async fn run() {
    let schedule: cron::Schedule = CRON.parse().unwrap();
    for next in schedule.upcoming(TZ) {
        if let Ok(wait) = (next - TZ::now()).to_std() {
            tokio::time::sleep(wait).await;
            job().await;
        }
    }
}

async fn job() {
    let log = DEFAULT.new(o!("function" => "job"));

    info!(log, "CRON");
    match record_rates().await {
        Ok(_) => info!(log, "success"),
        Err(err) => error!(log, "failure"; "error" => ?err),
    }
}

fn get_quote_token() -> TokenInAccount {
    WNEAR_TOKEN.clone().into()
}

fn get_initial_value() -> u128 {
    let in_milli =config::get("CRON_RECORD_RATES_INITIAL_VALUE")
        .and_then(|v| Ok(v.parse()?))
        .unwrap_or(100);
    MilliNear::of(in_milli).to_yocto()
}

async fn record_rates() -> Result<()> {
    let log = DEFAULT.new(o!("function" => "record_rates"));

    let quote_token = &get_quote_token();
    let initial_value = get_initial_value();

    info!(log, "recording rates";
        "quote_token" => %quote_token,
        "initial_value" => %initial_value,
    );

    let client = &jsonrpc::new_client();

    let pools = ref_finance::pool_info::PoolInfoList::read_from_node(client).await?;
    let graph = ref_finance::path::graph::TokenGraph::new(pools);
    let goals = graph.update_graph(quote_token)?;
    let values = graph.list_values(initial_value, quote_token, &goals)?;

    let rates: Vec<_> = values.into_iter().map(|(base, value)| {
        let rate = BigDecimal::from(value) / BigDecimal::from(initial_value);
        TokenRate::new(base, quote_token.clone(), rate)
    }).collect();

    TokenRate::batch_insert(&rates).await?;

    Ok(())
}
