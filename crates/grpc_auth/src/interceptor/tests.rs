use super::*;
use crate::authenticated_user::AuthenticatedUser;
use common::types::{Email, Role};
use tonic::Code;
use tonic::Request;
use tonic::metadata::MetadataValue;
use tonic::service::Interceptor;

/// Test authenticator that responds based on a closure.
struct StubAuthenticator<F>(F)
where
    F: Fn(&str) -> Result<AuthenticatedUser, AuthError> + Send + Sync + 'static;

impl<F> Authenticator for StubAuthenticator<F>
where
    F: Fn(&str) -> Result<AuthenticatedUser, AuthError> + Send + Sync + 'static,
{
    fn authenticate(&self, bearer_token: &str) -> Result<AuthenticatedUser, AuthError> {
        (self.0)(bearer_token)
    }
}

fn always_ok(
    email: &'static str,
    role: Role,
) -> Arc<
    StubAuthenticator<
        impl Fn(&str) -> Result<AuthenticatedUser, AuthError> + Send + Sync + 'static,
    >,
> {
    Arc::new(StubAuthenticator(move |_token: &str| {
        let parsed = Email::new(email).expect("test email is valid");
        Ok(AuthenticatedUser::new(parsed, role))
    }))
}

fn always_err() -> Arc<
    StubAuthenticator<
        impl Fn(&str) -> Result<AuthenticatedUser, AuthError> + Send + Sync + 'static,
    >,
> {
    Arc::new(StubAuthenticator(|_token: &str| {
        Err(AuthError::UserNotRegistered)
    }))
}

fn build_request(authorization: Option<&str>) -> Request<()> {
    let mut req = Request::new(());
    if let Some(value) = authorization {
        let metadata_value: MetadataValue<_> = value.parse().expect("valid metadata value");
        req.metadata_mut().insert("authorization", metadata_value);
    }
    req
}

#[test]
fn missing_authorization_returns_unauthenticated() {
    let mut interceptor = AuthInterceptor::new(always_ok("u@e.com", Role::Reader));

    let result = interceptor.call(build_request(None));
    let status = result.expect_err("should reject missing token");
    assert_eq!(status.code(), Code::Unauthenticated);
    assert_eq!(status.message(), "authentication required");
}

#[test]
fn missing_bearer_prefix_returns_unauthenticated() {
    let mut interceptor = AuthInterceptor::new(always_ok("u@e.com", Role::Reader));

    let result = interceptor.call(build_request(Some("tok")));
    let status = result.expect_err("should reject missing prefix");
    assert_eq!(status.code(), Code::Unauthenticated);
}

#[test]
fn empty_bearer_token_returns_unauthenticated() {
    let mut interceptor = AuthInterceptor::new(always_ok("u@e.com", Role::Reader));

    let result = interceptor.call(build_request(Some("Bearer ")));
    let status = result.expect_err("should reject empty token");
    assert_eq!(status.code(), Code::Unauthenticated);
}

#[test]
fn valid_token_injects_authenticated_user_into_extensions() {
    let mut interceptor = AuthInterceptor::new(always_ok("alice@example.com", Role::Writer));

    let req = interceptor
        .call(build_request(Some("Bearer secret")))
        .expect("should succeed");

    let user = req
        .extensions()
        .get::<AuthenticatedUser>()
        .expect("AuthenticatedUser should be inserted");
    assert_eq!(user.email().as_str(), "alice@example.com");
    assert_eq!(user.role(), Role::Writer);
    assert!(user.can_write());
}

#[test]
fn authenticator_error_returns_unauthenticated_without_detail() {
    let mut interceptor = AuthInterceptor::new(always_err());

    let result = interceptor.call(build_request(Some("Bearer wrong-token")));
    let status = result.expect_err("should reject");
    assert_eq!(status.code(), Code::Unauthenticated);
    assert_eq!(status.message(), "authentication required");
}

#[test]
fn extract_bearer_token_strips_prefix() {
    let req = build_request(Some("Bearer abc123"));
    let token = extract_bearer_token(&req).expect("should extract");
    assert_eq!(token, "abc123");
}

#[test]
fn extract_bearer_token_rejects_non_bearer_scheme() {
    let req = build_request(Some("Basic dXNlcjpwYXNz"));
    let err = extract_bearer_token(&req).expect_err("should reject non-Bearer");
    assert_eq!(err.kind(), "invalid_token");
}

#[test]
fn extract_bearer_token_is_case_insensitive_per_rfc7235() {
    // RFC 7235 §2.1: auth scheme names are case-insensitive.
    for prefix in ["bearer abc", "BEARER abc", "BeArEr abc"] {
        let req = build_request(Some(prefix));
        let token =
            extract_bearer_token(&req).unwrap_or_else(|_| panic!("should extract from {prefix}"));
        assert_eq!(token, "abc");
    }
}

#[test]
fn extract_bearer_token_rejects_bearer_without_space() {
    // "Beareralice" looks like Bearer but has no separator; must not be
    // accepted as a valid scheme.
    let req = build_request(Some("Beareralice"));
    let err = extract_bearer_token(&req).expect_err("should reject");
    assert_eq!(err.kind(), "invalid_token");
}
