use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use common::types::Role;
use logging::{DEFAULT, info, o};
use persistence::authorized_users::normalize_email;

/// In-memory cache of email → role mappings loaded from the
/// `authorized_users` table.
///
/// The cache is intentionally simple: it is loaded at startup and only
/// reloaded when an explicit `reload` call is made (for instance after a
/// user-management RPC modifies the table). The synchronous `lookup`
/// method is called from the auth interceptor.
pub struct UserCache {
    inner: RwLock<HashMap<String, Role>>,
}

impl UserCache {
    /// Build an empty cache. Useful for tests and as a placeholder before
    /// the first DB load.
    pub fn empty() -> Arc<Self> {
        Arc::new(Self {
            inner: RwLock::new(HashMap::new()),
        })
    }

    /// Build a cache pre-populated with the given users.
    ///
    /// Email keys are normalized (trimmed and lowercased) so that lookups
    /// are case-insensitive.
    pub fn from_entries<I>(entries: I) -> Arc<Self>
    where
        I: IntoIterator<Item = (String, Role)>,
    {
        let map: HashMap<String, Role> = entries
            .into_iter()
            .map(|(email, role)| (normalize_email(&email), role))
            .collect();
        Arc::new(Self {
            inner: RwLock::new(map),
        })
    }

    /// Build a cache by loading all authorized users from the database.
    ///
    /// On success returns a populated cache. On failure propagates the
    /// persistence error so callers can decide whether to abort or fall
    /// back to an empty cache.
    pub async fn load_from_db() -> anyhow::Result<Arc<Self>> {
        let entries = persistence::authorized_users::list_all().await?;
        let count = entries.len();
        let cache = Self::from_entries(entries);
        let log = DEFAULT.new(o!("module" => "google_auth::user_cache"));
        info!(log, "user_cache_loaded"; "count" => count);
        Ok(cache)
    }

    /// Look up the role for a given email. Returns `None` if the user is
    /// not in the cache. Lookup is case-insensitive: the input is normalized
    /// (trimmed and lowercased) before the map lookup.
    pub fn lookup(&self, email: &str) -> Option<Role> {
        let normalized = normalize_email(email);
        let guard = self
            .inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.get(&normalized).copied()
    }

    /// Returns true if the cache has no entries.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        let guard = self
            .inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.is_empty()
    }

    /// Returns the number of cached users.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        let guard = self
            .inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.len()
    }
}

#[cfg(test)]
mod tests;
