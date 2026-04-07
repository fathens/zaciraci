use super::*;
use tonic::Code;

#[test]
fn missing_token_kind() {
    assert_eq!(AuthError::MissingToken.kind(), "missing_token");
}

#[test]
fn invalid_token_kind_does_not_leak_detail() {
    let err = AuthError::InvalidToken("expired".to_string());
    assert_eq!(err.kind(), "invalid_token");
}

#[test]
fn missing_token_maps_to_unauthenticated() {
    let status: Status = AuthError::MissingToken.into();
    assert_eq!(status.code(), Code::Unauthenticated);
    assert_eq!(status.message(), "authentication required");
}

#[test]
fn invalid_token_maps_to_unauthenticated_without_detail() {
    let status: Status = AuthError::InvalidToken("aud mismatch".to_string()).into();
    assert_eq!(status.code(), Code::Unauthenticated);
    assert_eq!(status.message(), "authentication required");
    // Detail must NOT leak to the wire.
    assert!(!status.message().contains("aud"));
}

#[test]
fn email_not_verified_maps_to_unauthenticated() {
    let status: Status = AuthError::EmailNotVerified.into();
    assert_eq!(status.code(), Code::Unauthenticated);
    assert_eq!(status.message(), "authentication required");
}

#[test]
fn user_not_registered_maps_to_unauthenticated() {
    // Same opaque message as InvalidToken to prevent enumeration.
    let status: Status = AuthError::UserNotRegistered.into();
    assert_eq!(status.code(), Code::Unauthenticated);
    assert_eq!(status.message(), "authentication required");
}

#[test]
fn insufficient_role_maps_to_permission_denied() {
    let status: Status = AuthError::InsufficientRole.into();
    assert_eq!(status.code(), Code::PermissionDenied);
    assert_eq!(status.message(), "insufficient permissions");
}

#[test]
fn jwks_unavailable_maps_to_unavailable() {
    let status: Status = AuthError::JwksUnavailable.into();
    assert_eq!(status.code(), Code::Unavailable);
}
