use crate::persistence::Persistence;
use crate::ref_finance::pool;
use axum::extract::State;
use axum::routing::get;
use axum::Router;
use std::sync::Arc;

struct AppState {
    pstnce: Persistence,
}

pub async fn run() {
    let state = Arc::new(AppState {
        pstnce: Persistence::new().await.unwrap(),
    });
    let app = Router::new()
        .route("/healthcheck", get(|| async { "OK" }))
        .route("/counter", get(get_counter))
        .with_state(state.clone())
        .route("/counter/increase", get(inc_counter))
        .with_state(state.clone())
        .route("/pools/update", get(update_pools))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn get_counter(State(state): State<Arc<AppState>>) -> String {
    let cur = state.pstnce.get_counter().await.unwrap();
    format!("Counter: {cur}")
}

async fn inc_counter(State(state): State<Arc<AppState>>) -> String {
    let cur = state.pstnce.increment().await.unwrap();
    format!("Counter: {cur}",)
}

async fn update_pools(State(_): State<Arc<AppState>>) -> String {
    let pools = pool::get_all_from_node().await.unwrap();
    let count = pools.0.len();
    pool::update_all(pools).await.unwrap();
    format!("Pools: {}", count)
}
