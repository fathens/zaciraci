use std::fmt;

/// A string that must never reach the logs.
///
/// Both `Debug` and `Display` render `"[redacted]"` (or `"[unset]"` when
/// empty, so a missing secret is still visible as a misconfiguration);
/// the value is only reachable through [`SecretString::expose`]. This
/// makes a stray `{}` / `{:?}` / slog `%value` interpolation harmless
/// and keeps the real value out of `#[derive(Debug)]` output of any
/// struct that embeds it.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// The real value. Call sites should be few and obvious (building an
    /// `Authorization` header, opening a connection).
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn presence(&self) -> &'static str {
        if self.0.is_empty() {
            "[unset]"
        } else {
            "[redacted]"
        }
    }
}

impl From<String> for SecretString {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.presence())
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.presence())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_and_display_never_show_the_value() {
        let s = SecretString::new("hunter2".to_owned());
        assert_eq!(format!("{s:?}"), "[redacted]");
        assert_eq!(format!("{s}"), "[redacted]");
        assert_eq!(format!("{s:#?}"), "[redacted]");
        assert_eq!(s.expose(), "hunter2");
        assert!(!s.is_empty());
    }

    #[test]
    fn empty_value_reads_as_unset() {
        let s = SecretString::from(String::new());
        assert_eq!(format!("{s:?}"), "[unset]");
        assert_eq!(format!("{s}"), "[unset]");
        assert!(s.is_empty());
    }

    #[test]
    fn derived_debug_of_a_container_is_redacted() {
        #[derive(Debug)]
        struct Holder {
            key: SecretString,
        }
        let h = Holder {
            key: SecretString::new("topsecret".to_owned()),
        };
        let text = format!("{h:?}");
        assert!(!text.contains("topsecret"));
        assert!(text.contains("[redacted]"));
        assert_eq!(h.key.expose(), "topsecret");
    }
}
