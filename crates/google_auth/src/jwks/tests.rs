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
