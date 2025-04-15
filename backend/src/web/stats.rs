use super::AppState;
use crate::logging::*;
use crate::{
    persistence::TimeRange, ref_finance::token_account::TokenAccount,
    trade::stats::SameBaseTokenRates,
};
use axum::{
    extract::{Json, State},
    routing::post,
    Router,
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
    let log = DEFAULT.new(o!(
        "function" => "make_descs",
        "quote_token" => format!("{}", request.quote_token),
        "base_token" => format!("{}", request.base_token),
        "start" => format!("{}", request.start),
        "end" => format!("{}", request.end),
        "period" => format!("{}", request.period),
    ));
    info!(log, "start");

    let quote_token: TokenAccount = request
        .quote_token
        .parse()
        .map_err(|e| {
            info!(log, "Failed to parse quote token"; "error" => ?e);
            e
        })
        .unwrap();
    let base_token: TokenAccount = request
        .base_token
        .parse()
        .map_err(|e| {
            info!(log, "Failed to parse base token"; "error" => ?e);
            e
        })
        .unwrap();
    let range = TimeRange {
        start: request.start,
        end: request.end,
    };
    let rates = SameBaseTokenRates::load(&quote_token.into(), &base_token.into(), &range)
        .await
        .map_err(|e| {
            info!(log, "Failed to load rates"; "error" => ?e);
            e
        })
        .unwrap();
    let period = request.period;
    let descs = rates.stats(period).describes();
    info!(log, "success";
        "descs_count" => descs.len(),
    );
    serde_json::to_string(&descs).unwrap()
}
