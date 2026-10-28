use super::*;
use crate::credit::Reservation;
use mockito::{Matcher, Server, ServerGuard};
use std::sync::Mutex;

const TX: &str = "6LAHAus8voUyKFffiFri6UX6yMPCtsctqRzLzmvyrmYM";

fn today() -> chrono::NaiveDate {
    chrono::Utc::now().date_naive()
}

fn credits(n: u32) -> Mutex<Reservation> {
    Mutex::new(Reservation::new(today(), n))
}

fn fast_delays() -> RetryDelays {
    RetryDelays {
        after_429: Duration::from_millis(5),
        after_network_error: Duration::from_millis(5),
    }
}

fn client(server: &ServerGuard) -> NearBlocksClient {
    NearBlocksClient::new(
        &server.url(),
        &SecretString::new("test-key-123".to_owned()),
        Arc::new(RateLimiter::new(10_000, Duration::ZERO)),
        fast_delays(),
    )
    .unwrap()
}

#[tokio::test]
async fn list_sends_auth_user_agent_limit_and_cursor_as_query() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("GET", "/v3/accounts/v2.ref-finance.near/receipts")
        .match_header("authorization", "Bearer test-key-123")
        .match_header("user-agent", USER_AGENT_VALUE)
        .match_header("accept", "application/json")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("limit".into(), "100".into()),
            Matcher::UrlEncoded("next".into(), "abc=/?".into()),
        ]))
        .with_status(200)
        .with_body(
            r#"{"data":[{"receipt_id":"r1"},{"receipt_id":"r2"}],"meta":{"next_page":"cur2"}}"#,
        )
        .create_async()
        .await;
    let c = client(&server);
    let source = credits(5);
    let page = c
        .list_receipts(&ListFrom::Cursor("abc=/?".to_owned()), &source)
        .await
        .unwrap();
    mock.assert_async().await;
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.next_cursor.as_deref(), Some("cur2"));
    assert_eq!(
        source.lock().unwrap().remaining(),
        4,
        "exactly one credit spent"
    );
}

#[tokio::test]
async fn list_before_ts_and_last_page_without_cursor() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("GET", "/v3/accounts/v2.ref-finance.near/receipts")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("limit".into(), "100".into()),
            Matcher::UrlEncoded("before_ts".into(), "1792843229499015627".into()),
        ]))
        .with_body(r#"{"data":[],"meta":{"next_page":null}}"#)
        .create_async()
        .await;
    let c = client(&server);
    let page = c
        .list_receipts(&ListFrom::BeforeNs(1_792_843_229_499_015_627), &credits(1))
        .await
        .unwrap();
    mock.assert_async().await;
    assert!(page.items.is_empty());
    assert_eq!(page.next_cursor, None);
}

#[tokio::test]
async fn txn_receipts_uses_the_validated_hash_in_the_path() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("GET", format!("/v3/txns/{TX}/receipts").as_str())
        .with_body(r#"{"data":[{"receipt_id":"a","receipts":[]}]}"#)
        .create_async()
        .await;
    let c = client(&server);
    let roots = c
        .txn_receipts(&TxHash::new(TX).unwrap(), &credits(1))
        .await
        .unwrap();
    mock.assert_async().await;
    assert_eq!(roots.len(), 1);

    // A single object under `data` is accepted too.
    let mock = server
        .mock("GET", format!("/v3/txns/{TX}/receipts").as_str())
        .with_body(r#"{"data":{"receipt_id":"a"}}"#)
        .create_async()
        .await;
    let roots = c
        .txn_receipts(&TxHash::new(TX).unwrap(), &credits(1))
        .await
        .unwrap();
    mock.assert_async().await;
    assert_eq!(roots.len(), 1);
}

#[tokio::test]
async fn no_credit_means_no_request() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("GET", Matcher::Any)
        .expect(0)
        .create_async()
        .await;
    let c = client(&server);
    let err = c
        .list_receipts(&ListFrom::Newest, &credits(0))
        .await
        .unwrap_err();
    assert!(matches!(err, FetchError::NoCredit));
    mock.assert_async().await;
}

#[tokio::test]
async fn one_429_is_retried_two_are_rate_limited() {
    let mut server = Server::new_async().await;
    let path = format!("/v3/txns/{TX}/receipts");
    let first = server
        .mock("GET", path.as_str())
        .with_status(429)
        .expect(1)
        .create_async()
        .await;
    let second = server
        .mock("GET", path.as_str())
        .with_body(r#"{"data":[]}"#)
        .expect(1)
        .create_async()
        .await;
    let c = client(&server);
    let source = credits(5);
    let roots = c
        .txn_receipts(&TxHash::new(TX).unwrap(), &source)
        .await
        .unwrap();
    assert!(roots.is_empty());
    first.assert_async().await;
    second.assert_async().await;
    assert_eq!(
        source.lock().unwrap().remaining(),
        3,
        "both attempts charged"
    );

    let always = server
        .mock("GET", path.as_str())
        .with_status(429)
        .expect(2)
        .create_async()
        .await;
    let source = credits(5);
    let err = c
        .txn_receipts(&TxHash::new(TX).unwrap(), &source)
        .await
        .unwrap_err();
    assert!(matches!(err, FetchError::RateLimited), "{err}");
    always.assert_async().await;
    assert_eq!(source.lock().unwrap().remaining(), 3);
}

#[tokio::test]
async fn other_http_errors_and_redirects_are_not_retried() {
    let mut server = Server::new_async().await;
    let path = format!("/v3/txns/{TX}/receipts");
    let m500 = server
        .mock("GET", path.as_str())
        .with_status(500)
        .expect(1)
        .create_async()
        .await;
    let c = client(&server);
    let source = credits(5);
    let err = c
        .txn_receipts(&TxHash::new(TX).unwrap(), &source)
        .await
        .unwrap_err();
    assert!(
        matches!(err, FetchError::Http(StatusCode::INTERNAL_SERVER_ERROR)),
        "{err}"
    );
    m500.assert_async().await;
    assert_eq!(source.lock().unwrap().remaining(), 4);

    let redirect = server
        .mock("GET", path.as_str())
        .with_status(302)
        .with_header("location", "https://example.com/elsewhere")
        .expect(1)
        .create_async()
        .await;
    let err = c
        .txn_receipts(&TxHash::new(TX).unwrap(), &source)
        .await
        .unwrap_err();
    assert!(
        matches!(err, FetchError::Http(StatusCode::FOUND)),
        "redirects are not followed: {err}"
    );
    redirect.assert_async().await;
}

#[tokio::test]
async fn oversized_body_and_invalid_json_are_errors() {
    let mut server = Server::new_async().await;
    let path = format!("/v3/txns/{TX}/receipts");
    let big = "x".repeat(usize::try_from(MAX_BODY_BYTES).unwrap() + 1);
    let m = server
        .mock("GET", path.as_str())
        .with_body(big)
        .expect(1)
        .create_async()
        .await;
    let c = client(&server);
    let err = c
        .txn_receipts(&TxHash::new(TX).unwrap(), &credits(1))
        .await
        .unwrap_err();
    assert!(matches!(err, FetchError::BodyTooLarge), "{err}");
    m.assert_async().await;

    let m = server
        .mock("GET", path.as_str())
        .with_body("not json")
        .expect(1)
        .create_async()
        .await;
    let err = c
        .txn_receipts(&TxHash::new(TX).unwrap(), &credits(1))
        .await
        .unwrap_err();
    assert!(matches!(err, FetchError::Json(_)), "{err}");
    m.assert_async().await;

    let m = server
        .mock("GET", path.as_str())
        .with_body(r#"{"data":5}"#)
        .expect(1)
        .create_async()
        .await;
    let err = c
        .txn_receipts(&TxHash::new(TX).unwrap(), &credits(1))
        .await
        .unwrap_err();
    assert!(matches!(err, FetchError::Shape(_)), "{err}");
    m.assert_async().await;
}

#[tokio::test]
async fn missing_or_null_data_is_a_shape_error_not_an_empty_answer() {
    let mut server = Server::new_async().await;
    let path = format!("/v3/txns/{TX}/receipts");
    let c = client(&server);
    for body in [r#"{"message":"not found"}"#, r#"{"data":null}"#] {
        let m = server
            .mock("GET", path.as_str())
            .with_body(body)
            .expect(1)
            .create_async()
            .await;
        let err = c
            .txn_receipts(&TxHash::new(TX).unwrap(), &credits(1))
            .await
            .unwrap_err();
        assert!(matches!(err, FetchError::Shape(_)), "{body}: {err}");
        m.assert_async().await;
    }
}

#[tokio::test]
async fn network_errors_retry_up_to_max_attempts_charging_each() {
    // Nothing listens on this port.
    let c = NearBlocksClient::new(
        "http://127.0.0.1:9",
        &SecretString::new("k".to_owned()),
        Arc::new(RateLimiter::new(10_000, Duration::ZERO)),
        fast_delays(),
    )
    .unwrap();
    let source = credits(10);
    let err = c
        .txn_receipts(&TxHash::new(TX).unwrap(), &source)
        .await
        .unwrap_err();
    assert!(matches!(err, FetchError::Network(_)), "{err}");
    assert_eq!(
        source.lock().unwrap().remaining(),
        10 - MAX_ATTEMPTS,
        "every attempt is charged"
    );
    let text = err.to_string();
    assert!(
        !text.contains("127.0.0.1"),
        "URL must not leak into the error: {text}"
    );
}

#[tokio::test]
async fn overlong_cursor_is_refused_before_sending() {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("GET", Matcher::Any)
        .expect(0)
        .create_async()
        .await;
    let c = client(&server);
    let err = c
        .list_receipts(
            &ListFrom::Cursor("c".repeat(MAX_CURSOR_LEN + 1)),
            &credits(1),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, FetchError::CursorTooLong(_)));
    mock.assert_async().await;
}

#[test]
fn debug_output_does_not_contain_the_key() {
    let c = NearBlocksClient::new(
        DEFAULT_BASE_URL,
        &SecretString::new("super-secret-key".to_owned()),
        Arc::new(RateLimiter::new(1, Duration::ZERO)),
        RetryDelays::default(),
    )
    .unwrap();
    let text = format!("{c:?}");
    assert!(!text.contains("super-secret-key"));
    assert!(text.contains("api.nearblocks.io"));
}
