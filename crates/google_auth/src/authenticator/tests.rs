use super::*;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use common::types::{Email, Role};
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

struct TestKeypair {
    encoding: EncodingKey,
    decoding: DecodingKey,
}

static SHARED_KEYPAIR: LazyLock<TestKeypair> = LazyLock::new(|| {
    let private = RsaPrivateKey::new(&mut OsRng, 2048).expect("rsa gen");
    let public = RsaPublicKey::from(&private);

    let private_pem = private
        .to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
        .expect("private pem");

    let n = URL_SAFE_NO_PAD.encode(public.n().to_bytes_be());
    let e = URL_SAFE_NO_PAD.encode(public.e().to_bytes_be());
    let decoding = DecodingKey::from_rsa_components(&n, &e).expect("decoding key");
    let encoding = EncodingKey::from_rsa_pem(private_pem.as_bytes()).expect("encoding key");

    TestKeypair { encoding, decoding }
});

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock sanity")
        .as_secs()
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

fn sign(email: &str, verified: bool) -> String {
    let mut header = Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some(TEST_KID.to_string());
    let iat = now_secs();
    let claims = TestClaims {
        iss: "https://accounts.google.com",
        aud: TEST_CLIENT_ID,
        sub: "1234",
        email,
        email_verified: verified,
        iat,
        exp: iat + 3600,
    };
    encode(&header, &claims, &SHARED_KEYPAIR.encoding).expect("encode")
}

fn build_jwks() -> Arc<JwksCache> {
    let mut keys = HashMap::new();
    keys.insert(TEST_KID.to_string(), SHARED_KEYPAIR.decoding.clone());
    JwksCache::from_keys(keys)
}

#[test]
fn authenticate_returns_registered_user() {
    let jwks = build_jwks();
    let users = UserCache::from_entries(vec![(
        Email::new("alice@example.com").unwrap(),
        Role::Writer,
    )]);
    let auth = GoogleAuthenticator::new(TEST_CLIENT_ID.to_string(), jwks, users);

    let token = sign("alice@example.com", true);
    let user = auth.authenticate(&token).expect("should authenticate");
    assert_eq!(user.email().as_str(), "alice@example.com");
    assert_eq!(user.role(), Role::Writer);
}

#[test]
fn authenticate_rejects_unregistered_email() {
    let jwks = build_jwks();
    let users = UserCache::from_entries(vec![(
        Email::new("alice@example.com").unwrap(),
        Role::Reader,
    )]);
    let auth = GoogleAuthenticator::new(TEST_CLIENT_ID.to_string(), jwks, users);

    let token = sign("stranger@example.com", true);
    let err = auth.authenticate(&token).expect_err("should reject");
    assert_eq!(err.kind(), "user_not_registered");
}

#[test]
fn authenticate_rejects_unverified_email() {
    let jwks = build_jwks();
    let users = UserCache::from_entries(vec![(
        Email::new("alice@example.com").unwrap(),
        Role::Reader,
    )]);
    let auth = GoogleAuthenticator::new(TEST_CLIENT_ID.to_string(), jwks, users);

    let token = sign("alice@example.com", false);
    let err = auth.authenticate(&token).expect_err("should reject");
    assert_eq!(err.kind(), "email_not_verified");
}

#[test]
fn empty_client_id_rejects_every_token() {
    let jwks = build_jwks();
    let users = UserCache::from_entries(vec![(
        Email::new("alice@example.com").unwrap(),
        Role::Writer,
    )]);
    let auth = GoogleAuthenticator::new(String::new(), jwks, users);

    let token = sign("alice@example.com", true);
    let err = auth.authenticate(&token).expect_err("empty client_id");
    assert_eq!(err.kind(), "invalid_token");
}

#[test]
fn empty_token_returns_invalid_token() {
    let jwks = build_jwks();
    let users = UserCache::empty();
    let auth = GoogleAuthenticator::new(TEST_CLIENT_ID.to_string(), jwks, users);

    let err = auth.authenticate("").expect_err("empty token");
    assert_eq!(err.kind(), "invalid_token");
}

// ---- End-to-end interceptor + authenticator integration tests ----
//
// These ensure that AuthInterceptor and GoogleAuthenticator compose the
// way web::serve wires them: a signed ID token flows through the
// interceptor, the interceptor calls authenticate(), and the resulting
// AuthenticatedUser is inserted into the request extensions. A bad token
// must produce a Status::unauthenticated whose message does not leak
// internal detail.

use grpc_auth::{AuthInterceptor, AuthenticatedUser as GrpcAuthenticatedUser};
use tonic::metadata::MetadataValue;
use tonic::service::Interceptor;
use tonic::{Code, Request};

fn build_request_with_bearer(token: &str) -> Request<()> {
    let mut req = Request::new(());
    let header = format!("Bearer {token}");
    let value: MetadataValue<_> = header.parse().expect("valid metadata");
    req.metadata_mut().insert("authorization", value);
    req
}

#[test]
fn interceptor_passes_through_valid_google_token() {
    let jwks = build_jwks();
    let users = UserCache::from_entries(vec![(
        Email::new("alice@example.com").unwrap(),
        Role::Writer,
    )]);
    let auth = GoogleAuthenticator::new(TEST_CLIENT_ID.to_string(), jwks, users);
    let mut interceptor = AuthInterceptor::new(Arc::new(auth));

    let token = sign("alice@example.com", true);
    let req = interceptor
        .call(build_request_with_bearer(&token))
        .expect("should pass interceptor");

    let user = req
        .extensions()
        .get::<GrpcAuthenticatedUser>()
        .expect("AuthenticatedUser in extensions");
    assert_eq!(user.email().as_str(), "alice@example.com");
    assert_eq!(user.role(), Role::Writer);
}

#[test]
fn interceptor_rejects_invalid_signature_without_detail() {
    let jwks = build_jwks();
    let users = UserCache::from_entries(vec![(
        Email::new("alice@example.com").unwrap(),
        Role::Reader,
    )]);
    let auth = GoogleAuthenticator::new(TEST_CLIENT_ID.to_string(), jwks, users);
    let mut interceptor = AuthInterceptor::new(Arc::new(auth));

    // Corrupt the signature by flipping the final character.
    let mut token = sign("alice@example.com", true);
    let last = token.pop().unwrap();
    let replacement = if last == 'A' { 'B' } else { 'A' };
    token.push(replacement);

    let status = interceptor
        .call(build_request_with_bearer(&token))
        .expect_err("bad signature should be rejected");
    assert_eq!(status.code(), Code::Unauthenticated);
    // No internal detail (jsonwebtoken error message) must leak into the
    // wire-level Status.
    assert_eq!(status.message(), "authentication required");
}

#[test]
fn interceptor_rejects_unregistered_user_without_enumeration() {
    let jwks = build_jwks();
    let users = UserCache::from_entries(vec![(
        Email::new("alice@example.com").unwrap(),
        Role::Reader,
    )]);
    let auth = GoogleAuthenticator::new(TEST_CLIENT_ID.to_string(), jwks, users);
    let mut interceptor = AuthInterceptor::new(Arc::new(auth));

    let token = sign("stranger@example.com", true);
    let status = interceptor
        .call(build_request_with_bearer(&token))
        .expect_err("unregistered should be rejected");
    assert_eq!(status.code(), Code::Unauthenticated);
    // Must not reveal whether the user is known or the signature was bad.
    assert_eq!(status.message(), "authentication required");
}
