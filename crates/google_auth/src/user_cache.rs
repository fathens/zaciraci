use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use common::types::Role;
use logging::{DEFAULT, info, o};

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
    pub fn from_entries<I>(entries: I) -> Arc<Self>
    where
        I: IntoIterator<Item = (String, Role)>,
    {
        let map: HashMap<String, Role> = entries.into_iter().collect();
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

    /// Replace the cache contents by reloading from the database.
    ///
    /// Intended to be called after a successful user-management RPC.
    /// Briefly holds the write lock only long enough to swap the map in.
    pub async fn reload(&self) -> anyhow::Result<()> {
        let entries = persistence::authorized_users::list_all().await?;
        let count = entries.len();
        let new_map: HashMap<String, Role> = entries.into_iter().collect();
        {
            let mut guard = self
                .inner
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *guard = new_map;
        }
        let log = DEFAULT.new(o!("module" => "google_auth::user_cache"));
        info!(log, "user_cache_reloaded"; "count" => count);
        Ok(())
    }

    /// Look up the role for a given email. Returns `None` if the user is
    /// not in the cache.
    pub fn lookup(&self, email: &str) -> Option<Role> {
        let guard = self
            .inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.get(email).copied()
    }

    /// Returns true if the cache has no entries.
    pub fn is_empty(&self) -> bool {
        let guard = self
            .inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.is_empty()
    }

    /// Returns the number of cached users.
    pub fn len(&self) -> usize {
        let guard = self
            .inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.len()
    }
}

#[cfg(test)]
mod tests;
