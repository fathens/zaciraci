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
    // Seed the DB via raw SQL so test state is deterministic, reload,
    // and verify the cache reflects the new rows. Then mutate the DB
    // and call reload again to verify revocation semantics: the
    // removed row disappears from the cache after reload, without
    // requiring a process restart.
    use diesel::RunQueryDsl;
    use diesel::sql_query;
    use diesel::sql_types::Text;

    let conn = persistence::connection_pool::get().await.unwrap();

    async fn upsert_raw(raw: &str, role: &str) {
        let conn = persistence::connection_pool::get().await.unwrap();
        let raw = raw.to_string();
        let role = role.to_string();
        conn.interact(move |conn| {
            sql_query(
                "INSERT INTO authorized_users (email, role) VALUES ($1, $2) \
                 ON CONFLICT ((lower(email))) DO UPDATE SET role = EXCLUDED.role",
            )
            .bind::<Text, _>(raw)
            .bind::<Text, _>(role)
            .execute(conn)
        })
        .await
        .unwrap()
        .unwrap();
    }

    async fn delete_raw(raw: &str) {
        let conn = persistence::connection_pool::get().await.unwrap();
        let raw = raw.to_string();
        conn.interact(move |conn| {
            sql_query("DELETE FROM authorized_users WHERE lower(email) = lower($1)")
                .bind::<Text, _>(raw)
                .execute(conn)
        })
        .await
        .unwrap()
        .unwrap();
    }

    // Wipe any leftover test users from prior runs.
    conn.interact(|conn| {
        sql_query("DELETE FROM authorized_users WHERE email LIKE 'reload-test-%@example.com'")
            .execute(conn)
    })
    .await
    .unwrap()
    .unwrap();

    upsert_raw("reload-test-a@example.com", "reader").await;
    upsert_raw("reload-test-b@example.com", "writer").await;

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
    delete_raw("reload-test-a@example.com").await;
    upsert_raw("reload-test-b@example.com", "reader").await;
    cache.reload().await.unwrap();
    assert_eq!(cache.lookup(&email("reload-test-a@example.com")), None);
    assert_eq!(
        cache.lookup(&email("reload-test-b@example.com")),
        Some(Role::Reader)
    );

    delete_raw("reload-test-b@example.com").await;
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
