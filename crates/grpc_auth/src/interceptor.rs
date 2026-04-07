use std::sync::Arc;

use logging::{DEFAULT, info, o, warn};
use tonic::Status;
use tonic::service::Interceptor;

use crate::authenticator::Authenticator;
use crate::error::AuthError;

const AUTH_HEADER: &str = "authorization";
const BEARER_PREFIX: &str = "Bearer ";

/// A clonable, sync tonic interceptor that delegates to an `Authenticator`.
///
/// On success, the verified `AuthenticatedUser` is inserted into the
/// request's `Extensions` so that downstream service handlers can read it.
pub struct AuthInterceptor<A: Authenticator> {
    authenticator: Arc<A>,
}

impl<A: Authenticator> AuthInterceptor<A> {
    pub fn new(authenticator: Arc<A>) -> Self {
        Self { authenticator }
    }
}

// Manual Clone impl so that `A: Clone` is NOT required; only the Arc is cloned.
impl<A: Authenticator> Clone for AuthInterceptor<A> {
    fn clone(&self) -> Self {
        Self {
            authenticator: Arc::clone(&self.authenticator),
        }
    }
}

impl<A: Authenticator> Interceptor for AuthInterceptor<A> {
    fn call(&mut self, mut req: tonic::Request<()>) -> Result<tonic::Request<()>, Status> {
        let log = DEFAULT.new(o!("module" => "grpc_auth", "fn" => "auth_interceptor"));

        let token = extract_bearer_token(&req).map_err(|err| {
            warn!(log, "auth_failure"; "reason" => err.kind());
            Status::from(err)
        })?;

        match self.authenticator.authenticate(&token) {
            Ok(user) => {
                info!(log, "auth_success"; "email" => &user.email, "role" => %user.role);
                req.extensions_mut().insert(user);
                Ok(req)
            }
            Err(err) => {
                warn!(log, "auth_failure"; "reason" => err.kind());
                Err(Status::from(err))
            }
        }
    }
}

/// Convenience constructor for callers that want a plain interceptor handle.
pub fn make_interceptor<A: Authenticator>(authenticator: Arc<A>) -> AuthInterceptor<A> {
    AuthInterceptor::new(authenticator)
}

fn extract_bearer_token(req: &tonic::Request<()>) -> Result<String, AuthError> {
    let value = req
        .metadata()
        .get(AUTH_HEADER)
        .ok_or(AuthError::MissingToken)?;

    let value_str = value
        .to_str()
        .map_err(|_| AuthError::InvalidToken("non-ascii authorization header".to_string()))?;

    let token = value_str
        .strip_prefix(BEARER_PREFIX)
        .ok_or_else(|| AuthError::InvalidToken("missing Bearer prefix".to_string()))?;

    if token.is_empty() {
        return Err(AuthError::InvalidToken("empty bearer token".to_string()));
    }

    Ok(token.to_string())
}

#[cfg(test)]
mod tests;
