use std::fmt;

use common::types::{Email, Role};

/// An authenticated principal extracted from a verified token.
///
/// Stored in `tonic::Request::extensions()` by the auth interceptor
/// so that downstream service handlers can perform role-based checks.
///
/// `Debug` is implemented manually to mask the email address (PII) in
/// logs and panics. The fields are private so the only way to read the
/// raw email is via [`AuthenticatedUser::email`], which makes accidental
/// PII leakage through `format!("{:?}", ...)` or `slog`'s `%` formatter
/// structurally impossible.
#[derive(Clone)]
pub struct AuthenticatedUser {
    email: Email,
    role: Role,
}

impl AuthenticatedUser {
    pub fn new(email: Email, role: Role) -> Self {
        Self { email, role }
    }

    /// Returns the verified email address.
    ///
    /// Prefer [`AuthenticatedUser::masked_email`] for log output. The raw
    /// value should only be used when the email is part of an
    /// authorization decision (e.g., DB lookups for the user's role).
    pub fn email(&self) -> &Email {
        &self.email
    }

    /// Returns the user's role.
    pub fn role(&self) -> Role {
        self.role
    }

    /// Returns true if the user has writer privileges.
    pub fn can_write(&self) -> bool {
        self.role.can_write()
    }

    /// Return the email with the local part masked, suitable for logging.
    pub fn masked_email(&self) -> String {
        self.email.masked()
    }
}

impl fmt::Debug for AuthenticatedUser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthenticatedUser")
            .field("email", &self.masked_email())
            .field("role", &self.role)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn email(s: &str) -> Email {
        Email::new(s).expect("test email is valid")
    }

    #[test]
    fn masks_local_part() {
        let u = AuthenticatedUser::new(email("alice@example.com"), Role::Reader);
        assert_eq!(u.masked_email(), "a***@example.com");
    }

    #[test]
    fn debug_does_not_leak_full_email() {
        let u = AuthenticatedUser::new(email("alice@example.com"), Role::Writer);
        let rendered = format!("{u:?}");
        assert!(!rendered.contains("alice@example.com"));
        assert!(rendered.contains("a***@example.com"));
    }
}
