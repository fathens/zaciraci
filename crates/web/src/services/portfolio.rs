use crate::proto::portfolio_service_server::PortfolioService;
use crate::proto::{
    EvaluationPeriodEntry, GetEvaluationPeriodRequest, GetEvaluationPeriodResponse,
    GetEvaluationPeriodsRequest, GetEvaluationPeriodsResponse, GetLatestBatchRequest,
    GetLatestBatchResponse, GetLatestRatesRequest, GetLatestRatesResponse, GetRateHistoryRequest,
    GetRateHistoryResponse, GetSelectedTokensRequest, GetSelectedTokensResponse,
    GetTradesByBatchRequest, GetTradesByBatchResponse, GetTradesRequest, GetTradesResponse,
    RateEntry, TokenRateHistory, TradeEntry,
};
use common::types::{TimeRange, TokenAccount, TokenInAccount};
use persistence::evaluation_period::EvaluationPeriod;
use persistence::token_rate::TokenRate;
use persistence::trade_transaction::TradeTransaction;
use prost_types::Timestamp;
use std::str::FromStr;
use tonic::{Request, Response, Status};

pub struct PortfolioServiceImpl;

fn naive_to_timestamp(dt: chrono::NaiveDateTime) -> Option<Timestamp> {
    Some(Timestamp {
        seconds: dt.and_utc().timestamp(),
        nanos: dt.and_utc().timestamp_subsec_nanos() as i32,
    })
}

fn timestamp_to_naive(ts: &Timestamp) -> Option<chrono::NaiveDateTime> {
    chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32).map(|dt| dt.naive_utc())
}

fn period_to_entry(ep: EvaluationPeriod) -> EvaluationPeriodEntry {
    EvaluationPeriodEntry {
        period_id: ep.period_id,
        start_time: naive_to_timestamp(ep.start_time),
        initial_value: ep.initial_value.to_string(),
        created_at: naive_to_timestamp(ep.created_at),
    }
}

fn trade_to_entry(tx: TradeTransaction) -> TradeEntry {
    TradeEntry {
        tx_id: tx.tx_id,
        trade_batch_id: tx.trade_batch_id,
        from_token: tx.from_token,
        from_amount: tx.from_amount.to_string(),
        to_token: tx.to_token,
        to_amount: tx.to_amount.to_string(),
        timestamp: naive_to_timestamp(tx.timestamp),
        evaluation_period_id: tx.evaluation_period_id,
    }
}

fn token_rate_to_entry(rate: TokenRate) -> RateEntry {
    RateEntry {
        base_token: rate.base.to_string(),
        quote_token: rate.quote.to_string(),
        rate: rate.exchange_rate.raw_rate().to_string(),
        decimals: rate.exchange_rate.decimals() as u32,
        timestamp: naive_to_timestamp(rate.timestamp),
    }
}

#[tonic::async_trait]
impl PortfolioService for PortfolioServiceImpl {
    async fn get_evaluation_periods(
        &self,
        _request: Request<GetEvaluationPeriodsRequest>,
    ) -> Result<Response<GetEvaluationPeriodsResponse>, Status> {
        let periods = EvaluationPeriod::get_all_async()
            .await
            .map_err(|e| Status::internal(format!("Failed to get evaluation periods: {e}")))?;

        let entries = periods.into_iter().map(period_to_entry).collect();

        Ok(Response::new(GetEvaluationPeriodsResponse {
            periods: entries,
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
            period: period.map(period_to_entry),
        }))
    }

    async fn get_selected_tokens(
        &self,
        request: Request<GetSelectedTokensRequest>,
    ) -> Result<Response<GetSelectedTokensResponse>, Status> {
        let period_id = &request.get_ref().period_id;
        if period_id.is_empty() {
            return Err(Status::invalid_argument("period_id must not be empty"));
        }

        let period = EvaluationPeriod::get_by_period_id_async(period_id.clone())
            .await
            .map_err(|e| Status::internal(format!("Failed to get evaluation period: {e}")))?;

        let tokens = period
            .and_then(|ep| ep.selected_tokens)
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .collect();

        Ok(Response::new(GetSelectedTokensResponse { tokens }))
    }

    async fn get_trades(
        &self,
        request: Request<GetTradesRequest>,
    ) -> Result<Response<GetTradesResponse>, Status> {
        let req = request.get_ref();
        let limit = if req.limit == 0 { 50 } else { req.limit.min(1000) } as i64;
        let offset = req.offset as i64;
        let period_id = req.evaluation_period_id.clone().filter(|s| !s.is_empty());

        let (trades, total) = TradeTransaction::find_paginated_async(period_id, limit, offset)
            .await
            .map_err(|e| Status::internal(format!("Failed to get trades: {e}")))?;

        let entries = trades.into_iter().map(trade_to_entry).collect();

        Ok(Response::new(GetTradesResponse {
            trades: entries,
            total_count: total as u32,
        }))
    }

    async fn get_trades_by_batch(
        &self,
        request: Request<GetTradesByBatchRequest>,
    ) -> Result<Response<GetTradesByBatchResponse>, Status> {
        let batch_id = &request.get_ref().batch_id;
        if batch_id.is_empty() {
            return Err(Status::invalid_argument("batch_id must not be empty"));
        }

        let trades = TradeTransaction::find_by_batch_id_async(batch_id.clone())
            .await
            .map_err(|e| Status::internal(format!("Failed to get trades by batch: {e}")))?;

        let entries = trades.into_iter().map(trade_to_entry).collect();

        Ok(Response::new(GetTradesByBatchResponse { trades: entries }))
    }

    async fn get_latest_batch(
        &self,
        _request: Request<GetLatestBatchRequest>,
    ) -> Result<Response<GetLatestBatchResponse>, Status> {
        let batch_id = TradeTransaction::get_latest_batch_id_async()
            .await
            .map_err(|e| Status::internal(format!("Failed to get latest batch ID: {e}")))?;

        let trades = match &batch_id {
            Some(bid) => TradeTransaction::find_by_batch_id_async(bid.clone())
                .await
                .map_err(|e| Status::internal(format!("Failed to get trades by batch: {e}")))?,
            None => vec![],
        };

        let entries = trades.into_iter().map(trade_to_entry).collect();

        Ok(Response::new(GetLatestBatchResponse {
            batch_id,
            trades: entries,
        }))
    }

    async fn get_latest_rates(
        &self,
        _request: Request<GetLatestRatesRequest>,
    ) -> Result<Response<GetLatestRatesResponse>, Status> {
        let rates = persistence::token_rate::get_all_latest()
            .await
            .map_err(|e| Status::internal(format!("Failed to get latest rates: {e}")))?;

        let entries = rates.into_iter().map(token_rate_to_entry).collect();

        Ok(Response::new(GetLatestRatesResponse { rates: entries }))
    }

    async fn get_rate_history(
        &self,
        request: Request<GetRateHistoryRequest>,
    ) -> Result<Response<GetRateHistoryResponse>, Status> {
        let req = request.get_ref();

        if req.base_tokens.is_empty() {
            return Err(Status::invalid_argument("base_tokens must not be empty"));
        }
        if req.quote_token.is_empty() {
            return Err(Status::invalid_argument("quote_token must not be empty"));
        }

        let start_time = req
            .start_time
            .as_ref()
            .and_then(timestamp_to_naive)
            .ok_or_else(|| Status::invalid_argument("start_time is required"))?;
        let end_time = req
            .end_time
            .as_ref()
            .and_then(timestamp_to_naive)
            .ok_or_else(|| Status::invalid_argument("end_time is required"))?;

        let quote: TokenInAccount = TokenAccount::from_str(&req.quote_token)
            .map_err(|e| Status::invalid_argument(format!("invalid quote_token: {e}")))?
            .into();

        let range = TimeRange {
            start: start_time,
            end: end_time,
        };

        let rates_map = persistence::token_rate::get_rates_for_multiple_tokens_simple(
            &req.base_tokens,
            &quote,
            &range,
        )
        .await
        .map_err(|e| Status::internal(format!("Failed to get rate history: {e}")))?;

        let histories = rates_map
            .into_iter()
            .map(|(base_token, rates)| TokenRateHistory {
                base_token,
                rates: rates.into_iter().map(token_rate_to_entry).collect(),
            })
            .collect();

        Ok(Response::new(GetRateHistoryResponse { histories }))
    }
}

#[cfg(test)]
mod tests;
