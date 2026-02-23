use super::*;
use crate::proto::portfolio_service_server::PortfolioService;
use tonic::Request;

#[tokio::test]
async fn test_get_evaluation_periods_returns_ok() {
    let svc = PortfolioServiceImpl;
    let response = svc
        .get_evaluation_periods(Request::new(GetEvaluationPeriodsRequest {}))
        .await
        .unwrap();
    let _ = response.into_inner().periods;
}

#[tokio::test]
async fn test_get_evaluation_period_empty_id_rejected() {
    let svc = PortfolioServiceImpl;
    let result = svc
        .get_evaluation_period(Request::new(GetEvaluationPeriodRequest {
            period_id: String::new(),
        }))
        .await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn test_get_evaluation_period_not_found() {
    let svc = PortfolioServiceImpl;
    let response = svc
        .get_evaluation_period(Request::new(GetEvaluationPeriodRequest {
            period_id: "nonexistent_period".to_string(),
        }))
        .await
        .unwrap();
    assert!(response.into_inner().period.is_none());
}

#[tokio::test]
async fn test_get_selected_tokens_empty_id_rejected() {
    let svc = PortfolioServiceImpl;
    let result = svc
        .get_selected_tokens(Request::new(GetSelectedTokensRequest {
            period_id: String::new(),
        }))
        .await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn test_get_selected_tokens_not_found_returns_empty() {
    let svc = PortfolioServiceImpl;
    let response = svc
        .get_selected_tokens(Request::new(GetSelectedTokensRequest {
            period_id: "nonexistent_period".to_string(),
        }))
        .await
        .unwrap();
    assert!(response.into_inner().tokens.is_empty());
}

#[tokio::test]
async fn test_get_trades_default_pagination() {
    let svc = PortfolioServiceImpl;
    let response = svc
        .get_trades(Request::new(GetTradesRequest {
            evaluation_period_id: None,
            limit: 0,
            offset: 0,
        }))
        .await
        .unwrap();
    let _ = response.into_inner();
}

#[tokio::test]
async fn test_get_trades_by_batch_empty_id_rejected() {
    let svc = PortfolioServiceImpl;
    let result = svc
        .get_trades_by_batch(Request::new(GetTradesByBatchRequest {
            batch_id: String::new(),
        }))
        .await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn test_get_latest_batch_returns_ok() {
    let svc = PortfolioServiceImpl;
    let response = svc
        .get_latest_batch(Request::new(GetLatestBatchRequest {}))
        .await
        .unwrap();
    let _ = response.into_inner();
}

#[tokio::test]
async fn test_get_latest_rates_returns_ok() {
    let svc = PortfolioServiceImpl;
    let response = svc
        .get_latest_rates(Request::new(GetLatestRatesRequest {}))
        .await
        .unwrap();
    let _ = response.into_inner().rates;
}

#[tokio::test]
async fn test_get_rate_history_empty_tokens_rejected() {
    let svc = PortfolioServiceImpl;

    let result = svc
        .get_rate_history(Request::new(GetRateHistoryRequest {
            base_tokens: vec![],
            quote_token: "wrap.near".to_string(),
            start_time: Some(Timestamp {
                seconds: 1000,
                nanos: 0,
            }),
            end_time: Some(Timestamp {
                seconds: 2000,
                nanos: 0,
            }),
        }))
        .await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn test_get_rate_history_empty_quote_rejected() {
    let svc = PortfolioServiceImpl;

    let result = svc
        .get_rate_history(Request::new(GetRateHistoryRequest {
            base_tokens: vec!["token.near".to_string()],
            quote_token: String::new(),
            start_time: Some(Timestamp {
                seconds: 1000,
                nanos: 0,
            }),
            end_time: Some(Timestamp {
                seconds: 2000,
                nanos: 0,
            }),
        }))
        .await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn test_get_rate_history_missing_time_rejected() {
    let svc = PortfolioServiceImpl;

    let result = svc
        .get_rate_history(Request::new(GetRateHistoryRequest {
            base_tokens: vec!["token.near".to_string()],
            quote_token: "wrap.near".to_string(),
            start_time: None,
            end_time: Some(Timestamp {
                seconds: 2000,
                nanos: 0,
            }),
        }))
        .await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
}
