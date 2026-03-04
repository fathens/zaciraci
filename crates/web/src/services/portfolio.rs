use crate::proto::portfolio_service_server::PortfolioService;
use crate::proto::{
    GetEvaluationPeriodRequest, GetEvaluationPeriodResponse, GetEvaluationPeriodsRequest,
    GetEvaluationPeriodsResponse,
};
use persistence::evaluation_period::EvaluationPeriod;
use tonic::{Request, Response, Status};

fn naive_to_timestamp(dt: chrono::NaiveDateTime) -> prost_types::Timestamp {
    let utc = dt.and_utc();
    prost_types::Timestamp {
        seconds: utc.timestamp(),
        nanos: 0,
    }
}

fn evaluation_period_to_proto(
    ep: EvaluationPeriod,
) -> crate::proto::EvaluationPeriod {
    let selected_tokens = ep
        .selected_tokens
        .unwrap_or_default()
        .into_iter()
        .flatten()
        .collect();

    crate::proto::EvaluationPeriod {
        id: ep.id,
        period_id: ep.period_id,
        start_time: Some(naive_to_timestamp(ep.start_time)),
        initial_value: ep.initial_value.to_string(),
        selected_tokens,
    }
}

pub struct PortfolioServiceImpl;

#[tonic::async_trait]
impl PortfolioService for PortfolioServiceImpl {
    async fn get_evaluation_periods(
        &self,
        request: Request<GetEvaluationPeriodsRequest>,
    ) -> Result<Response<GetEvaluationPeriodsResponse>, Status> {
        let req = request.get_ref();
        let page = i64::from(req.page.max(0));
        let page_size = i64::from(req.page_size.clamp(1, 200));

        let (periods, total_count) = tokio::try_join!(
            EvaluationPeriod::get_paginated_async(page, page_size),
            EvaluationPeriod::count_all_async(),
        )
        .map_err(|e| Status::internal(format!("Failed to get evaluation periods: {e}")))?;

        let periods = periods.into_iter().map(evaluation_period_to_proto).collect();

        Ok(Response::new(GetEvaluationPeriodsResponse {
            periods,
            total_count,
        }))
    }

    async fn get_evaluation_period(
        &self,
        request: Request<GetEvaluationPeriodRequest>,
    ) -> Result<Response<GetEvaluationPeriodResponse>, Status> {
        let period_id = &request.get_ref().period_id;

        if period_id.is_empty() {
            return Err(Status::invalid_argument("period_id must not be empty"));
        }

        let period = EvaluationPeriod::get_by_period_id_async(period_id.clone())
            .await
            .map_err(|e| Status::internal(format!("Failed to get evaluation period: {e}")))?;

        Ok(Response::new(GetEvaluationPeriodResponse {
            period: period.map(evaluation_period_to_proto),
        }))
    }
}
