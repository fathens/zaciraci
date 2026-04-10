use super::*;
use reqwest::header::HeaderValue;

#[test]
fn parse_max_age_simple() {
    let value = HeaderValue::from_static("public, max-age=3600");
    let parsed = parse_max_age(Some(&value));
    assert_eq!(parsed, Some(Duration::from_secs(3600)));
}

#[test]
fn parse_max_age_with_multiple_directives() {
    let value = HeaderValue::from_static("public, max-age=21600, must-revalidate");
    let parsed = parse_max_age(Some(&value));
    assert_eq!(parsed, Some(Duration::from_secs(21600)));
}

#[test]
fn parse_max_age_missing() {
    let value = HeaderValue::from_static("public, no-cache");
    let parsed = parse_max_age(Some(&value));
    assert_eq!(parsed, None);
}

#[test]
fn parse_max_age_none_header() {
    let parsed = parse_max_age(None);
    assert_eq!(parsed, None);
}

#[test]
fn parse_max_age_zero_is_clamped_to_minimum_at_use_site() {
    // `parse_max_age` itself returns the raw parsed value; the clamp is
    // applied at the call site in `refresh_once`. This test documents the
    // contract: a hostile `max-age=0` parses successfully, and the
    // surrounding code is responsible for the `.clamp(MIN_JWKS_TTL, MAX_JWKS_TTL)`.
    let value = HeaderValue::from_static("max-age=0");
    let parsed = parse_max_age(Some(&value));
    assert_eq!(parsed, Some(Duration::from_secs(0)));
    let clamped = parsed
        .unwrap_or(DEFAULT_TTL)
        .clamp(MIN_JWKS_TTL, MAX_JWKS_TTL);
    assert_eq!(clamped, MIN_JWKS_TTL);
}

#[test]
fn parse_max_age_u64_max_is_clamped_to_maximum_at_use_site() {
    // Without the upper bound clamp, a hostile `max-age=u64::MAX` would
    // produce `Duration::from_secs(u64::MAX)`. `Instant + ttl` and
    // `ttl.mul_f64(...)` would then panic in the refresh task. The clamp
    // at the `refresh_once` call site must cap this at `MAX_JWKS_TTL`.
    let header = format!("max-age={}", u64::MAX);
    let value = HeaderValue::from_str(&header).unwrap();
    let parsed = parse_max_age(Some(&value));
    assert_eq!(parsed, Some(Duration::from_secs(u64::MAX)));
    let clamped = parsed
        .unwrap_or(DEFAULT_TTL)
        .clamp(MIN_JWKS_TTL, MAX_JWKS_TTL);
    assert_eq!(clamped, MAX_JWKS_TTL);
    // Downstream arithmetic must not panic.
    let now = Instant::now();
    let _expires = now + clamped;
    let _sleep = clamped.mul_f64(REFRESH_THRESHOLD_RATIO);
}

#[test]
fn min_refresh_sleep_caps_short_ttls() {
    // Even after the TTL clamp, the post-refresh sleep is additionally
    // floored so a tiny clamped TTL cannot drive the loop into a tight
    // cycle.
    let ttl = MIN_JWKS_TTL;
    let sleep_for = ttl.mul_f64(REFRESH_THRESHOLD_RATIO).max(MIN_REFRESH_SLEEP);
    assert!(sleep_for >= MIN_REFRESH_SLEEP);
}

#[test]
fn needs_refresh_when_empty() {
    let cached = CachedJwks::default();
    assert!(cached.needs_refresh(Instant::now()));
}

#[test]
fn needs_refresh_before_threshold() {
    let now = Instant::now();
    let cached = CachedJwks {
        keys: HashMap::new(),
        fetched_at: Some(now),
        expires_at: Some(now + Duration::from_secs(1000)),
    };
    // Immediately after fetch, no refresh needed.
    assert!(!cached.needs_refresh(now));
}

#[test]
fn needs_refresh_after_threshold() {
    let fetched = Instant::now();
    let cached = CachedJwks {
        keys: HashMap::new(),
        fetched_at: Some(fetched),
        expires_at: Some(fetched + Duration::from_secs(1000)),
    };
    // 95% of TTL past → should refresh.
    let later = fetched + Duration::from_secs(950);
    assert!(cached.needs_refresh(later));
}

#[test]
fn decode_keys_skips_unsupported_algorithm() {
    let jwks = vec![Jwk {
        kid: "bad".to_string(),
        alg: Some("ES256".to_string()),
        n: "irrelevant".to_string(),
        e: "irrelevant".to_string(),
    }];
    let keys = decode_keys(jwks);
    assert!(keys.is_empty());
}

#[test]
fn decode_keys_skips_missing_algorithm() {
    // Defence in depth: Google JWKS always returns alg; a missing alg is
    // unusual and should not be trusted as implicitly RS256.
    let jwks = vec![Jwk {
        kid: "no-alg".to_string(),
        alg: None,
        n: "irrelevant".to_string(),
        e: "irrelevant".to_string(),
    }];
    let keys = decode_keys(jwks);
    assert!(keys.is_empty());
}

#[test]
fn accepted_algorithm_is_rs256() {
    assert_eq!(ACCEPTED_ALGORITHM, Algorithm::RS256);
}

#[test]
fn clear_if_expired_drops_stale_keys() {
    let mut keys = HashMap::new();
    keys.insert(
        "kid-1".to_string(),
        DecodingKey::from_rsa_components("AQAB", "AQAB").unwrap(),
    );
    let cache = JwksCache::from_keys(keys);
    assert!(!cache.is_empty());

    // Force the snapshot's expires_at to a point already in the past.
    {
        let mut guard = cache.inner.write().unwrap();
        guard.expires_at = Some(Instant::now() - Duration::from_secs(1));
    }

    let cleared = cache.clear_if_expired(Instant::now());
    assert!(cleared);
    assert!(cache.is_empty());
    // A second call is a no-op because the keys are already empty.
    assert!(!cache.clear_if_expired(Instant::now()));
}

#[test]
fn clear_if_expired_is_noop_when_not_expired() {
    let mut keys = HashMap::new();
    keys.insert(
        "kid-1".to_string(),
        DecodingKey::from_rsa_components("AQAB", "AQAB").unwrap(),
    );
    let cache = JwksCache::from_keys(keys);
    // from_keys sets expires_at to now + DEFAULT_TTL, so not expired.
    assert!(!cache.clear_if_expired(Instant::now()));
    assert!(!cache.is_empty());
}

/// Regression guard for the `spawn_refresh_task` idempotency flag. The
/// internal `refresh_spawned: OnceLock<()>` must transition exactly once,
/// so that a second accidental call is recognised and short-circuited
/// before any `tokio::spawn` happens.
#[tokio::test]
async fn spawn_refresh_task_is_idempotent() {
    let cache = JwksCache::from_keys(HashMap::new());
    // First call takes the slot.
    assert!(cache.refresh_spawned.get().is_none());
    cache.spawn_refresh_task();
    assert!(cache.refresh_spawned.get().is_some());
    // Second call must short-circuit (no panic, no second spawn). We can
    // only verify the state didn't regress and that the call returns.
    cache.spawn_refresh_task();
    assert!(cache.refresh_spawned.get().is_some());
}
