mod basic;
mod pools;
mod storage;

use axum::Router;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

struct AppState {}

pub async fn run() {
    let state = Arc::new(AppState {});

    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any);

    let app = add_routes(
        Router::new(),
        &[storage::add_route, pools::add_route, basic::add_route],
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
