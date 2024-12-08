use crate::milli_near::MilliNear;
use crate::persistence::tables;
use crate::ref_finance::pool_info;
use crate::ref_finance::token_account::TokenAccount;
use axum::extract::{Path, State};
use axum::routing::get;
use axum::Router;
use num_rational::Ratio;
use num_traits::ToPrimitive;
use std::sync::Arc;

struct AppState {}

pub async fn run() {
    let state = Arc::new(AppState {});
    let app = Router::new()
        .route("/healthcheck", get(|| async { "OK" }))
        .route("/counter", get(get_counter))
        .with_state(state.clone())
        .route("/counter/increase", get(inc_counter))
        .with_state(state.clone())
        .route("/pools/get_all", get(get_all_pools))
        .with_state(state.clone())
        .route(
            "/pools/estimate_return/:pool_id/:amount",
            get(estimate_return),
        )
        .with_state(state.clone())
        .route("/pools/get_return/:pool_id/:amount", get(get_return))
        .with_state(state.clone())
        .route("/pools/update_all", get(update_all_pools))
        .with_state(state.clone())
        .route("/pools/list_all_tokens", get(list_all_tokens))
        .with_state(state.clone())
        .route(
            "/pools/list_returns/:token_account/:amount",
            get(list_returns),
        )
        .with_state(state.clone())
        .route("/pools/pick_goals/:token_account/:amount", get(pick_goals))
        .with_state(state.clone())
        .route(
            "/pools/run_swap/:token_in_account/:initial_value/:token_out_account/:min_out_ratio",
            get(run_swap),
        )
        .with_state(state.clone())
        .route("/storage/deposit_min", get(storage_deposit_min))
        .with_state(state.clone())
        .route("/storage/deposit/:amount", get(storage_deposit))
        .with_state(state.clone())
        .route(
            "/storage/unregister/:token_account",
            get(storage_unregister_token),
        )
        .with_state(state.clone())
        .route("/deposit/:token_account/:amount", get(deposit_token))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn get_counter(State(_): State<Arc<AppState>>) -> String {
    let cur = tables::counter::get().await.unwrap();
    format!("Counter: {cur}")
}

async fn inc_counter(State(_): State<Arc<AppState>>) -> String {
    let cur = tables::counter::increment().await.unwrap();
    format!("Counter: {cur}",)
}

async fn get_all_pools(State(_): State<Arc<AppState>>) -> String {
    let pools = pool_info::PoolInfoList::load_from_db().await.unwrap();
    format!("Pools: {}", pools.len())
}

async fn update_all_pools(State(_): State<Arc<AppState>>) -> String {
    let pools = pool_info::PoolInfoList::read_from_node().await.unwrap();
    tables::pool_info::delete_all().await.unwrap();
    let n = pools.save_to_db().await.unwrap();
    format!("Pools: {n}")
}

async fn estimate_return(
    State(_): State<Arc<AppState>>,
    Path((pool_id, amount)): Path<(u32, u128)>,
) -> String {
    use crate::ref_finance::errors::Error;

    let pools = pool_info::PoolInfoList::load_from_db().await.unwrap();
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

    let pools = pool_info::PoolInfoList::load_from_db().await.unwrap();
    let pool = pools.get(pool_id).unwrap();
    let n = pool.len();
    assert!(n > 1, "{}", Error::InvalidPoolSize(n));
    let token_in = 0;
    let token_out = n - 1;
    let amount_in = amount;
    let pair = pool.get_pair(token_in.into(), token_out.into()).unwrap();
    let token_a = pair.token_in_id();
    let token_b = pair.token_out_id();
    let amount_out = pair.get_return(amount_in).await.unwrap();
    format!("Return: {token_a}({amount_in}) -> {token_b}({amount_out})")
}

async fn list_all_tokens(State(_): State<Arc<AppState>>) -> String {
    let pools = pool_info::PoolInfoList::load_from_db().await.unwrap();
    let tokens = crate::ref_finance::path::all_tokens(&pools);
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
    let amount_in = MilliNear::of(initial_value.replace("_", "").parse().unwrap());
    let start: TokenAccount = token_account.parse().unwrap();
    let mut sorted_returns = crate::ref_finance::path::sorted_returns(start.into(), amount_in)
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
    let amount_in: u32 = initial_value.replace("_", "").parse().unwrap();
    let start: TokenAccount = token_account.parse().unwrap();
    let goals = crate::ref_finance::path::pick_goals(start.into(), MilliNear::of(amount_in))
        .await
        .unwrap();
    let mut result = String::from(&format!("from: {token_account}({amount_in})\n"));
    match goals {
        None => {
            result.push_str("No goals found\n");
        }
        Some(previews) => {
            for preview in previews {
                let input_value = preview.input_value;
                let token_name = preview.token.to_string();
                let gain = MilliNear::from_yocto(preview.output_value - input_value.to_yocto());
                result.push_str(&format!("{input_value:?} -> {token_name} -> {gain:?}\n"));
            }
        }
    }
    result
}

async fn run_swap(
    State(_): State<Arc<AppState>>,
    Path((token_in_account, initial_value, token_out_account, min_out_ratio)): Path<(
        String,
        String,
        String,
        u128,
    )>,
) -> String {
    let amount_in: u128 = initial_value.replace("_", "").parse().unwrap();
    let start: TokenAccount = token_in_account.parse().unwrap();
    let goal: TokenAccount = token_out_account.parse().unwrap();
    let res =
        crate::ref_finance::swap::run_swap(start.into(), goal.into(), amount_in, min_out_ratio)
            .await;

    match res {
        Ok(value) => format!("Result: {value}"),
        Err(e) => format!("Error: {e}"),
    }
}

async fn storage_deposit_min(State(_): State<Arc<AppState>>) -> String {
    let bounds = crate::ref_finance::storage::check_bounds().await.unwrap();
    let value = bounds.min.0;
    let res = crate::ref_finance::storage::deposit(value, true).await;
    match res {
        Ok(_) => format!("Deposited: {value}"),
        Err(e) => format!("Error: {e}"),
    }
}

async fn storage_deposit(State(_): State<Arc<AppState>>, Path(amount): Path<String>) -> String {
    let amount: u128 = amount.replace("_", "").parse().unwrap();
    let res = crate::ref_finance::storage::deposit(amount, false).await;
    match res {
        Ok(_) => format!("Deposited: {amount}"),
        Err(e) => format!("Error: {e}"),
    }
}

async fn storage_unregister_token(
    State(_): State<Arc<AppState>>,
    Path(token_account): Path<String>,
) -> String {
    let token: TokenAccount = token_account.parse().unwrap();
    let res = crate::ref_finance::deposit::unregister_tokens(&[token]).await;
    match res {
        Ok(_) => format!("Unregistered: {token_account}"),
        Err(e) => format!("Error: {e}"),
    }
}

async fn deposit_token(
    State(_): State<Arc<AppState>>,
    Path((token_account, amount)): Path<(String, String)>,
) -> String {
    let amount: u128 = amount.replace("_", "").parse().unwrap();
    let token: TokenAccount = token_account.parse().unwrap();
    let res = crate::ref_finance::deposit::deposit(token, amount).await;
    match res {
        Ok(_) => format!("Deposited: {amount}"),
        Err(e) => format!("Error: {e}"),
    }
}
