//! HTTP client for the NearBlocks v3 API.
//!
//! Every attempt consumes one [`Credit`] *before* the request is sent
//! (fail-closed: no credit, no request), goes through the shared
//! [`RateLimiter`], and reads at most [`MAX_BODY_BYTES`]. Redirects are
//! never followed (one credit must map to at most one provider request),
//! the base URL must be HTTPS outside tests, and the API key only ever
//! travels in a sensitive `Authorization` header.
//!
//! Retry policy (mirrors the Python collector):
//! - `429`: wait and retry once; a second `429` is [`FetchError::RateLimited`]
//!   and the caller must cool down.
//! - connection / timeout errors: up to `MAX_ATTEMPTS` attempts.
//! - any other non-2xx status: no retry.

use crate::credit::{Credit, CreditSource};
use crate::rate_limit::RateLimiter;
use common::config::SecretString;
use common::types::TxHash;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use reqwest::{StatusCode, Url};
use serde_json::Value;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

pub const DEFAULT_BASE_URL: &str = "https://api.nearblocks.io";
/// Cloudflare rejects the default reqwest/urllib agents.
pub const USER_AGENT_VALUE: &str = "zaciraci-flow-sampler";
pub const REF_ACCOUNT: &str = "v2.ref-finance.near";
pub const LIST_LIMIT: u32 = 100;
pub const MAX_BODY_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_CURSOR_LEN: usize = 512;
pub const MAX_ATTEMPTS: u32 = 4;
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Delays between attempts. Production values are constants; tests shrink
/// them.
#[derive(Debug, Clone, Copy)]
pub struct RetryDelays {
    pub after_429: Duration,
    pub after_network_error: Duration,
}

impl Default for RetryDelays {
    fn default() -> Self {
        Self {
            after_429: Duration::from_secs(10),
            after_network_error: Duration::from_secs(15),
        }
    }
}

#[derive(Debug, Error)]
pub enum FetchError {
    /// No credit available: the request was not sent.
    #[error("no request credit available")]
    NoCredit,
    /// Two consecutive 429 responses; the caller must cool down.
    #[error("rate limited by the provider")]
    RateLimited,
    #[error("HTTP {0}")]
    Http(StatusCode),
    /// Connection or timeout failure on every attempt (URL omitted).
    #[error("network error: {0}")]
    Network(String),
    #[error("response body exceeded {MAX_BODY_BYTES} bytes")]
    BodyTooLarge,
    #[error("response is not JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("response JSON is not the expected shape: {0}")]
    Shape(&'static str),
    #[error("cursor is {0} characters, longer than {MAX_CURSOR_LEN}")]
    CursorTooLong(usize),
}

/// Where a list page starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListFrom {
    /// The newest receipts.
    Newest,
    /// The opaque `meta.next_page` cursor of a previous page.
    Cursor(String),
    /// Receipts older than this block timestamp (nanoseconds).
    BeforeNs(u64),
}

/// One page of the account receipts list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListPage {
    pub items: Vec<Value>,
    /// Cursor of the next (older) page, if any.
    pub next_cursor: Option<String>,
}

/// The two endpoints the collector uses. Implemented by
/// [`NearBlocksClient`] and by in-memory fakes in tests.
pub trait NearBlocksApi: Send + Sync {
    fn list_receipts(
        &self,
        from: &ListFrom,
        credits: &dyn CreditSource,
    ) -> impl Future<Output = Result<ListPage, FetchError>> + Send;

    /// Top-level receipts of one transaction (a tree each).
    fn txn_receipts(
        &self,
        tx_hash: &TxHash,
        credits: &dyn CreditSource,
    ) -> impl Future<Output = Result<Vec<Value>, FetchError>> + Send;
}

pub struct NearBlocksClient {
    http: reqwest::Client,
    base: Url,
    limiter: Arc<RateLimiter>,
    delays: RetryDelays,
}

impl std::fmt::Debug for NearBlocksClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NearBlocksClient")
            .field("base", &self.base.as_str())
            .finish_non_exhaustive()
    }
}

impl NearBlocksClient {
    /// `base` is HTTPS in production; a plain-HTTP base is accepted only so
    /// tests can point at a local mock server.
    pub fn new(
        base: &str,
        api_key: &SecretString,
        limiter: Arc<RateLimiter>,
        delays: RetryDelays,
    ) -> anyhow::Result<Self> {
        let base = Url::parse(base)?;
        let mut auth = HeaderValue::from_str(&format!("Bearer {}", api_key.expose()))
            .map_err(|_| anyhow::anyhow!("API key contains characters not allowed in a header"))?;
        auth.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
        headers.insert(AUTHORIZATION, auth);
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(HTTP_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .https_only(base.scheme() == "https")
            .build()?;
        Ok(Self {
            http,
            base,
            limiter,
            delays,
        })
    }

    /// GET `path` with `query`, charging one credit per attempt.
    async fn get_json(
        &self,
        path: &str,
        query: &[(&str, String)],
        credits: &dyn CreditSource,
    ) -> Result<Value, FetchError> {
        let mut url = self.base.clone();
        url.set_path(path);
        if !query.is_empty() {
            url.query_pairs_mut()
                .extend_pairs(query.iter().map(|(k, v)| (*k, v.as_str())));
        }
        let mut got_429 = false;
        let mut attempts = 0;
        loop {
            attempts += 1;
            let credit = credits
                .take(chrono::Utc::now().date_naive())
                .ok_or(FetchError::NoCredit)?;
            self.limiter.acquire().await;
            match self.send_once(credit, url.clone()).await {
                Ok(value) => return Ok(value),
                Err(Attempt::Status(StatusCode::TOO_MANY_REQUESTS)) => {
                    if got_429 {
                        return Err(FetchError::RateLimited);
                    }
                    got_429 = true;
                    tokio::time::sleep(self.delays.after_429).await;
                }
                Err(Attempt::Status(status)) => return Err(FetchError::Http(status)),
                Err(Attempt::Network(e)) => {
                    if attempts >= MAX_ATTEMPTS {
                        return Err(FetchError::Network(e));
                    }
                    tokio::time::sleep(self.delays.after_network_error).await;
                }
                Err(Attempt::Fatal(e)) => return Err(e),
            }
        }
    }

    /// One attempt. Takes the credit by value: it is spent whether or not
    /// the provider answers.
    async fn send_once(&self, credit: Credit, url: Url) -> Result<Value, Attempt> {
        let _spent = credit;
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| Attempt::Network(e.without_url().to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(Attempt::Status(status));
        }
        let body = read_body_with_cap(response, MAX_BODY_BYTES).await?;
        serde_json::from_slice(&body).map_err(|e| Attempt::Fatal(FetchError::Json(e)))
    }
}

enum Attempt {
    Status(StatusCode),
    Network(String),
    Fatal(FetchError),
}

/// Stream the body into memory, aborting once it exceeds `cap` (guards
/// against a chunked response with no Content-Length).
async fn read_body_with_cap(mut response: reqwest::Response, cap: u64) -> Result<Vec<u8>, Attempt> {
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| Attempt::Network(e.without_url().to_string()))?
    {
        if (buf.len() as u64).saturating_add(chunk.len() as u64) > cap {
            return Err(Attempt::Fatal(FetchError::BodyTooLarge));
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

fn data_array(mut value: Value) -> Result<Vec<Value>, FetchError> {
    match value.get_mut("data").map(Value::take) {
        Some(Value::Array(items)) => Ok(items),
        Some(Value::Null) | None => Ok(Vec::new()),
        Some(single @ Value::Object(_)) => Ok(vec![single]),
        Some(_) => Err(FetchError::Shape("data is neither an array nor an object")),
    }
}

impl NearBlocksApi for NearBlocksClient {
    async fn list_receipts(
        &self,
        from: &ListFrom,
        credits: &dyn CreditSource,
    ) -> Result<ListPage, FetchError> {
        let mut query = vec![("limit", LIST_LIMIT.to_string())];
        match from {
            ListFrom::Newest => {}
            ListFrom::Cursor(cursor) => {
                if cursor.len() > MAX_CURSOR_LEN {
                    return Err(FetchError::CursorTooLong(cursor.len()));
                }
                query.push(("next", cursor.clone()));
            }
            ListFrom::BeforeNs(ns) => query.push(("before_ts", ns.to_string())),
        }
        let path = format!("/v3/accounts/{REF_ACCOUNT}/receipts");
        let mut value = self.get_json(&path, &query, credits).await?;
        let next_cursor = value
            .get("meta")
            .and_then(|m| m.get("next_page"))
            .and_then(Value::as_str)
            .filter(|c| !c.is_empty())
            .map(str::to_owned);
        if next_cursor
            .as_ref()
            .is_some_and(|c| c.len() > MAX_CURSOR_LEN)
        {
            return Err(FetchError::CursorTooLong(
                next_cursor.map_or(0, |c| c.len()),
            ));
        }
        let items = match value.get_mut("data").map(Value::take) {
            Some(Value::Array(items)) => items,
            Some(Value::Null) | None => Vec::new(),
            Some(_) => return Err(FetchError::Shape("data is not an array")),
        };
        Ok(ListPage { items, next_cursor })
    }

    async fn txn_receipts(
        &self,
        tx_hash: &TxHash,
        credits: &dyn CreditSource,
    ) -> Result<Vec<Value>, FetchError> {
        // `tx_hash` is validated base58, so it cannot carry path or query
        // delimiters into the URL.
        let path = format!("/v3/txns/{}/receipts", tx_hash.as_str());
        let value = self.get_json(&path, &[], credits).await?;
        data_array(value)
    }
}

#[cfg(test)]
mod tests;
