use crate::authenticated_user::AuthenticatedUser;
use crate::error::AuthError;

/// Synchronous authenticator interface used by the tonic interceptor.
///
/// Implementations are expected to perform validation against in-memory
/// caches that are populated and refreshed asynchronously elsewhere.
/// `authenticate` itself must not perform any blocking I/O because it
/// runs inside the synchronous tonic interceptor function.
pub trait Authenticator: Send + Sync + 'static {
    fn authenticate(&self, bearer_token: &str) -> Result<AuthenticatedUser, AuthError>;
}
