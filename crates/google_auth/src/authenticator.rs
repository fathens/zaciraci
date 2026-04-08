use std::sync::Arc;
use std::time::Duration;

use grpc_auth::{AuthError, AuthenticatedUser, Authenticator};
use logging::{DEFAULT, o, warn};

use crate::jwks::JwksCache;
use crate::user_cache::UserCache;
use crate::validator;

/// Number of attempts to load the user cache from the database at startup.
const USER_CACHE_BOOTSTRAP_ATTEMPTS: u32 = 5;
/// Initial delay between user-cache bootstrap retries (doubles each attempt,
/// capped at [`USER_CACHE_BOOTSTRAP_MAX_DELAY`]).
const USER_CACHE_BOOTSTRAP_INITIAL_DELAY: Duration = Duration::from_secs(2);
const USER_CACHE_BOOTSTRAP_MAX_DELAY: Duration = Duration::from_secs(30);

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
    /// initial fetch, fail-open on error), loading users from the database
    /// with bounded retries, and wiring everything together.
    ///
    /// JWKS failures are non-fatal (the background refresh task will keep
    /// trying). User-cache failures are retried
    /// [`USER_CACHE_BOOTSTRAP_ATTEMPTS`] times with exponential backoff before
    /// the error is returned to the caller, which gives a briefly-unavailable
    /// database enough time to come back before we abort startup.
    pub async fn bootstrap(client_id: String) -> anyhow::Result<Self> {
        let jwks = JwksCache::new_google().await;
        jwks.spawn_refresh_task();
        let users = load_user_cache_with_retry().await?;
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

async fn load_user_cache_with_retry() -> anyhow::Result<Arc<UserCache>> {
    let log = DEFAULT.new(o!("module" => "google_auth::bootstrap"));
    let mut delay = USER_CACHE_BOOTSTRAP_INITIAL_DELAY;
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 1..=USER_CACHE_BOOTSTRAP_ATTEMPTS {
        match UserCache::load_from_db().await {
            Ok(cache) => return Ok(cache),
            Err(err) => {
                warn!(
                    log,
                    "user_cache_load_failed";
                    "attempt" => attempt,
                    "max_attempts" => USER_CACHE_BOOTSTRAP_ATTEMPTS,
                    "retry_in_secs" => delay.as_secs(),
                    "error" => %err,
                );
                last_err = Some(err);
                if attempt < USER_CACHE_BOOTSTRAP_ATTEMPTS {
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(USER_CACHE_BOOTSTRAP_MAX_DELAY);
                }
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("user cache bootstrap failed")))
}

#[cfg(test)]
mod tests;
