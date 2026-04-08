use super::*;

#[test]
fn empty_cache_has_no_entries() {
    let cache = UserCache::empty();
    assert!(cache.is_empty());
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.lookup("anyone@example.com"), None);
}

#[test]
fn from_entries_populates_cache() {
    let cache = UserCache::from_entries(vec![
        ("alice@example.com".to_string(), Role::Writer),
        ("bob@example.com".to_string(), Role::Reader),
    ]);

    assert_eq!(cache.len(), 2);
    assert_eq!(cache.lookup("alice@example.com"), Some(Role::Writer));
    assert_eq!(cache.lookup("bob@example.com"), Some(Role::Reader));
    assert_eq!(cache.lookup("eve@example.com"), None);
}

#[test]
fn lookup_is_case_insensitive_and_trimmed() {
    let cache = UserCache::from_entries(vec![("Alice@Example.com".to_string(), Role::Reader)]);
    // Entry and query are both normalized (trim + lowercase) before comparison.
    assert_eq!(cache.lookup("Alice@Example.com"), Some(Role::Reader));
    assert_eq!(cache.lookup("alice@example.com"), Some(Role::Reader));
    assert_eq!(cache.lookup("  ALICE@EXAMPLE.COM  "), Some(Role::Reader));
    assert_eq!(cache.lookup("eve@example.com"), None);
}
