use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, DecodingKey};
use logging::{DEFAULT, info, o, warn};
use serde::Deserialize;

pub const GOOGLE_JWKS_URL: &str = "https://www.googleapis.com/oauth2/v3/certs";

/// Default TTL if Cache-Control cannot be parsed.
const DEFAULT_TTL: Duration = Duration::from_secs(3600);

/// Minimum wait before retrying after a failed fetch.
const MIN_RETRY_BACKOFF: Duration = Duration::from_secs(30);

/// Maximum wait before retrying after repeated failures.
const MAX_RETRY_BACKOFF: Duration = Duration::from_secs(600);

/// Refresh when the cached entry has reached this fraction of its lifetime.
const REFRESH_THRESHOLD_RATIO: f64 = 0.9;

/// A single JSON Web Key entry from Google's JWKS endpoint.
#[derive(Debug, Deserialize)]
struct Jwk {
    kid: String,
    #[serde(rename = "kty")]
    _kty: String,
    #[serde(default, rename = "alg")]
    alg: Option<String>,
    n: String,
    e: String,
}

#[derive(Debug, Deserialize)]
struct JwksResponse {
    keys: Vec<Jwk>,
}

/// Snapshot of the currently cached JWKS keys.
///
/// `expires_at` is `None` if no successful fetch has happened yet.
#[derive(Default)]
struct CachedJwks {
    keys: HashMap<String, DecodingKey>,
    fetched_at: Option<Instant>,
    expires_at: Option<Instant>,
}

impl CachedJwks {
    fn needs_refresh(&self, now: Instant) -> bool {
        match (self.fetched_at, self.expires_at) {
            (Some(fetched), Some(expires)) => {
                let total = expires.saturating_duration_since(fetched);
                let elapsed = now.saturating_duration_since(fetched);
                elapsed.as_secs_f64() >= total.as_secs_f64() * REFRESH_THRESHOLD_RATIO
            }
            _ => true,
        }
    }

    fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

/// Thread-safe cache of Google's JWKS decoding keys.
///
/// Lookups (`get`) are synchronous reads used by the interceptor. Refresh is
/// performed asynchronously by a background task spawned via `spawn_refresh_task`.
/// HTTP I/O is always performed outside the lock and the resulting snapshot is
/// swapped in under a brief write lock.
pub struct JwksCache {
    http: reqwest::Client,
    url: String,
    inner: RwLock<CachedJwks>,
}

impl JwksCache {
    /// Build a new cache and attempt an initial fetch.
    ///
    /// A fetch failure is logged but does not prevent construction. The cache
    /// will then be empty until the background refresh task (or a subsequent
    /// call) succeeds. This is the intended fail-open behaviour.
    pub async fn new(url: impl Into<String>) -> Arc<Self> {
        let cache = Arc::new(Self {
            http: reqwest::Client::new(),
            url: url.into(),
            inner: RwLock::new(CachedJwks::default()),
        });

        if let Err(err) = cache.refresh_once().await {
            let log = DEFAULT.new(o!("module" => "google_auth::jwks"));
            warn!(log, "initial_jwks_fetch_failed"; "error" => %err);
        }
        cache
    }

    /// Build a cache targeting Google's production JWKS endpoint.
    pub async fn new_google() -> Arc<Self> {
        Self::new(GOOGLE_JWKS_URL).await
    }

    /// Construct a cache pre-populated with the given keys.
    ///
    /// Used by tests that need deterministic key material without performing
    /// HTTP I/O. The background refresh task is not spawned; callers that use
    /// this constructor are responsible for the cache lifecycle.
    #[doc(hidden)]
    pub fn from_keys(keys: HashMap<String, DecodingKey>) -> Arc<Self> {
        let now = Instant::now();
        Arc::new(Self {
            http: reqwest::Client::new(),
            url: String::new(),
            inner: RwLock::new(CachedJwks {
                keys,
                fetched_at: Some(now),
                expires_at: Some(now + DEFAULT_TTL),
            }),
        })
    }

    /// Look up a decoding key by key ID. Returns `None` if the key is not
    /// currently in the cache (either never fetched, or key rotation).
    pub fn get(&self, kid: &str) -> Option<DecodingKey> {
        let guard = self
            .inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.keys.get(kid).cloned()
    }

    /// Returns true if the cache currently has no keys loaded.
    pub fn is_empty(&self) -> bool {
        let guard = self
            .inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.is_empty()
    }

    /// Fetch the JWKS once and swap the cache in place.
    async fn refresh_once(&self) -> Result<Duration, reqwest::Error> {
        let response = self.http.get(&self.url).send().await?.error_for_status()?;

        let ttl = parse_max_age(response.headers().get(reqwest::header::CACHE_CONTROL))
            .unwrap_or(DEFAULT_TTL);

        let jwks: JwksResponse = response.json().await?;
        let keys = decode_keys(jwks.keys);

        let now = Instant::now();
        let new_snapshot = CachedJwks {
            keys,
            fetched_at: Some(now),
            expires_at: Some(now + ttl),
        };

        // Brief write lock: swap the snapshot then release.
        {
            let mut guard = self
                .inner
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *guard = new_snapshot;
        }

        let log = DEFAULT.new(o!("module" => "google_auth::jwks"));
        info!(log, "jwks_refreshed"; "ttl_secs" => ttl.as_secs(), "fetched_at" => %now_iso());
        Ok(ttl)
    }

    /// Spawn a background task that keeps the cache warm.
    ///
    /// Strategy:
    /// - On success, sleep until `ttl * REFRESH_THRESHOLD_RATIO` has passed
    ///   before the next refresh (pre-emptive refresh).
    /// - On failure, back off exponentially from `MIN_RETRY_BACKOFF` to
    ///   `MAX_RETRY_BACKOFF`, retaining the existing cached snapshot.
    pub fn spawn_refresh_task(self: &Arc<Self>) {
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let log = DEFAULT.new(o!("module" => "google_auth::jwks::refresh"));
            let mut backoff = MIN_RETRY_BACKOFF;
            loop {
                let needs_refresh = {
                    let guard = this
                        .inner
                        .read()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    guard.needs_refresh(Instant::now())
                };

                if !needs_refresh {
                    // Sleep until near the refresh threshold.
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    continue;
                }

                match this.refresh_once().await {
                    Ok(ttl) => {
                        backoff = MIN_RETRY_BACKOFF;
                        let sleep_for =
                            Duration::from_secs_f64(ttl.as_secs_f64() * REFRESH_THRESHOLD_RATIO);
                        tokio::time::sleep(sleep_for).await;
                    }
                    Err(err) => {
                        warn!(log, "jwks_refresh_failed"; "error" => %err, "retry_in_secs" => backoff.as_secs());
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(MAX_RETRY_BACKOFF);
                    }
                }
            }
        });
    }
}

/// Parse the `max-age=N` directive out of a Cache-Control header value.
fn parse_max_age(header: Option<&reqwest::header::HeaderValue>) -> Option<Duration> {
    let value = header?.to_str().ok()?;
    for directive in value.split(',') {
        let directive = directive.trim();
        if let Some(rest) = directive.strip_prefix("max-age=")
            && let Ok(secs) = rest.trim().parse::<u64>()
        {
            return Some(Duration::from_secs(secs));
        }
    }
    None
}

/// Convert raw JWK entries into a map keyed by `kid`.
fn decode_keys(jwks: Vec<Jwk>) -> HashMap<String, DecodingKey> {
    let mut map = HashMap::new();
    let log = DEFAULT.new(o!("module" => "google_auth::jwks"));
    for jwk in jwks {
        // Only RS256 is used by Google for ID tokens. Skip anything else.
        match jwk.alg.as_deref() {
            Some("RS256") | None => {}
            Some(other) => {
                warn!(log, "unsupported_jwk_alg"; "kid" => &jwk.kid, "alg" => other);
                continue;
            }
        }
        match DecodingKey::from_rsa_components(&jwk.n, &jwk.e) {
            Ok(key) => {
                map.insert(jwk.kid, key);
            }
            Err(err) => {
                warn!(log, "jwk_decode_failed"; "kid" => &jwk.kid, "error" => %err);
            }
        }
    }
    map
}

fn now_iso() -> String {
    let now: DateTime<Utc> = Utc::now();
    now.to_rfc3339()
}

/// Algorithms accepted for ID token verification.
pub fn accepted_algorithm() -> Algorithm {
    Algorithm::RS256
}

#[cfg(test)]
mod tests;
