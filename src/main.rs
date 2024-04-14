#![deny(warnings)]

mod persistence;

use axum::extract::State;
use axum::{routing::get, Router};
use persistence::Persistence;
use std::sync::Arc;
use once_cell::sync::Lazy;

struct AppState {
    pstnce: Persistence,
}

static APP_STATE: Lazy<Arc<AppState>> = Lazy::new(|| {
    let pstnce = Persistence::new().unwrap();
    Arc::new(AppState { pstnce })
});

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/healthcheck", get(|| async { "OK" }))
        .route("/counter", get(get_counter))
        .with_state(Arc::clone(&APP_STATE))
        .route("/counter/increase", get(inc_counter))
        .with_state(Arc::clone(&APP_STATE));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn get_counter(State(state): State<Arc<AppState>>) -> String {
    let cur = state.pstnce.get_counter().unwrap();
    format!("Counter: {}", cur)
}

async fn inc_counter(State(state): State<Arc<AppState>>) -> String {
    let cur = state.pstnce.increment().unwrap();
    format!("Counter: {}", cur)
}
