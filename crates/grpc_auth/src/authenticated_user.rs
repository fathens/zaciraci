use common::types::Role;

/// An authenticated principal extracted from a verified token.
///
/// Stored in `tonic::Request::extensions()` by the auth interceptor
/// so that downstream service handlers can perform role-based checks.
#[derive(Debug, Clone)]
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
}
