use std::sync::Arc;

use logging::{DEFAULT, info, o, warn};
use tonic::Status;
use tonic::service::Interceptor;

use crate::authenticator::Authenticator;
use crate::error::AuthError;

const AUTH_HEADER: &str = "authorization";
/// Length of the textual `Bearer` scheme name (excluding the trailing
/// separator). RFC 7235 §2.1 says auth scheme names are case-insensitive,
/// so we compare case-insensitively and strip by length.
const BEARER_SCHEME: &str = "Bearer";

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
            warn!(log, "auth_failure"; "reason" => err.kind(), "detail" => %err);
            Status::from(err)
        })?;

        match self.authenticator.authenticate(&token) {
            Ok(user) => {
                info!(
                    log,
                    "auth_success";
                    "email" => user.masked_email(),
                    "role" => %user.role(),
                );
                req.extensions_mut().insert(user);
                Ok(req)
            }
            Err(err) => {
                warn!(log, "auth_failure"; "reason" => err.kind(), "detail" => %err);
                Err(Status::from(err))
            }
        }
    }
}

fn extract_bearer_token(req: &tonic::Request<()>) -> Result<String, AuthError> {
    let value = req
        .metadata()
        .get(AUTH_HEADER)
        .ok_or(AuthError::MissingToken)?;

    let value_str = value
        .to_str()
        .map_err(|_| AuthError::InvalidToken("non-ascii authorization header".to_string()))?;

    // RFC 7235: scheme names are case-insensitive. Match "Bearer" in any
    // case followed by exactly one space.
    let scheme_len = BEARER_SCHEME.len();
    if value_str.len() <= scheme_len
        || !value_str
            .get(..scheme_len)
            .is_some_and(|s| s.eq_ignore_ascii_case(BEARER_SCHEME))
        || !value_str[scheme_len..].starts_with(' ')
    {
        return Err(AuthError::InvalidToken("missing Bearer prefix".to_string()));
    }
    let token = value_str[scheme_len + 1..].trim_start();

    if token.is_empty() {
        return Err(AuthError::InvalidToken("empty bearer token".to_string()));
    }

    Ok(token.to_string())
}

#[cfg(test)]
mod tests;
