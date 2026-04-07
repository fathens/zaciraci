use std::sync::Arc;

use grpc_auth::{AuthError, AuthenticatedUser, Authenticator};
use logging::{DEFAULT, o, warn};

use crate::jwks::JwksCache;
use crate::user_cache::UserCache;
use crate::validator;

/// `grpc_auth::Authenticator` implementation that verifies Google-issued
/// ID tokens and resolves the caller against an in-memory user cache.
pub struct GoogleAuthenticator {
    client_id: String,
    jwks: Arc<JwksCache>,
    users: Arc<UserCache>,
}

impl GoogleAuthenticator {
    /// Build an authenticator from its dependencies.
    ///
    /// `client_id` is the Google OAuth2 client id used for the `aud` check.
    /// If empty, every `authenticate` call will return `AuthError::InvalidToken`;
    /// a warning is emitted here to make the misconfiguration obvious in logs.
    pub fn new(client_id: String, jwks: Arc<JwksCache>, users: Arc<UserCache>) -> Self {
        if client_id.is_empty() {
            let log = DEFAULT.new(o!("module" => "google_auth"));
            warn!(
                log,
                "google_client_id_not_configured";
                "effect" => "authenticated endpoints will reject every request"
            );
        }
        Self {
            client_id,
            jwks,
            users,
        }
    }

    /// Bootstrap the authenticator by constructing a JWKS cache (attempts an
    /// initial fetch, fail-open on error), loading users from the database,
    /// and wiring everything together.
    pub async fn bootstrap(client_id: String) -> anyhow::Result<Self> {
        let jwks = JwksCache::new_google().await;
        jwks.spawn_refresh_task();
        let users = UserCache::load_from_db().await?;
        Ok(Self::new(client_id, jwks, users))
    }

    /// Access the user cache so callers can reload it after user-management
    /// operations (e.g., inside an `AuthorizedUserService` RPC handler).
    pub fn user_cache(&self) -> Arc<UserCache> {
        Arc::clone(&self.users)
    }
}

impl Authenticator for GoogleAuthenticator {
    fn authenticate(&self, bearer_token: &str) -> Result<AuthenticatedUser, AuthError> {
        let claims = validator::validate_id_token(bearer_token, &self.client_id, &self.jwks)?;

        let role = self
            .users
            .lookup(&claims.email)
            .ok_or(AuthError::UserNotRegistered)?;

        Ok(AuthenticatedUser::new(claims.email, role))
    }
}

#[cfg(test)]
mod tests;
