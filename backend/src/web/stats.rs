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
            Some(value) if value.is_finite() && value > 0.0 => Some(ValueAtTime {
                time: p.timestamp,
                value,
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

    // データ数の検証
    if values.len() < 4 {
        error!(log, "Insufficient data points for prediction";
            "values_count" => values.len(),
            "min_required" => 4,
        );
        return Json(ApiResponse::Error(format!(
            "Insufficient data points: {} (minimum 4 required for prediction)",
            values.len()
        )));
    }

    info!(log, "success";
        "values_count" => values.len(),
    );
    Json(ApiResponse::Success(GetValuesResponse { values }))
}
