use super::AppState;
use crate::logging::*;
use crate::persistence::prediction_record::PredictionRecord;
use crate::{
    persistence::TimeRange, ref_finance::token_account::TokenAccount,
    trade::rate_stats::SameBaseTokenRates,
};
use axum::{
    Router,
    extract::{Json, Query, State},
    routing::{get, post},
};
use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use common::ApiResponse;
use common::stats::{DescribesRequest, GetValuesRequest, GetValuesResponse, ValueAtTime};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

fn path(sub: &str) -> String {
    format!("/stats/{sub}")
}

pub fn add_route(app: Router<Arc<AppState>>) -> Router<Arc<AppState>> {
    app.route(&path("describes"), post(make_descs))
        .route(&path("get_values"), post(get_values))
        .route(&path("prediction_records"), get(get_prediction_records))
        .route(&path("prediction_mape"), get(get_prediction_mape))
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

#[derive(Debug, Deserialize)]
struct PredictionRecordsQuery {
    limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct PredictionRecordResponse {
    id: i32,
    evaluation_period_id: String,
    token: String,
    quote_token: String,
    predicted_price: BigDecimal,
    prediction_time: NaiveDateTime,
    target_time: NaiveDateTime,
    actual_price: Option<BigDecimal>,
    mape: Option<f64>,
    absolute_error: Option<BigDecimal>,
    evaluated_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Serialize)]
struct PredictionRecordsResponse {
    records: Vec<PredictionRecordResponse>,
    total: usize,
}

async fn get_prediction_records(
    State(_): State<Arc<AppState>>,
    Query(query): Query<PredictionRecordsQuery>,
) -> Json<ApiResponse<PredictionRecordsResponse, String>> {
    let log = DEFAULT.new(o!("function" => "get_prediction_records"));
    let limit = query.limit.unwrap_or(20);

    info!(log, "start"; "limit" => limit);

    match PredictionRecord::get_recent_evaluated(limit).await {
        Ok(records) => {
            let total = records.len();
            let records: Vec<PredictionRecordResponse> = records
                .into_iter()
                .map(|r| PredictionRecordResponse {
                    id: r.id,
                    evaluation_period_id: r.evaluation_period_id,
                    token: r.token,
                    quote_token: r.quote_token,
                    predicted_price: r.predicted_price,
                    prediction_time: r.prediction_time,
                    target_time: r.target_time,
                    actual_price: r.actual_price,
                    mape: r.mape,
                    absolute_error: r.absolute_error,
                    evaluated_at: r.evaluated_at,
                })
                .collect();

            info!(log, "success"; "count" => total);
            Json(ApiResponse::Success(PredictionRecordsResponse {
                records,
                total,
            }))
        }
        Err(e) => {
            error!(log, "failed to get prediction records"; "error" => ?e);
            Json(ApiResponse::Error(e.to_string()))
        }
    }
}

#[derive(Debug, Deserialize)]
struct PredictionMapeQuery {
    window: Option<i64>,
    min_samples: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
struct PredictionMapeResponse {
    rolling_mape: Option<f64>,
    sample_count: usize,
    window: i64,
}

async fn get_prediction_mape(
    State(_): State<Arc<AppState>>,
    Query(query): Query<PredictionMapeQuery>,
) -> Json<ApiResponse<PredictionMapeResponse, String>> {
    let log = DEFAULT.new(o!("function" => "get_prediction_mape"));
    let window = query.window.unwrap_or(10);
    let min_samples = query.min_samples.unwrap_or(3);

    info!(log, "start"; "window" => window, "min_samples" => min_samples);

    match PredictionRecord::get_recent_evaluated(window).await {
        Ok(records) => {
            let mape_values: Vec<f64> = records.iter().filter_map(|r| r.mape).collect();
            let sample_count = mape_values.len();

            let rolling_mape = if sample_count >= min_samples {
                Some(mape_values.iter().sum::<f64>() / sample_count as f64)
            } else {
                None
            };

            info!(log, "success";
                "rolling_mape" => rolling_mape.map(|m| format!("{:.2}%", m)).unwrap_or_else(|| "N/A".to_string()),
                "sample_count" => sample_count);
            Json(ApiResponse::Success(PredictionMapeResponse {
                rolling_mape,
                sample_count,
                window,
            }))
        }
        Err(e) => {
            error!(log, "failed to get prediction MAPE"; "error" => ?e);
            Json(ApiResponse::Error(e.to_string()))
        }
    }
}
