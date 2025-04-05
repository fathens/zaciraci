use super::AppState;
use crate::jsonrpc::{GasInfo, SentTx};
use crate::ref_finance::pool_info;
use crate::ref_finance::pool_info::TokenPairLike;
use crate::ref_finance::token_account::TokenAccount;
use crate::types::{MicroNear, MilliNear};
use crate::{jsonrpc, ref_finance, wallet};
use axum::Json;
use axum::{
    Router,
    extract::{Path, State},
    routing::{get, post},
};
use num_rational::Ratio;
use num_traits::ToPrimitive;
use std::collections::HashMap;
use std::sync::Arc;
use zaciraci_common::pools::{TradeRequest, TradeResponse};
use zaciraci_common::types::YoctoNearToken;

fn path(sub: &str) -> String {
    format!("/pools/{sub}")
}

pub fn add_route(app: Router<Arc<AppState>>) -> Router<Arc<AppState>> {
    app.route(&path("get_all"), get(get_all_pools))
        .route(
            &path("estimate_return/{pool_id}/{amount}"),
            get(estimate_return),
        )
        .route(&path("get_return/{pool_id}/{amount}"), get(get_return))
        .route(&path("list_all_tokens"), get(list_all_tokens))
        .route(
            &path("list_returns/{token_account}/{amount}"),
            get(list_returns),
        )
        .route(
            &path("pick_goals/{token_account}/{amount}"),
            get(pick_goals),
        )
        .route(
            &path("run_swap/{token_in_account}/{initial_value}/{token_out_account}"),
            get(run_swap),
        )
        .route(&path("estimate_trade"), post(estimate_trade))
}

async fn get_all_pools(State(_): State<Arc<AppState>>) -> String {
    let pools = pool_info::PoolInfoList::read_from_db(None).await.unwrap();
    format!("Pools: {}", pools.len())
}

async fn estimate_return(
    State(_): State<Arc<AppState>>,
    Path((pool_id, amount)): Path<(u32, u128)>,
) -> String {
    use crate::ref_finance::errors::Error;

    let client = jsonrpc::new_client();
    let pools = pool_info::PoolInfoList::read_from_node(&client)
        .await
        .unwrap();
    let pool = pools.get(pool_id).unwrap();
    let n = pool.len();
    assert!(n > 1, "{}", Error::InvalidPoolSize(n));
    let token_in = 0;
    let token_out = n - 1;
    let amount_in = amount;
    let pair = pool.get_pair(token_in.into(), token_out.into()).unwrap();
    let amount_out = pair.estimate_return(amount_in).unwrap();
    let token_a = pair.token_in_id();
    let token_b = pair.token_out_id();
    format!("Estimated: {token_a}({amount_in}) -> {token_b}({amount_out})")
}

async fn get_return(
    State(_): State<Arc<AppState>>,
    Path((pool_id, amount)): Path<(u32, u128)>,
) -> String {
    use crate::ref_finance::errors::Error;

    let client = jsonrpc::new_client();
    let pools = pool_info::PoolInfoList::read_from_db(None).await.unwrap();
    let pool = pools.get(pool_id).unwrap();
    let n = pool.len();
    assert!(n > 1, "{}", Error::InvalidPoolSize(n));
    let token_in = 0;
    let token_out = n - 1;
    let amount_in = amount;
    let pair = pool.get_pair(token_in.into(), token_out.into()).unwrap();
    let token_a = pair.token_in_id();
    let token_b = pair.token_out_id();
    let amount_out = pair.get_return(&client, amount_in).await.unwrap();
    format!("Return: {token_a}({amount_in}) -> {token_b}({amount_out})")
}

async fn list_all_tokens(State(_): State<Arc<AppState>>) -> String {
    let pools = pool_info::PoolInfoList::read_from_db(None).await.unwrap();
    let tokens = ref_finance::path::all_tokens(pools);
    let mut tokens: Vec<_> = tokens.iter().map(|t| t.to_string()).collect();
    tokens.sort();
    let mut result = String::from("Tokens:\n");
    for token in tokens {
        result.push_str(&format!("{token}\n"));
    }
    result
}

async fn list_returns(
    State(_): State<Arc<AppState>>,
    Path((token_account, initial_value)): Path<(String, String)>,
) -> String {
    let pools = pool_info::PoolInfoList::read_from_db(None).await.unwrap();
    let graph = ref_finance::path::graph::TokenGraph::new(pools);
    let amount_in = MilliNear::of(initial_value.replace("_", "").parse().unwrap());
    let start: TokenAccount = token_account.parse().unwrap();
    let mut sorted_returns = ref_finance::path::sorted_returns(&graph, &start.into(), amount_in)
        .await
        .unwrap();
    sorted_returns.reverse();

    let mut result = String::from("from: {token_account}\n");
    for (goal, value, depth) in sorted_returns {
        let rational = Ratio::new(value.to_yocto(), amount_in.to_yocto());
        let ret = rational.to_f32().unwrap();
        result.push_str(&format!("{goal}: {ret}({depth})\n"));
    }
    result
}

async fn pick_goals(
    State(_): State<Arc<AppState>>,
    Path((token_account, initial_value)): Path<(String, String)>,
) -> String {
    let gas_price = jsonrpc::new_client().get_gas_price(None).await.unwrap();
    let pools = pool_info::PoolInfoList::read_from_db(None).await.unwrap();
    let graph = ref_finance::path::graph::TokenGraph::new(pools);
    let amount_in: u32 = initial_value.replace("_", "").parse().unwrap();
    let start: TokenAccount = token_account.parse().unwrap();
    let goals =
        ref_finance::path::pick_goals(&graph, &start.into(), MilliNear::of(amount_in), gas_price)
            .await
            .unwrap();
    let mut result = String::from(&format!("from: {token_account}({amount_in})\n"));
    match goals {
        None => {
            result.push_str("No goals found\n");
        }
        Some(previews) => {
            for preview in previews {
                let input_value = MicroNear::from_yocto(preview.input_value);
                let token_name = preview.token.to_string();
                let gain = MicroNear::from_yocto(preview.output_value - input_value.to_yocto());
                result.push_str(&format!("{input_value:?} -> {token_name} -> {gain:?}\n"));
            }
        }
    }
    result
}

async fn run_swap(
    State(_): State<Arc<AppState>>,
    Path((token_in_account, initial_value, token_out_account)): Path<(String, String, String)>,
) -> String {
    let client = jsonrpc::new_client();
    let wallet = wallet::new_wallet();
    let pools = pool_info::PoolInfoList::read_from_node(&client)
        .await
        .unwrap();
    let graph = ref_finance::path::graph::TokenGraph::new(pools);
    let amount_in: u128 = initial_value.replace("_", "").parse().unwrap();
    let start_token: TokenAccount = token_in_account.parse().unwrap();
    let goal_token: TokenAccount = token_out_account.parse().unwrap();
    let start = &start_token.into();
    let goal = &goal_token.into();

    let path = ref_finance::path::swap_path(&graph, start, goal)
        .await
        .unwrap();
    let tokens = ref_finance::swap::gather_token_accounts(&[&path]);
    let res = ref_finance::storage::check_and_deposit(&client, &wallet, &tokens)
        .await
        .unwrap();
    if res.is_some() {
        return "no account to deposit".to_string();
    }

    let arg = ref_finance::swap::SwapArg {
        initial_in: amount_in,
        min_out: amount_in + MilliNear::of(1).to_yocto(),
    };
    let res = ref_finance::swap::run_swap(&client, &wallet, &path, arg).await;

    match res {
        Ok((tx_hash, value)) => {
            let outcome = tx_hash.wait_for_success().await.unwrap();
            format!("Result: {value:?} ({outcome:?})")
        }
        Err(e) => format!("Error: {e}"),
    }
}

async fn estimate_trade(
    State(_): State<Arc<AppState>>,
    Json(request): Json<TradeRequest>,
) -> Json<TradeResponse> {
    let pools = pool_info::PoolInfoList::read_from_db(Some(request.timestamp))
        .await
        .unwrap();
    let graph = ref_finance::path::graph::TokenGraph::new(pools);
    let amount_in = request.amount_in.as_yoctonear();
    let start_token: TokenAccount = request.token_in.into();
    let goal_token: TokenAccount = request.token_out.into();
    let start = &start_token.into();
    let goal = &goal_token.into();

    let goals = graph.update_graph(start).unwrap();
    let is_goal = goals.contains(goal);
    if !is_goal {
        panic!("goal not found");
    }
    let values: HashMap<_, _> = graph
        .list_values(amount_in, start, &[goal.clone()])
        .unwrap()
        .into_iter()
        .collect();
    let value = values.get(goal).expect("goal not found");

    let result = TradeResponse {
        amount_out: YoctoNearToken::from_yocto(*value),
    };

    Json(result)
}
