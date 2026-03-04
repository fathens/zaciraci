use crate::proto::portfolio_service_server::PortfolioService;
use crate::proto::{
    GetEvaluationPeriodsRequest, GetEvaluationPeriodsResponse, GetPortfolioHoldingsRequest,
    GetPortfolioHoldingsResponse,
};
use common::types::near_units::YoctoValue;
use common::types::time_range::TimeRange;
use common::types::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use common::types::token_types::{ExchangeRate, TokenAmount};
use persistence::evaluation_period::EvaluationPeriod;
use persistence::portfolio_holding::{DbPortfolioHolding, PortfolioHolding};
use persistence::token_rate::TokenRate;
use std::collections::HashMap;
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

fn wnear_token() -> TokenAccount {
    let s = if common::config::startup::get().is_mainnet {
        "wrap.near"
    } else {
        "wrap.testnet"
    };
    s.parse().expect("hardcoded wNEAR token ID must be valid")
}

fn db_holding_to_proto(
    holding: &DbPortfolioHolding,
    wnear: &TokenAccount,
    rates_map: &HashMap<TokenOutAccount, Vec<TokenRate>>,
) -> Result<crate::proto::PortfolioHolding, Status> {
    let parsed = holding
        .parse_holdings()
        .map_err(|e| Status::internal(format!("Failed to parse token holdings: {e}")))?;
    let ts = holding.timestamp;

    let mut token_holdings = Vec::with_capacity(parsed.len());
    let mut total_yocto = YoctoValue::zero();

    for th in &parsed {
        let amount =
            TokenAmount::from_smallest_units(th.balance.as_bigdecimal().clone(), th.decimals);

        let yocto = if th.token == *wnear {
            let rate = ExchangeRate::wnear();
            let near_value = amount / &rate;
            near_value.to_yocto()
        } else {
            let token_out = TokenOutAccount::from(th.token.clone());

            let rates = rates_map.get(&token_out).ok_or_else(|| {
                Status::failed_precondition(format!(
                    "No rates found for {} in time range",
                    th.token
                ))
            })?;

            let spot_rate = TokenRate::latest_spot_rate(rates).ok_or_else(|| {
                Status::failed_precondition(format!(
                    "No rates found for {} in time range",
                    th.token
                ))
            })?;

            let near_value = amount / &spot_rate;
            near_value.to_yocto()
        };

        token_holdings.push(crate::proto::TokenHolding {
            token: th.token.to_string(),
            balance: th.balance.to_string(),
            decimals: u32::from(th.decimals),
            value_wnear: yocto.to_string(),
        });

        total_yocto = total_yocto + yocto;
    }

    Ok(crate::proto::PortfolioHolding {
        timestamp: Some(naive_to_timestamp(ts)),
        token_holdings,
        total_value_wnear: total_yocto.to_string(),
    })
}

/// 全 holding から wNEAR 以外のトークンを収集し、一括でレートを取得
async fn fetch_all_rates(
    db_holdings: &[DbPortfolioHolding],
    wnear: &TokenAccount,
) -> Result<HashMap<TokenOutAccount, Vec<TokenRate>>, Status> {
    // 全 holding をパースして wNEAR 以外のトークンを収集
    let mut tokens: Vec<TokenOutAccount> = Vec::new();
    let mut min_ts = chrono::NaiveDateTime::MAX;
    let mut max_ts = chrono::NaiveDateTime::MIN;

    for h in db_holdings {
        let parsed = h
            .parse_holdings()
            .map_err(|e| Status::internal(format!("Failed to parse token holdings: {e}")))?;
        for th in &parsed {
            if th.token != *wnear {
                let token_out = TokenOutAccount::from(th.token.clone());
                if !tokens.contains(&token_out) {
                    tokens.push(token_out);
                }
            }
        }
        if h.timestamp < min_ts {
            min_ts = h.timestamp;
        }
        if h.timestamp > max_ts {
            max_ts = h.timestamp;
        }
    }

    if tokens.is_empty() {
        return Ok(HashMap::new());
    }

    let range = TimeRange {
        start: min_ts - chrono::Duration::hours(RATE_LOOKBACK_HOURS),
        end: max_ts,
    };

    let wnear_in = TokenInAccount::from(wnear.clone());

    TokenRate::get_rates_for_multiple_tokens(&tokens, &wnear_in, &range)
        .await
        .map_err(|e| Status::internal(format!("Failed to get rates: {e}")))
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

        let wnear = wnear_token();
        let rates_map = fetch_all_rates(&db_holdings, &wnear).await?;

        let mut holdings = Vec::with_capacity(db_holdings.len());
        for h in &db_holdings {
            holdings.push(db_holding_to_proto(h, &wnear, &rates_map)?);
        }

        Ok(Response::new(GetPortfolioHoldingsResponse { holdings }))
    }
}
