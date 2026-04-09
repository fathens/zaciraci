use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use common::types::{Email, Role};
use logging::{DEFAULT, info, o, warn};

/// Default interval between background cache refreshes. Chosen as a
/// compromise: short enough that a DELETE from `authorized_users` takes
/// effect within a single digit number of minutes (bounding the window in
/// which a revoked principal can still authenticate against a stale
/// snapshot), long enough that the DB load is negligible.
pub(crate) const DEFAULT_REFRESH_INTERVAL: Duration = Duration::from_secs(300);

/// In-memory cache of email → role mappings loaded from the
/// `authorized_users` table.
///
/// The cache is loaded at startup via [`UserCache::load_from_db`] and
/// periodically refreshed in the background by the task spawned via
/// [`UserCache::spawn_refresh_task`]. An explicit [`UserCache::reload`]
/// is also available (e.g. for tests, or for a future user-management
/// RPC that wants an immediate refresh after modifying the table).
///
/// Revocation semantics: removing a row or downgrading a role in
/// `authorized_users` becomes effective at most
/// `DEFAULT_REFRESH_INTERVAL` later. Until then, the cached snapshot
/// still answers lookups. This is a deliberate trade-off: the cache
/// avoids per-request DB load, at the cost of a bounded revocation
/// delay.
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

    /// Re-query the database and atomically swap the cached map.
    ///
    /// On failure the existing snapshot is preserved so a transient DB
    /// outage does not wipe the cache. Callers can observe the error but
    /// the cache remains usable either way.
    pub async fn reload(&self) -> anyhow::Result<()> {
        let entries = persistence::authorized_users::list_all().await?;
        let count = entries.len();
        let new_map: HashMap<Email, Role> = entries.into_iter().collect();
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

    /// Spawn a background task that periodically calls [`Self::reload`]
    /// every `interval`, logging failures without aborting the loop.
    ///
    /// This is the revocation path: deletions and role downgrades in
    /// `authorized_users` become visible to the auth interceptor after
    /// at most one `interval`.
    pub fn spawn_refresh_task(self: &Arc<Self>, interval: Duration) {
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let log = DEFAULT.new(o!("module" => "google_auth::user_cache::refresh"));
            let mut ticker = tokio::time::interval(interval);
            // Skip the first immediate tick: the cache was just loaded by
            // `load_from_db` at bootstrap, so reloading again right away is
            // wasted DB load.
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if let Err(err) = this.reload().await {
                    warn!(log, "user_cache_reload_failed"; "error" => %err);
                }
            }
        });
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
