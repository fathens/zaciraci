use super::AppState;
use crate::logging::*;
use crate::{
    persistence::TimeRange, ref_finance::token_account::TokenAccount,
    trade::rate_stats::SameBaseTokenRates,
};
use axum::{
    Router,
    extract::{Json, State},
    routing::post,
};
use std::sync::Arc;
use zaciraci_common::ApiResponse;
use zaciraci_common::stats::{DescribesRequest, GetValuesRequest, GetValuesResponse, ValueAtTime};

fn path(sub: &str) -> String {
    format!("/stats/{sub}")
}

pub fn add_route(app: Router<Arc<AppState>>) -> Router<Arc<AppState>> {
    app.route(&path("describes"), post(make_descs))
        .route(&path("get_values"), post(get_values))
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
    let descs = rates.aggregate(period).describes();
    info!(log, "success";
        "descs_count" => descs.len(),
    );
    serde_json::to_string(&descs).unwrap()
}

async fn get_values(
    State(_): State<Arc<AppState>>,
    Json(request): Json<GetValuesRequest>,
) -> Json<ApiResponse<GetValuesResponse, String>> {
    let log = DEFAULT.new(o!(
        "function" => "get_values",
        "quote_token" => format!("{}", request.quote_token),
        "base_token" => format!("{}", request.base_token),
        "start" => format!("{}", request.start),
        "end" => format!("{}", request.end),
    ));
    info!(log, "start");
    let quote_token = request.quote_token;
    let base_token = request.base_token;
    let range = TimeRange {
        start: request.start,
        end: request.end,
    };
    let rates =
        match SameBaseTokenRates::load(&quote_token.into(), &base_token.into(), &range).await {
            Ok(rates) => rates,
            Err(e) => {
                error!(log, "Failed to load rates"; "error" => ?e);
                return Json(ApiResponse::Error(e.to_string()));
            }
        };
    let values: Vec<_> = rates
        .points
        .into_iter()
        .map(|p| ValueAtTime {
            time: p.timestamp,
            value: p.price,
        })
        .collect();

    info!(log, "success";
        "values_count" => values.len(),
    );
    Json(ApiResponse::Success(GetValuesResponse { values }))
}
