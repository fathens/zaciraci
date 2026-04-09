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
