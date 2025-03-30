mod basic;
mod ollama;
mod pools;
mod storage;
mod stats;

use axum::Router;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

struct AppState {}

pub async fn run() {
    let state = Arc::new(AppState {});

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = add_routes(
        Router::new(),
        &[
            basic::add_route,
            pools::add_route,
            storage::add_route,
            ollama::add_route,
            stats::add_route,
        ],
    )
    .with_state(state)
    .layer(cors);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

fn add_routes<T>(app: Router<T>, funcs: &[fn(Router<T>) -> Router<T>]) -> Router<T> {
    let mut app = app;
    for func in funcs {
        app = func(app);
    }
    app
}