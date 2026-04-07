use super::*;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use common::types::Role;
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
    let users = UserCache::from_entries(vec![("alice@example.com".to_string(), Role::Writer)]);
    let auth = GoogleAuthenticator::new(TEST_CLIENT_ID.to_string(), jwks, users);

    let token = sign("alice@example.com", true);
    let user = auth.authenticate(&token).expect("should authenticate");
    assert_eq!(user.email, "alice@example.com");
    assert_eq!(user.role, Role::Writer);
}

#[test]
fn authenticate_rejects_unregistered_email() {
    let jwks = build_jwks();
    let users = UserCache::from_entries(vec![("alice@example.com".to_string(), Role::Reader)]);
    let auth = GoogleAuthenticator::new(TEST_CLIENT_ID.to_string(), jwks, users);

    let token = sign("stranger@example.com", true);
    let err = auth.authenticate(&token).expect_err("should reject");
    assert_eq!(err.kind(), "user_not_registered");
}

#[test]
fn authenticate_rejects_unverified_email() {
    let jwks = build_jwks();
    let users = UserCache::from_entries(vec![("alice@example.com".to_string(), Role::Reader)]);
    let auth = GoogleAuthenticator::new(TEST_CLIENT_ID.to_string(), jwks, users);

    let token = sign("alice@example.com", false);
    let err = auth.authenticate(&token).expect_err("should reject");
    assert_eq!(err.kind(), "email_not_verified");
}

#[test]
fn empty_client_id_rejects_every_token() {
    let jwks = build_jwks();
    let users = UserCache::from_entries(vec![("alice@example.com".to_string(), Role::Writer)]);
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
