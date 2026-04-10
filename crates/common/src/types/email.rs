use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

/// A normalized, structurally-validated email address.
///
/// Construction enforces:
/// - non-empty after trimming
/// - exactly one `@` separator with non-empty local and domain parts
/// - no ASCII control characters or whitespace inside the value
/// - ASCII-lowercase + trim normalization, applied unconditionally
///
/// Once an `Email` exists, the wrapped string is the canonical key used for
/// DB lookups, in-memory caches, and authorization decisions. This makes
/// case-insensitive de-duplication a property of the type system rather than
/// of every call site, and prevents the kind of normalization drift that
/// would otherwise let `Alice@x.com` and `alice@x.com` resolve to different
/// principals.
///
/// `Display` renders the **masked** form so that a stray `{}` interpolation
/// in logs cannot leak PII. Use [`Email::as_str`] only when the authoritative
/// value is required (e.g., DB query parameters).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct Email(String);

impl Email {
    /// Parse and normalize an email address.
    pub fn new(input: &str) -> Result<Self, ParseEmailError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(ParseEmailError::Empty);
        }
        if trimmed.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return Err(ParseEmailError::InvalidCharacter);
        }
        let (local, domain) = trimmed
            .split_once('@')
            .ok_or(ParseEmailError::MissingAtSign)?;
        if local.is_empty() || domain.is_empty() {
            return Err(ParseEmailError::EmptyPart);
        }
        if domain.contains('@') {
            return Err(ParseEmailError::MultipleAtSigns);
        }
        Ok(Self(trimmed.to_ascii_lowercase()))
    }

    /// Return the canonical (normalized) form. Use only when the raw value
    /// is required by an external system; prefer [`Email::masked`] for logs.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Return the email with the local part masked, suitable for logging.
    ///
    /// Examples: `alice@example.com` → `a***@example.com`,
    /// `a@b` → `a***@b`. Because construction guarantees a non-empty local
    /// part and exactly one `@`, this is total.
    pub fn masked(&self) -> String {
        // Safe: constructor guarantees one `@` with non-empty local part.
        let Some((local, domain)) = self.0.split_once('@') else {
            unreachable!("Email invariant: contains @");
        };
        let Some(first) = local.chars().next() else {
            unreachable!("Email invariant: non-empty local");
        };
        format!("{first}***@{domain}")
    }
}

impl AsRef<str> for Email {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<Email> for String {
    fn from(email: Email) -> Self {
        email.0
    }
}

impl FromStr for Email {
    type Err = ParseEmailError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl<'de> Deserialize<'de> for Email {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::new(&raw).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ParseEmailError {
    #[error("email is empty")]
    Empty,
    #[error("email contains whitespace or control characters")]
    InvalidCharacter,
    #[error("email is missing the '@' separator")]
    MissingAtSign,
    #[error("email contains multiple '@' separators")]
    MultipleAtSigns,
    #[error("email local or domain part is empty")]
    EmptyPart,
}

impl fmt::Display for Email {
    /// **Renders the masked form, not the canonical value.** This is a
    /// deliberate, safety-first choice: any accidental `format!("{email}")`,
    /// `{email}` interpolation, `slog`'s `%email` argument, or error-message
    /// concatenation can never leak PII, because there is no code path that
    /// turns an `Email` into its raw address via `Display`.
    ///
    /// Callers that need the canonical (normalized) raw value must call
    /// [`Email::as_str`] explicitly — e.g., `db_query.bind(email.as_str())`
    /// — or use the `From<Email> for String` conversion. Using `{}` inside
    /// a SQL string, HTTP header value, or other external-system payload
    /// will silently produce the masked form and should be treated as a
    /// bug at code-review time.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.masked())
    }
}

#[cfg(test)]
mod tests;
