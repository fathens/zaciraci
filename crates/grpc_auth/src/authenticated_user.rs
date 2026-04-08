use std::fmt;

use common::types::Role;

/// An authenticated principal extracted from a verified token.
///
/// Stored in `tonic::Request::extensions()` by the auth interceptor
/// so that downstream service handlers can perform role-based checks.
///
/// `Debug` is implemented manually to mask the email address (PII) in
/// logs and panics. Use [`AuthenticatedUser::email`] explicitly when the
/// full value is actually needed.
#[derive(Clone)]
pub struct AuthenticatedUser {
    pub email: String,
    pub role: Role,
}

impl AuthenticatedUser {
    pub fn new(email: String, role: Role) -> Self {
        Self { email, role }
    }

    /// Returns true if the user has writer privileges.
    pub fn can_write(&self) -> bool {
        self.role.can_write()
    }

    /// Return the email with the local part masked, suitable for logging.
    ///
    /// Examples: `alice@example.com` → `a***@example.com`, `a@b` → `*@b`,
    /// a value without `@` → `***`.
    pub fn masked_email(&self) -> String {
        mask_email(&self.email)
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

fn mask_email(email: &str) -> String {
    match email.split_once('@') {
        Some((local, domain)) if !local.is_empty() => {
            let first = local.chars().next().unwrap();
            format!("{first}***@{domain}")
        }
        Some((_, domain)) => format!("*@{domain}"),
        None => "***".to_string(),
    }
}

#[cfg(test)]
mod authenticated_user_tests {
    use super::*;

    #[test]
    fn masks_local_part() {
        let u = AuthenticatedUser::new("alice@example.com".to_string(), Role::Reader);
        assert_eq!(u.masked_email(), "a***@example.com");
    }

    #[test]
    fn masks_empty_local_part() {
        let u = AuthenticatedUser::new("@example.com".to_string(), Role::Reader);
        assert_eq!(u.masked_email(), "*@example.com");
    }

    #[test]
    fn masks_missing_at_sign() {
        let u = AuthenticatedUser::new("not-an-email".to_string(), Role::Reader);
        assert_eq!(u.masked_email(), "***");
    }

    #[test]
    fn debug_does_not_leak_full_email() {
        let u = AuthenticatedUser::new("alice@example.com".to_string(), Role::Writer);
        let rendered = format!("{u:?}");
        assert!(!rendered.contains("alice@example.com"));
        assert!(rendered.contains("a***@example.com"));
    }
}
