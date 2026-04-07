use thiserror::Error;
use tonic::Status;

/// Authentication / authorization errors.
///
/// Internal variants carry detail useful for server-side logging,
/// but the conversion to `tonic::Status` collapses them into a small
/// set of opaque external messages to avoid information leakage
/// (e.g., user enumeration via distinct error responses).
#[derive(Debug, Error)]
pub enum AuthError {
    #[error("missing authorization metadata")]
    MissingToken,

    #[error("invalid token: {0}")]
    InvalidToken(String),

    #[error("email is not verified")]
    EmailNotVerified,

    #[error("user is not registered")]
    UserNotRegistered,

    #[error("insufficient role")]
    InsufficientRole,

    #[error("auth provider unavailable")]
    JwksUnavailable,
}

impl AuthError {
    /// Stable identifier for logging (no PII).
    pub fn kind(&self) -> &'static str {
        match self {
            AuthError::MissingToken => "missing_token",
            AuthError::InvalidToken(_) => "invalid_token",
            AuthError::EmailNotVerified => "email_not_verified",
            AuthError::UserNotRegistered => "user_not_registered",
            AuthError::InsufficientRole => "insufficient_role",
            AuthError::JwksUnavailable => "jwks_unavailable",
        }
    }
}

impl From<AuthError> for Status {
    fn from(err: AuthError) -> Self {
        match err {
            AuthError::MissingToken
            | AuthError::InvalidToken(_)
            | AuthError::EmailNotVerified
            | AuthError::UserNotRegistered => Status::unauthenticated("authentication required"),
            AuthError::InsufficientRole => Status::permission_denied("insufficient permissions"),
            AuthError::JwksUnavailable => {
                Status::unavailable("auth service temporarily unavailable")
            }
        }
    }
}

#[cfg(test)]
mod tests;
