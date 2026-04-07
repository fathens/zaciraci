use super::*;
use crate::jwks::JwksCache;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, encode};
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::rand_core::OsRng;
use rsa::traits::PublicKeyParts;
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

const TEST_CLIENT_ID: &str = "test-client-id.apps.googleusercontent.com";
const TEST_KID: &str = "test-key-1";

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock sanity")
        .as_secs()
}

struct TestKeypair {
    encoding: EncodingKey,
    decoding: DecodingKey,
}

/// 2048-bit RSA keypair used by all validator tests. Generating once and
/// sharing across tests avoids the ~1s cost per test of running key generation.
static SHARED_KEYPAIR: LazyLock<TestKeypair> = LazyLock::new(|| {
    // jsonwebtoken rejects keys smaller than 2048 bits.
    let private = RsaPrivateKey::new(&mut OsRng, 2048).expect("rsa gen");
    let public = RsaPublicKey::from(&private);

    let private_pem = private
        .to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
        .expect("private pem");

    // Build DecodingKey from modulus/exponent (same shape as the JWKS cache).
    let n = URL_SAFE_NO_PAD.encode(public.n().to_bytes_be());
    let e = URL_SAFE_NO_PAD.encode(public.e().to_bytes_be());
    let decoding = DecodingKey::from_rsa_components(&n, &e).expect("decoding key");

    let encoding = EncodingKey::from_rsa_pem(private_pem.as_bytes()).expect("encoding key");

    TestKeypair { encoding, decoding }
});

fn shared_keypair() -> &'static TestKeypair {
    &SHARED_KEYPAIR
}

fn build_jwks_with(kid: &str, decoding: DecodingKey) -> std::sync::Arc<JwksCache> {
    let mut keys = HashMap::new();
    keys.insert(kid.to_string(), decoding);
    JwksCache::from_keys(keys)
}

#[derive(Serialize)]
struct TestClaims<'a> {
    iss: &'a str,
    aud: &'a str,
    sub: &'a str,
    email: &'a str,
    email_verified: bool,
    iat: u64,
    exp: u64,
}

fn sign_token(
    keypair: &TestKeypair,
    kid: &str,
    iss: &str,
    aud: &str,
    email: &str,
    email_verified: bool,
    exp_delta_secs: i64,
) -> String {
    let mut header = Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some(kid.to_string());

    let iat = now_secs();
    let exp = if exp_delta_secs >= 0 {
        iat + exp_delta_secs as u64
    } else {
        iat.saturating_sub((-exp_delta_secs) as u64)
    };

    let claims = TestClaims {
        iss,
        aud,
        sub: "1234567890",
        email,
        email_verified,
        iat,
        exp,
    };
    encode(&header, &claims, &keypair.encoding).expect("encode")
}

#[test]
fn validate_happy_path() {
    let keypair = shared_keypair();
    let jwks = build_jwks_with(TEST_KID, keypair.decoding.clone());

    let token = sign_token(
        keypair,
        TEST_KID,
        "https://accounts.google.com",
        TEST_CLIENT_ID,
        "user@example.com",
        true,
        3600,
    );

    let claims = validate_id_token(&token, TEST_CLIENT_ID, &jwks).expect("valid token");
    assert_eq!(claims.email, "user@example.com");
    assert!(claims.email_verified);
}

#[test]
fn validate_accepts_short_form_issuer() {
    let keypair = shared_keypair();
    let jwks = build_jwks_with(TEST_KID, keypair.decoding.clone());

    let token = sign_token(
        keypair,
        TEST_KID,
        "accounts.google.com",
        TEST_CLIENT_ID,
        "user@example.com",
        true,
        3600,
    );

    validate_id_token(&token, TEST_CLIENT_ID, &jwks).expect("short issuer accepted");
}

#[test]
fn validate_rejects_wrong_issuer() {
    let keypair = shared_keypair();
    let jwks = build_jwks_with(TEST_KID, keypair.decoding.clone());

    let token = sign_token(
        keypair,
        TEST_KID,
        "https://evil.example.com",
        TEST_CLIENT_ID,
        "user@example.com",
        true,
        3600,
    );

    let err = validate_id_token(&token, TEST_CLIENT_ID, &jwks).expect_err("wrong iss");
    assert_eq!(err.kind(), "invalid_token");
}

#[test]
fn validate_rejects_wrong_audience() {
    let keypair = shared_keypair();
    let jwks = build_jwks_with(TEST_KID, keypair.decoding.clone());

    let token = sign_token(
        keypair,
        TEST_KID,
        "https://accounts.google.com",
        "different-audience.apps.googleusercontent.com",
        "user@example.com",
        true,
        3600,
    );

    let err = validate_id_token(&token, TEST_CLIENT_ID, &jwks).expect_err("wrong aud");
    assert_eq!(err.kind(), "invalid_token");
}

#[test]
fn validate_rejects_expired_token() {
    let keypair = shared_keypair();
    let jwks = build_jwks_with(TEST_KID, keypair.decoding.clone());

    // 5 minutes ago → well beyond the 60s leeway.
    let token = sign_token(
        keypair,
        TEST_KID,
        "https://accounts.google.com",
        TEST_CLIENT_ID,
        "user@example.com",
        true,
        -300,
    );

    let err = validate_id_token(&token, TEST_CLIENT_ID, &jwks).expect_err("expired");
    assert_eq!(err.kind(), "invalid_token");
}

#[test]
fn validate_rejects_email_not_verified() {
    let keypair = shared_keypair();
    let jwks = build_jwks_with(TEST_KID, keypair.decoding.clone());

    let token = sign_token(
        keypair,
        TEST_KID,
        "https://accounts.google.com",
        TEST_CLIENT_ID,
        "user@example.com",
        false,
        3600,
    );

    let err = validate_id_token(&token, TEST_CLIENT_ID, &jwks).expect_err("unverified");
    assert_eq!(err.kind(), "email_not_verified");
}

#[test]
fn validate_rejects_unknown_kid_when_cache_has_keys() {
    let keypair = shared_keypair();
    // Cache has a different kid than the token references.
    let jwks = build_jwks_with("other-kid", keypair.decoding.clone());

    let token = sign_token(
        keypair,
        TEST_KID,
        "https://accounts.google.com",
        TEST_CLIENT_ID,
        "user@example.com",
        true,
        3600,
    );

    let err = validate_id_token(&token, TEST_CLIENT_ID, &jwks).expect_err("unknown kid");
    assert_eq!(err.kind(), "invalid_token");
}

#[test]
fn validate_returns_jwks_unavailable_when_cache_empty() {
    let jwks = JwksCache::from_keys(HashMap::new());
    let keypair = shared_keypair();

    let token = sign_token(
        keypair,
        TEST_KID,
        "https://accounts.google.com",
        TEST_CLIENT_ID,
        "user@example.com",
        true,
        3600,
    );

    let err = validate_id_token(&token, TEST_CLIENT_ID, &jwks).expect_err("empty cache");
    assert_eq!(err.kind(), "jwks_unavailable");
}

#[test]
fn validate_rejects_empty_client_id() {
    let jwks = JwksCache::from_keys(HashMap::new());
    let err = validate_id_token("whatever", "", &jwks).expect_err("empty client_id");
    assert_eq!(err.kind(), "invalid_token");
}

#[test]
fn validate_rejects_rubbish_token() {
    let jwks = JwksCache::from_keys(HashMap::new());
    let err = validate_id_token("not-a-jwt", TEST_CLIENT_ID, &jwks).expect_err("rubbish");
    assert_eq!(err.kind(), "invalid_token");
}
