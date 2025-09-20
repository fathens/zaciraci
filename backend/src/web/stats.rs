use super::AppState;
use crate::logging::*;
use crate::{
    persistence::TimeRange, ref_finance::token_account::TokenAccount,
    trade::stats::SameBaseTokenRates,
};
use axum::{
    Router,
    extract::{Json, State},
    routing::post,
};
use bigdecimal::{BigDecimal, FromPrimitive};
use num_traits::ToPrimitive;
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
) -> std::result::Result<String, String> {
    let log = DEFAULT.new(o!(
        "function" => "make_descs",
        "quote_token" => format!("{}", request.quote_token),
        "base_token" => format!("{}", request.base_token),
        "start" => format!("{}", request.start),
        "end" => format!("{}", request.end),
        "period" => format!("{}", request.period),
    ));
    info!(log, "start");

    let quote_token: TokenAccount = request.quote_token.parse().map_err(
        |e: near_primitives::account::id::ParseAccountError| {
            info!(log, "Failed to parse quote token"; "error" => ?e);
            e.to_string()
        },
    )?;
    let base_token: TokenAccount = request.base_token.parse().map_err(
        |e: near_primitives::account::id::ParseAccountError| {
            info!(log, "Failed to parse base token"; "error" => ?e);
            e.to_string()
        },
    )?;
    let range = TimeRange {
        start: request.start,
        end: request.end,
    };
    let rates = SameBaseTokenRates::load(&quote_token.into(), &base_token.into(), &range)
        .await
        .map_err(|e| {
            info!(log, "Failed to load rates"; "error" => ?e);
            e.to_string()
        })?;
    let period_secs = request.period.num_seconds().max(0) as u32;
    let descs = rates
        .aggregate(period_secs)
        .describes(period_secs)
        .map_err(|e| {
            info!(log, "Failed to describe rates"; "error" => ?e);
            e.to_string()
        })?;
    info!(log, "success";
        "descs_count" => descs.len(),
    );
    serde_json::to_string(&descs).map_err(|e| e.to_string())
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
    let quote_token: TokenAccount = match request.quote_token.try_into() {
        Ok(token) => token,
        Err(e) => {
            error!(log, "Failed to parse quote token"; "error" => ?e);
            return Json(ApiResponse::Error(e.to_string()));
        }
    };
    let base_token: TokenAccount = match request.base_token.try_into() {
        Ok(token) => token,
        Err(e) => {
            error!(log, "Failed to parse base token"; "error" => ?e);
            return Json(ApiResponse::Error(e.to_string()));
        }
    };
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
        .filter_map(|p| match p.rate.to_f64() {
            Some(value) if value.is_finite() && value >= 0.0 => Some(ValueAtTime {
                time: p.timestamp,
                value: BigDecimal::from_f64(value).unwrap_or_default(),
            }),
            Some(value) => {
                error!(log, "Invalid rate value filtered out";
                    "value" => %value,
                    "timestamp" => %p.timestamp
                );
                None
            }
            None => {
                error!(log, "Failed to convert BigDecimal to f64";
                    "rate" => %p.rate,
                    "timestamp" => %p.timestamp
                );
                None
            }
        })
        .collect();

    info!(log, "success";
        "values_count" => values.len(),
    );
    Json(ApiResponse::Success(GetValuesResponse { values }))
}
