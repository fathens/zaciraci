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

/// Total HTTP timeout applied to JWKS fetches. Without this the background
/// refresh task could block indefinitely on a half-open connection, leaving
/// the cache stale (or past its TTL) until the TCP stack gives up on its own.
const JWKS_HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// TCP connect timeout for JWKS fetches.
const JWKS_HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Build the shared `reqwest::Client` used for all JWKS fetches.
fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(JWKS_HTTP_TIMEOUT)
        .connect_timeout(JWKS_HTTP_CONNECT_TIMEOUT)
        .build()
        .expect("reqwest client with static timeouts must build")
}

/// A single JSON Web Key entry from Google's JWKS endpoint.
#[derive(Debug, Deserialize)]
struct Jwk {
    kid: String,
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
    /// A fetch failure during construction is logged but does not prevent
    /// the `JwksCache` from being built; the background refresh task (or a
    /// subsequent call) will keep retrying. However, the end-to-end request
    /// path is fail-closed: until the cache actually has keys, the validator
    /// translates the empty snapshot into `AuthError::JwksUnavailable`
    /// (`Status::unavailable`). A stale snapshot that has passed its TTL is
    /// also wiped via `clear_if_expired`. So "construction never fails" must
    /// not be read as "tokens are accepted without keys".
    pub(crate) async fn new(url: impl Into<String>) -> Arc<Self> {
        let cache = Arc::new(Self {
            http: build_http_client(),
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
    #[cfg(test)]
    pub(crate) fn from_keys(keys: HashMap<String, DecodingKey>) -> Arc<Self> {
        let now = Instant::now();
        Arc::new(Self {
            http: build_http_client(),
            url: String::new(),
            inner: RwLock::new(CachedJwks {
                keys,
                fetched_at: Some(now),
                expires_at: Some(now + DEFAULT_TTL),
            }),
        })
    }

    /// Acquire a read guard, recovering from poisoning by taking the inner
    /// value. Any poisoned state is produced by a writer panic while swapping
    /// the snapshot; the data itself is still a valid (possibly stale) cache.
    fn read_guard(&self) -> std::sync::RwLockReadGuard<'_, CachedJwks> {
        self.inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Acquire a write guard, recovering from poisoning the same way.
    fn write_guard(&self) -> std::sync::RwLockWriteGuard<'_, CachedJwks> {
        self.inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Look up a decoding key by key ID. Returns `None` if the key is not
    /// currently in the cache (either never fetched, or key rotation).
    pub fn get(&self, kid: &str) -> Option<DecodingKey> {
        self.read_guard().keys.get(kid).cloned()
    }

    /// Returns true if the cache currently has no keys loaded.
    pub fn is_empty(&self) -> bool {
        self.read_guard().is_empty()
    }

    /// Clear the cached keys if the current snapshot has passed its
    /// `expires_at`. Used as a fail-closed safeguard in the background
    /// refresh task: if Google rotates or emergency-revokes a key and the
    /// refresh keeps failing past the TTL, we would rather reject all
    /// requests (`JwksUnavailable`) than keep validating signatures with a
    /// potentially-revoked key.
    fn clear_if_expired(&self, now: Instant) -> bool {
        let mut guard = self.write_guard();
        let expired = match guard.expires_at {
            Some(expires) => now >= expires,
            None => false,
        };
        if expired && !guard.keys.is_empty() {
            guard.keys.clear();
            true
        } else {
            false
        }
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
            let mut guard = self.write_guard();
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
                let needs_refresh = this.read_guard().needs_refresh(Instant::now());

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
                        // If the current snapshot has fully expired, drop the
                        // stale keys so requests fail closed
                        // (JwksUnavailable) instead of being validated with
                        // possibly-revoked material.
                        if this.clear_if_expired(Instant::now()) {
                            warn!(
                                log,
                                "jwks_expired_cleared";
                                "reason" => "refresh failing past TTL",
                            );
                        }
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
        // An absent `alg` is also skipped: Google's JWKS always includes it,
        // so a missing value is unusual and should not be trusted as
        // implicitly RS256.
        match jwk.alg.as_deref() {
            Some("RS256") => {}
            Some(other) => {
                warn!(log, "unsupported_jwk_alg"; "kid" => &jwk.kid, "alg" => other);
                continue;
            }
            None => {
                warn!(log, "jwk_missing_alg"; "kid" => &jwk.kid);
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

/// Algorithm accepted for ID token verification.
pub const ACCEPTED_ALGORITHM: Algorithm = Algorithm::RS256;

#[cfg(test)]
mod tests;
