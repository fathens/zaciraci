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
    #[serde(skip_serializing_if = "Option::is_none")]
    nbf: Option<u64>,
}

/// Test claim set without `iat`, used to verify the required-spec-claim check.
#[derive(Serialize)]
struct TestClaimsNoIat<'a> {
    iss: &'a str,
    aud: &'a str,
    sub: &'a str,
    email: &'a str,
    email_verified: bool,
    exp: u64,
}

/// Builder describing how to construct a JWT for tests.
struct TokenBuilder<'a> {
    kid: &'a str,
    iss: &'a str,
    aud: &'a str,
    email: &'a str,
    email_verified: bool,
    /// Offset (positive = future, negative = past) applied to `now_secs()`
    /// to compute `exp`.
    exp_delta_secs: i64,
    /// Offset applied to `now_secs()` to compute `iat`. Negative means
    /// the token was issued that many seconds ago.
    iat_offset_secs: i64,
    /// Optional `nbf` offset relative to `now_secs()`. `None` omits the
    /// claim entirely.
    nbf_offset_secs: Option<i64>,
}

impl<'a> TokenBuilder<'a> {
    fn new(email: &'a str) -> Self {
        Self {
            kid: TEST_KID,
            iss: "https://accounts.google.com",
            aud: TEST_CLIENT_ID,
            email,
            email_verified: true,
            exp_delta_secs: 3600,
            iat_offset_secs: 0,
            nbf_offset_secs: None,
        }
    }

    fn iss(mut self, iss: &'a str) -> Self {
        self.iss = iss;
        self
    }

    fn aud(mut self, aud: &'a str) -> Self {
        self.aud = aud;
        self
    }

    fn email_verified(mut self, verified: bool) -> Self {
        self.email_verified = verified;
        self
    }

    fn exp_delta(mut self, secs: i64) -> Self {
        self.exp_delta_secs = secs;
        self
    }

    fn iat_offset(mut self, secs: i64) -> Self {
        self.iat_offset_secs = secs;
        self
    }

    fn nbf_offset(mut self, secs: i64) -> Self {
        self.nbf_offset_secs = Some(secs);
        self
    }

    fn sign(self, keypair: &TestKeypair) -> String {
        let mut header = Header::new(jsonwebtoken::Algorithm::RS256);
        header.kid = Some(self.kid.to_string());

        let now = now_secs();
        let iat = apply_offset(now, self.iat_offset_secs);
        let exp = apply_offset(now, self.exp_delta_secs);
        let nbf = self.nbf_offset_secs.map(|secs| apply_offset(now, secs));

        let claims = TestClaims {
            iss: self.iss,
            aud: self.aud,
            sub: "1234567890",
            email: self.email,
            email_verified: self.email_verified,
            iat,
            exp,
            nbf,
        };
        encode(&header, &claims, &keypair.encoding).expect("encode")
    }
}

fn apply_offset(base: u64, offset: i64) -> u64 {
    if offset >= 0 {
        base + offset as u64
    } else {
        base.saturating_sub((-offset) as u64)
    }
}

/// Backward-compatible helper used by the existing tests; constructs a token
/// via `TokenBuilder` so the surface stays minimal while supporting the new
/// fields.
fn sign_token(
    keypair: &TestKeypair,
    kid: &str,
    iss: &str,
    aud: &str,
    email: &str,
    email_verified: bool,
    exp_delta_secs: i64,
) -> String {
    let mut builder = TokenBuilder::new(email)
        .iss(iss)
        .aud(aud)
        .email_verified(email_verified)
        .exp_delta(exp_delta_secs);
    builder.kid = kid;
    builder.sign(keypair)
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

/// Regression guard for the "auth disabled" fail-closed contract: even a
/// fully-valid, correctly-signed ID token must be rejected when `client_id`
/// is empty. This locks in the behaviour documented on
/// `GoogleAuthenticator::new` so that any future refactor which accidentally
/// makes an empty `client_id` accept tokens fails loudly in tests.
#[test]
fn validate_rejects_empty_client_id_even_with_valid_token() {
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

    let err = validate_id_token(&token, "", &jwks).expect_err("empty client_id fail-closed");
    assert_eq!(err.kind(), "invalid_token");
}

#[test]
fn validate_rejects_rubbish_token() {
    let jwks = JwksCache::from_keys(HashMap::new());
    let err = validate_id_token("not-a-jwt", TEST_CLIENT_ID, &jwks).expect_err("rubbish");
    assert_eq!(err.kind(), "invalid_token");
}

#[test]
fn validate_rejects_future_nbf() {
    let keypair = shared_keypair();
    let jwks = build_jwks_with(TEST_KID, keypair.decoding.clone());

    // nbf 10 minutes into the future, well beyond the 60s leeway.
    let token = TokenBuilder::new("user@example.com")
        .nbf_offset(600)
        .sign(keypair);

    let err = validate_id_token(&token, TEST_CLIENT_ID, &jwks).expect_err("future nbf");
    assert_eq!(err.kind(), "invalid_token");
}

#[test]
fn validate_rejects_token_too_old() {
    let keypair = shared_keypair();
    let jwks = build_jwks_with(TEST_KID, keypair.decoding.clone());

    // iat 2 hours ago, but exp still in the future so jsonwebtoken's exp
    // check passes and the manual max-age check is what kicks in. With
    // MAX_TOKEN_AGE_SECONDS tightened to 1 hour, this must be rejected.
    let token = TokenBuilder::new("user@example.com")
        .iat_offset(-(2 * 60 * 60))
        .exp_delta(3600)
        .sign(keypair);

    let err = validate_id_token(&token, TEST_CLIENT_ID, &jwks).expect_err("too old");
    assert_eq!(err.kind(), "invalid_token");
}

#[test]
fn validate_accepts_token_within_age_limit() {
    let keypair = shared_keypair();
    let jwks = build_jwks_with(TEST_KID, keypair.decoding.clone());

    // iat 30 minutes ago, exp still in the future. Within the 1 hour limit.
    let token = TokenBuilder::new("user@example.com")
        .iat_offset(-(30 * 60))
        .exp_delta(3600)
        .sign(keypair);

    let claims = validate_id_token(&token, TEST_CLIENT_ID, &jwks).expect("within age");
    assert_eq!(claims.email, "user@example.com");
}

#[test]
fn validate_rejects_missing_iat() {
    let keypair = shared_keypair();
    let jwks = build_jwks_with(TEST_KID, keypair.decoding.clone());

    // Hand-craft a JWT whose payload omits `iat` entirely.
    let mut header = Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some(TEST_KID.to_string());
    let now = now_secs();
    let claims = TestClaimsNoIat {
        iss: "https://accounts.google.com",
        aud: TEST_CLIENT_ID,
        sub: "1234567890",
        email: "user@example.com",
        email_verified: true,
        exp: now + 3600,
    };
    let token = encode(&header, &claims, &keypair.encoding).expect("encode");

    let err = validate_id_token(&token, TEST_CLIENT_ID, &jwks).expect_err("missing iat");
    assert_eq!(err.kind(), "invalid_token");
}
