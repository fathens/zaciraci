use super::*;

fn email(s: &str) -> Email {
    Email::new(s).expect("test email is valid")
}

#[test]
fn empty_cache_has_no_entries() {
    let cache = UserCache::empty();
    assert!(cache.is_empty());
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.lookup(&email("anyone@example.com")), None);
}

#[test]
fn from_entries_populates_cache() {
    let cache = UserCache::from_entries(vec![
        (email("alice@example.com"), Role::Writer),
        (email("bob@example.com"), Role::Reader),
    ]);

    assert_eq!(cache.len(), 2);
    assert_eq!(
        cache.lookup(&email("alice@example.com")),
        Some(Role::Writer)
    );
    assert_eq!(cache.lookup(&email("bob@example.com")), Some(Role::Reader));
    assert_eq!(cache.lookup(&email("eve@example.com")), None);
}

#[tokio::test]
#[serial_test::serial]
async fn reload_swaps_snapshot_from_db() {
    // Seed the DB via the persistence test helpers (no direct diesel
    // dep here), reload, and verify the cache reflects the new rows.
    // Then mutate the DB and call reload again to verify revocation
    // semantics: the removed row disappears from the cache after
    // reload, without requiring a process restart.
    use persistence::authorized_users::test_helpers::{raw_delete, raw_upsert, wipe_by_email_like};

    // Wipe any leftover test users from prior runs.
    wipe_by_email_like("reload-test-%@example.com")
        .await
        .unwrap();

    raw_upsert("reload-test-a@example.com", "reader")
        .await
        .unwrap();
    raw_upsert("reload-test-b@example.com", "writer")
        .await
        .unwrap();

    let cache = UserCache::load_from_db().await.unwrap();
    assert_eq!(
        cache.lookup(&email("reload-test-a@example.com")),
        Some(Role::Reader)
    );
    assert_eq!(
        cache.lookup(&email("reload-test-b@example.com")),
        Some(Role::Writer)
    );

    // Remove one user and downgrade the other, then reload and verify
    // that the cache reflects both changes.
    raw_delete("reload-test-a@example.com").await.unwrap();
    raw_upsert("reload-test-b@example.com", "reader")
        .await
        .unwrap();
    cache.reload().await.unwrap();
    assert_eq!(cache.lookup(&email("reload-test-a@example.com")), None);
    assert_eq!(
        cache.lookup(&email("reload-test-b@example.com")),
        Some(Role::Reader)
    );

    raw_delete("reload-test-b@example.com").await.unwrap();
}

#[test]
fn lookup_is_case_insensitive_via_email_normalization() {
    // The Email newtype trims and lowercases at construction, so equivalent
    // surface forms collapse to the same key both on insert and on lookup.
    let cache = UserCache::from_entries(vec![(email("Alice@Example.com"), Role::Reader)]);
    assert_eq!(
        cache.lookup(&email("Alice@Example.com")),
        Some(Role::Reader)
    );
    assert_eq!(
        cache.lookup(&email("alice@example.com")),
        Some(Role::Reader)
    );
    assert_eq!(
        cache.lookup(&email("  ALICE@EXAMPLE.COM  ")),
        Some(Role::Reader)
    );
    assert_eq!(cache.lookup(&email("eve@example.com")), None);
}
