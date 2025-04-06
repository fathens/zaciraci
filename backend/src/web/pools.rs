use super::AppState;
use crate::jsonrpc::{GasInfo, SentTx};
use crate::logging::*;
use crate::ref_finance::path::graph::TokenGraph;
use crate::ref_finance::pool_info::PoolInfoList;
use crate::ref_finance::pool_info::TokenPairLike;
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use crate::types::{MicroNear, MilliNear};
use crate::{jsonrpc, ref_finance, wallet};
use axum::Json;
use axum::{
    Router,
    extract::{Path, State},
    routing::{get, post},
};
use num_bigint::BigUint;
use num_rational::Ratio;
use num_traits::ToPrimitive;
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
    let pools = PoolInfoList::read_from_db(None).await.unwrap();
    format!("Pools: {}", pools.len())
}

async fn estimate_return(
    State(_): State<Arc<AppState>>,
    Path((pool_id, amount)): Path<(u32, u128)>,
) -> String {
    use crate::ref_finance::errors::Error;

    let client = jsonrpc::new_client();
    let pools = PoolInfoList::read_from_node(&client).await.unwrap();
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
    let pools = PoolInfoList::read_from_db(None).await.unwrap();
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
    let pools = PoolInfoList::read_from_db(None).await.unwrap();
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
    let pools = PoolInfoList::read_from_db(None).await.unwrap();
    let graph = TokenGraph::new(pools);
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
    let pools = PoolInfoList::read_from_db(None).await.unwrap();
    let graph = TokenGraph::new(pools);
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
    let pools = PoolInfoList::read_from_node(&client).await.unwrap();
    let graph = TokenGraph::new(pools);
    let amount_in: u128 = initial_value.replace("_", "").parse().unwrap();
    let start_token: TokenAccount = token_in_account.parse().unwrap();
    let goal_token: TokenAccount = token_out_account.parse().unwrap();
    let start = &start_token.into();
    let goal = &goal_token.into();

    let path = ref_finance::path::swap_path(&graph, start, goal)
        .await
        .unwrap();
    let tokens = ref_finance::swap::gather_token_accounts(&[&path.0]);
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
    let res = ref_finance::swap::run_swap(&client, &wallet, &path.0, arg).await;

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
    let log = DEFAULT.new(o!(
        "function" => "estimate_trade",
    ));

    let timestamp = request.timestamp;
    let amount_in = request.amount_in.as_yoctonear();
    let start_token: TokenAccount = request.token_in.into();
    let goal_token: TokenAccount = request.token_out.into();
    let start = &start_token.into();
    let goal = &goal_token.into();
    info!(log, "start";
        "timestamp" => %timestamp,
        "amount_in" => %amount_in,
        "start_token" => %start,
        "goal_token" => %goal,
    );

    let pools = PoolInfoList::read_from_db(Some(timestamp)).await.unwrap();
    fn out_amount(
        iteration_id: usize,
        prev_out: u128,
        pools: Arc<PoolInfoList>,
        amount_in: u128,
        start: &TokenInAccount,
        goal: &TokenOutAccount,
    ) -> u128 {
        let log = DEFAULT.new(o!(
            "function" => "estimate_trade::out_amount",
            "iteration_id" => iteration_id,
            "prev_out" => format!("{prev_out}"),
            "amount_in" => format!("{amount_in}"),
            "start_token" => format!("{start}"),
            "goal_token" => format!("{goal}"),
        ));
        let graph = TokenGraph::new(pools);
        let goals = graph.update_graph(start).unwrap();
        if !goals.contains(goal) {
            panic!("goal not found");
        }

        let path = graph.get_path(start, goal).unwrap();
        if log.is_info_enabled() {
            let mut path_info = String::new();
            for token_pair in path.0.iter() {
                let line = format!(
                    "{}: {} -> {}\n",
                    token_pair.pool_id(),
                    token_pair.token_in_id(),
                    token_pair.token_out_id(),
                );
                path_info.push_str(&line);
            }
            info!(log, "path found";
                "path" => %path_info,
            );
        }
        let amount_out = path.calc_value(amount_in).unwrap();
        info!(log, "value calculated";
            "amount_out" => %amount_out,
        );
        if prev_out > 0 {
            let reversed_path = path.reversed();
            let reversed_prev_out = reversed_path.calc_value(prev_out).unwrap();
            let reversed_amount_out = reversed_path.calc_value(amount_out).unwrap();
            info!(log, "reversed value calculated";
                "prev_out" => %prev_out,
                "amount_out" => %amount_out,
                "reversed_prev_out" => %reversed_prev_out,
                "reversed_amount_out" => %reversed_amount_out,
            );
        }
        amount_out
    }
    let mut amount_outs: Vec<BigUint> = vec![];
    for i in 0..10 {
        let prev_out = if i > 0 { amount_outs[i - 1].to_u128().unwrap() } else { 0 };
        let v = out_amount(i, prev_out, pools.clone(), amount_in, &start, &goal);
        amount_outs.push(BigUint::from(v));
    }
    let amount_out = amount_outs.iter().sum::<BigUint>() / BigUint::from(amount_outs.len());

    Json(TradeResponse {
        amount_out: YoctoNearToken::from_yocto(amount_out.to_u128().unwrap()),
    })
}
