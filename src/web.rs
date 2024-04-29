use crate::persistence::tables;
use crate::ref_finance::pool;
use axum::extract::{Path, State};
use axum::routing::get;
use axum::Router;
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
        .route("/pools/update_all", get(update_all_pools))
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
    let pools = pool::PoolInfoList::from_db().await.unwrap();
    format!("Pools: {}", pools.len())
}

async fn update_all_pools(State(_): State<Arc<AppState>>) -> String {
    let pools = pool::PoolInfoList::from_node().await.unwrap();
    let n = pools.update_all().await.unwrap();
    format!("Pools: {n}")
}

async fn estimate_return(
    State(_): State<Arc<AppState>>,
    Path((pool_id, amount)): Path<(usize, u128)>,
) -> String {
    use crate::ref_finance::errors::Error;

    let pools = pool::PoolInfoList::from_db().await.unwrap();
    let pool = pools.get(pool_id).unwrap();
    let amount_in = amount;
    let tokens = pool.tokens().unwrap();
    let n = tokens.len();
    assert!(n > 1, "{}", Error::InvalidPoolSize(n));
    let amount_out = pool.estimate_return(0, amount_in, n - 1).unwrap();
    let token_a = tokens[0].0;
    let token_b = tokens[n - 1].0;
    format!("Estimated: {token_a}({amount_in}) -> {token_b}({amount_out})")
}
