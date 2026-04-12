//! Shared authorization helpers used by the authenticated gRPC services.
//!
//! The [`grpc_auth::AuthInterceptor`] inserts an [`AuthenticatedUser`] into
//! the request extensions on success. These helpers extract it and apply
//! role-based gating so every service goes through the same code path and
//! the symmetry between reader- and writer-only RPCs is visible in the
//! service source.

use grpc_auth::{AuthError, AuthenticatedUser};
use tonic::{Request, Status};

/// Enforce that the caller is an authenticated user, returning a borrow of
/// the verified [`AuthenticatedUser`] on success.
///
/// Used by read-only services (e.g. `PortfolioService`). The interceptor
/// already guarantees this for any RPC wired through it, but routing the
/// check through an explicit helper keeps the intent visible at the
/// handler site and prevents a future refactor from accidentally
/// exposing a handler without wrapping it in `InterceptedService`.
///
/// `#[must_use]` guarantees that a `let _ = require_reader(...)` refactor
/// trips a compiler warning (which CI treats as an error via
/// `#![deny(warnings)]`), so the authorization check cannot be silently
/// dropped. The returned borrow lets the handler read `email()` /
/// `role()` without a second `extensions().get::<AuthenticatedUser>()`
/// lookup.
#[must_use = "the authorization result must be checked with `?`"]
pub(crate) fn require_reader<T>(request: &Request<T>) -> Result<&AuthenticatedUser, Status> {
    request
        .extensions()
        .get::<AuthenticatedUser>()
        .ok_or_else(|| -> Status { AuthError::MissingToken.into() })
}

/// Enforce that the caller is an authenticated user with writer privileges,
/// returning a borrow of the verified [`AuthenticatedUser`] on success.
///
/// Missing extension implies the request reached the handler without
/// passing the interceptor (e.g., tests that bypass it), so we treat it as
/// unauthenticated rather than silently granting access.
#[must_use = "the authorization result must be checked with `?`"]
pub(crate) fn require_writer<T>(request: &Request<T>) -> Result<&AuthenticatedUser, Status> {
    let user = request
        .extensions()
        .get::<AuthenticatedUser>()
        .ok_or_else(|| -> Status { AuthError::MissingToken.into() })?;
    if !user.can_write() {
        return Err(AuthError::InsufficientRole.into());
    }
    Ok(user)
}
