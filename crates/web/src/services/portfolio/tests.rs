use super::*;
use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use std::str::FromStr;

fn make_evaluation_period(
    id: i32,
    period_id: &str,
    start_time: NaiveDateTime,
    initial_value: &str,
    tokens: Option<Vec<Option<String>>>,
) -> EvaluationPeriod {
    EvaluationPeriod {
        id,
        period_id: period_id.to_string(),
        start_time,
        initial_value: BigDecimal::from_str(initial_value).unwrap(),
        selected_tokens: tokens,
        created_at: start_time,
    }
}

#[test]
fn test_naive_to_timestamp() {
    let dt = NaiveDateTime::parse_from_str("2025-01-15 10:30:00", "%Y-%m-%d %H:%M:%S").unwrap();
    let ts = naive_to_timestamp(dt);
    assert_eq!(ts.seconds, dt.and_utc().timestamp());
    assert_eq!(ts.nanos, 0);
}

#[test]
fn test_naive_to_timestamp_epoch() {
    let dt = NaiveDateTime::parse_from_str("1970-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    let ts = naive_to_timestamp(dt);
    assert_eq!(ts.seconds, 0);
    assert_eq!(ts.nanos, 0);
}

#[test]
fn test_evaluation_period_to_proto_with_tokens() {
    let dt = NaiveDateTime::parse_from_str("2025-06-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    let ep = make_evaluation_period(
        1,
        "eval_abc",
        dt,
        "1000000",
        Some(vec![Some("token1".to_string()), Some("token2".to_string())]),
    );

    let proto = evaluation_period_to_proto(ep);
    assert_eq!(proto.id, 1);
    assert_eq!(proto.period_id, "eval_abc");
    assert_eq!(proto.initial_value, "1000000");
    assert_eq!(proto.selected_tokens, vec!["token1", "token2"]);
    assert!(proto.start_time.is_some());
    assert_eq!(proto.start_time.unwrap().seconds, dt.and_utc().timestamp());
}

#[test]
fn test_evaluation_period_to_proto_no_tokens() {
    let dt = NaiveDateTime::parse_from_str("2025-06-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    let ep = make_evaluation_period(2, "eval_def", dt, "500", None);

    let proto = evaluation_period_to_proto(ep);
    assert_eq!(proto.id, 2);
    assert!(proto.selected_tokens.is_empty());
}

#[test]
fn test_evaluation_period_to_proto_tokens_with_none() {
    let dt = NaiveDateTime::parse_from_str("2025-06-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    let ep = make_evaluation_period(
        3,
        "eval_ghi",
        dt,
        "999",
        Some(vec![Some("a".to_string()), None, Some("b".to_string())]),
    );

    let proto = evaluation_period_to_proto(ep);
    assert_eq!(proto.selected_tokens, vec!["a", "b"]);
}

#[tokio::test]
async fn test_get_evaluation_period_empty_period_id_returns_invalid_argument() {
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
#[serial_test::serial]
async fn test_get_evaluation_periods_returns_list() {
    let svc = PortfolioServiceImpl;
    let result = svc
        .get_evaluation_periods(Request::new(GetEvaluationPeriodsRequest {
            page: 0,
            page_size: 10,
        }))
        .await;
    assert!(result.is_ok());
    let resp = result.unwrap().into_inner();
    assert!(resp.total_count >= 0);
}

#[tokio::test]
#[serial_test::serial]
async fn test_get_evaluation_period_not_found() {
    let svc = PortfolioServiceImpl;
    let result = svc
        .get_evaluation_period(Request::new(GetEvaluationPeriodRequest {
            period_id: "eval_nonexistent_00000000".to_string(),
        }))
        .await;
    assert!(result.is_ok());
    assert!(result.unwrap().into_inner().period.is_none());
}
