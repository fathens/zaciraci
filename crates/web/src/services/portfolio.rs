use crate::proto::portfolio_service_server::PortfolioService;
use crate::proto::{
    GetEvaluationPeriodsRequest, GetEvaluationPeriodsResponse, GetPortfolioHoldingsRequest,
    GetPortfolioHoldingsResponse,
};
use bigdecimal::BigDecimal;
use common::types::time_range::TimeRange;
use common::types::token_account::{TokenInAccount, TokenOutAccount};
use common::types::token_types::{ExchangeRate, TokenAmount};
use persistence::evaluation_period::EvaluationPeriod;
use persistence::portfolio_holding::{DbPortfolioHolding, PortfolioHolding};
use persistence::token_rate::TokenRate;
use std::str::FromStr;
use tonic::{Request, Response, Status};

const RATE_LOOKBACK_HOURS: i64 = 24;

fn naive_to_timestamp(dt: chrono::NaiveDateTime) -> prost_types::Timestamp {
    let utc = dt.and_utc();
    prost_types::Timestamp {
        seconds: utc.timestamp(),
        nanos: 0,
    }
}

fn evaluation_period_to_proto(ep: EvaluationPeriod) -> crate::proto::EvaluationPeriod {
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

fn wnear_token_id() -> &'static str {
    if common::config::startup::get().is_mainnet {
        "wrap.near"
    } else {
        "wrap.testnet"
    }
}

async fn db_holding_to_proto(
    holding: DbPortfolioHolding,
    wnear_token: &str,
) -> Result<crate::proto::PortfolioHolding, Status> {
    let parsed = holding
        .parse_holdings()
        .map_err(|e| Status::internal(format!("Failed to parse token holdings: {e}")))?;
    let ts = holding.timestamp;
    let range = TimeRange {
        start: ts - chrono::Duration::hours(RATE_LOOKBACK_HOURS),
        end: ts,
    };

    let wnear_in: TokenInAccount = wnear_token
        .parse()
        .map_err(|e| Status::internal(format!("Invalid wNEAR token ID: {e}")))?;

    let mut token_holdings = Vec::with_capacity(parsed.len());
    let mut total_yocto = BigDecimal::from(0);

    for th in &parsed {
        let balance = BigDecimal::from_str(&th.balance)
            .map_err(|e| Status::internal(format!("Invalid balance for {}: {e}", th.token)))?;
        let amount = TokenAmount::from_smallest_units(balance, th.decimals);

        let yocto_str = if th.token == wnear_token {
            let rate = ExchangeRate::wnear();
            let near_value = amount / &rate;
            let yocto = near_value.to_yocto();
            yocto.to_string()
        } else {
            let token_out: TokenOutAccount = th
                .token
                .parse()
                .map_err(|e| Status::internal(format!("Invalid token ID {}: {e}", th.token)))?;

            let rates = TokenRate::get_rates_in_time_range(&range, &token_out, &wnear_in)
                .await
                .map_err(|e| {
                    Status::internal(format!("Failed to get rates for {}: {e}", th.token))
                })?;

            let spot_rate = TokenRate::latest_spot_rate(&rates).ok_or_else(|| {
                Status::internal(format!("No rates found for {} in time range", th.token))
            })?;

            let near_value = amount / &spot_rate;
            let yocto = near_value.to_yocto();
            yocto.to_string()
        };

        let yocto_val = BigDecimal::from_str(&yocto_str)
            .map_err(|e| Status::internal(format!("Invalid yocto value: {e}")))?;
        total_yocto += yocto_val;

        token_holdings.push(crate::proto::TokenHolding {
            token: th.token.clone(),
            balance: th.balance.clone(),
            decimals: u32::from(th.decimals),
            value_wnear: yocto_str,
        });
    }

    Ok(crate::proto::PortfolioHolding {
        timestamp: Some(naive_to_timestamp(ts)),
        token_holdings,
        total_value_wnear: total_yocto.to_string(),
    })
}

pub struct PortfolioServiceImpl;

#[cfg(test)]
mod tests;

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

        let periods = periods
            .into_iter()
            .map(evaluation_period_to_proto)
            .collect();

        Ok(Response::new(GetEvaluationPeriodsResponse {
            periods,
            total_count,
        }))
    }

    async fn get_portfolio_holdings(
        &self,
        request: Request<GetPortfolioHoldingsRequest>,
    ) -> Result<Response<GetPortfolioHoldingsResponse>, Status> {
        let period_id = &request.get_ref().period_id;

        if period_id.is_empty() {
            return Err(Status::invalid_argument("period_id must not be empty"));
        }

        let db_holdings = PortfolioHolding::get_by_period_async(period_id.clone())
            .await
            .map_err(|e| Status::internal(format!("Failed to get portfolio holdings: {e}")))?;

        let wnear_token = wnear_token_id();
        let mut holdings = Vec::with_capacity(db_holdings.len());
        for h in db_holdings {
            holdings.push(db_holding_to_proto(h, wnear_token).await?);
        }

        Ok(Response::new(GetPortfolioHoldingsResponse { holdings }))
    }
}
