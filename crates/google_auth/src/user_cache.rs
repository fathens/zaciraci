use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use common::types::{Email, Role};
use logging::{DEFAULT, info, o};

/// In-memory cache of email → role mappings loaded from the
/// `authorized_users` table.
///
/// The cache is intentionally simple: it is loaded at startup and only
/// reloaded when an explicit `reload` call is made (for instance after a
/// user-management RPC modifies the table). The synchronous `lookup`
/// method is called from the auth interceptor.
///
/// Keys are [`Email`] values whose normalization is enforced at construction,
/// so the cache cannot drift apart from the DB or from validator-side input.
pub struct UserCache {
    inner: RwLock<HashMap<Email, Role>>,
}

impl UserCache {
    /// Build an empty cache. Test-only helper; the runtime path uses
    /// [`UserCache::load_from_db`] instead.
    #[cfg(test)]
    pub(crate) fn empty() -> Arc<Self> {
        Arc::new(Self {
            inner: RwLock::new(HashMap::new()),
        })
    }

    /// Build a cache pre-populated with the given users.
    ///
    /// Used internally by [`UserCache::load_from_db`] and by tests that want
    /// deterministic cache contents; not exposed as `pub` because runtime
    /// callers should always go through the DB path.
    pub(crate) fn from_entries<I>(entries: I) -> Arc<Self>
    where
        I: IntoIterator<Item = (Email, Role)>,
    {
        let map: HashMap<Email, Role> = entries.into_iter().collect();
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
    /// not in the cache.
    pub fn lookup(&self, email: &Email) -> Option<Role> {
        let guard = self
            .inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.get(email).copied()
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
