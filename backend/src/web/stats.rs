use super::AppState;
use crate::{
    persistence::TimeRange, ref_finance::token_account::TokenAccount,
    trade::trade::SameBaseTokenRates,
};
use axum::{
    Router,
    extract::{Json, State},
    routing::post,
};
use std::sync::Arc;
use zaciraci_common::stats::DescribesRequest;

fn path(sub: &str) -> String {
    format!("/stats/{sub}")
}

pub fn add_route(app: Router<Arc<AppState>>) -> Router<Arc<AppState>> {
    app.route(&path("describes"), post(make_descs))
}

async fn make_descs(
    State(_): State<Arc<AppState>>,
    Json(request): Json<DescribesRequest>,
) -> String {
    let quote_token: TokenAccount = request.quote_token.parse().unwrap();
    let base_token: TokenAccount = request.base_token.parse().unwrap();
    let range = TimeRange {
        start: request.start,
        end: request.end,
    };
    let rates = SameBaseTokenRates::load(&quote_token.into(), &base_token.into(), &range)
        .await
        .unwrap();
    let period = request.period;
    let descs = rates.stats(period).describes();
    serde_json::to_string(&descs).unwrap()
}
