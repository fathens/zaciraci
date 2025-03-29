use super::AppState;
use axum::Router;
use std::sync::Arc;

pub fn add_route(app: Router<Arc<AppState>>) -> Router<Arc<AppState>> {
    app
}