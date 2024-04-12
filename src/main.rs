#![deny(warnings)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use axum::{Router, routing::get};
use axum::extract::State;

struct AppState {
    counter: AtomicU32,
}

#[tokio::main]
async fn main() {
    let state = Arc::new( AppState {
        counter: AtomicU32::new(0),
    });

    let app = Router::new()
        .route("/healthcheck", get(|| async { "OK"}))
        .route("/counter", get( get_counter)).with_state(Arc::clone(&state))
        .route("/counter/increase", get( inc_counter)).with_state(Arc::clone(&state));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn get_counter(State(state): State<Arc<AppState>>) -> String {
    let cur = state.counter.fetch_or(0, Ordering::Relaxed);
    format!("Counter: {}", cur)
}

async fn inc_counter(State(state): State<Arc<AppState>>) -> String {
    let cur = state.counter.fetch_add(1, Ordering::Relaxed);
    format!("Counter: {}", cur)
}
