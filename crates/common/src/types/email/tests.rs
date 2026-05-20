use super::*;

#[test]
fn normalizes_case_and_trims() {
    let e = Email::new("  Alice@Example.COM  ").unwrap();
    assert_eq!(e.as_str(), "alice@example.com");
}

#[test]
fn rejects_empty() {
    assert_eq!(Email::new("").unwrap_err(), ParseEmailError::Empty);
    assert_eq!(Email::new("   ").unwrap_err(), ParseEmailError::Empty);
}

#[test]
fn rejects_missing_at() {
    assert_eq!(
        Email::new("not-an-email").unwrap_err(),
        ParseEmailError::MissingAtSign
    );
}

#[test]
fn rejects_empty_parts() {
    assert_eq!(
        Email::new("@example.com").unwrap_err(),
        ParseEmailError::EmptyPart
    );
    assert_eq!(
        Email::new("alice@").unwrap_err(),
        ParseEmailError::EmptyPart
    );
}

#[test]
fn rejects_multiple_at_signs() {
    assert_eq!(
        Email::new("a@b@c").unwrap_err(),
        ParseEmailError::MultipleAtSigns
    );
}

#[test]
fn rejects_internal_whitespace_or_control() {
    assert_eq!(
        Email::new("ali ce@example.com").unwrap_err(),
        ParseEmailError::InvalidCharacter
    );
    assert_eq!(
        Email::new("alice\n@example.com").unwrap_err(),
        ParseEmailError::InvalidCharacter
    );
}

#[test]
fn masked_form_hides_local_part() {
    let e = Email::new("alice@example.com").unwrap();
    assert_eq!(e.masked(), "a***@example.com");
}

#[test]
fn display_renders_masked_form() {
    let e = Email::new("alice@example.com").unwrap();
    assert_eq!(format!("{e}"), "a***@example.com");
}

#[test]
fn display_matches_masked() {
    // Guards against drift between the zero-allocation `Display` path and
    // the owned-`String` `masked()` helper: they must stay byte-identical.
    for input in [
        "alice@example.com",
        "a@b",
        "Bob+filter@Example.COM",
        "user.name@sub.domain.example",
    ] {
        let e = Email::new(input).unwrap();
        assert_eq!(format!("{e}"), e.masked());
    }
}

#[test]
fn from_into_string_returns_normalized() {
    let e = Email::new("Bob@Example.com").unwrap();
    let s: String = e.into();
    assert_eq!(s, "bob@example.com");
}

#[test]
fn fromstr_matches_new() {
    let parsed: Email = "Carol@x.io".parse().unwrap();
    assert_eq!(parsed.as_str(), "carol@x.io");
}

#[test]
fn deserialize_normalizes() {
    let json = "\"Dave@Example.COM\"";
    let e: Email = serde_json::from_str(json).unwrap();
    assert_eq!(e.as_str(), "dave@example.com");
}

#[test]
fn deserialize_rejects_invalid() {
    let json = "\"not-an-email\"";
    let result: Result<Email, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

#[test]
fn equality_is_after_normalization() {
    let a = Email::new("Alice@Example.com").unwrap();
    let b = Email::new("alice@example.com").unwrap();
    assert_eq!(a, b);
}

#[test]
fn debug_does_not_leak_full_email() {
    let e = Email::new("alice@example.com").unwrap();
    let debug = format!("{e:?}");
    assert!(!debug.contains("alice@"), "Debug leaked raw email: {debug}");
    assert!(debug.contains("a***@example.com"));
}

#[test]
fn debug_pretty_form_masks() {
    let e = Email::new("alice@example.com").unwrap();
    let pretty = format!("{e:#?}");
    assert!(
        !pretty.contains("alice@"),
        "Pretty debug leaked raw email: {pretty}"
    );
    assert!(pretty.contains("a***@example.com"));
}

#[test]
fn debug_in_hashmap_masks_entries() {
    use std::collections::HashMap;
    let mut m: HashMap<Email, &str> = HashMap::new();
    m.insert(Email::new("alice@example.com").unwrap(), "reader");
    let s = format!("{m:?}");
    assert!(!s.contains("alice@"), "HashMap debug leaked raw email: {s}");
    assert!(s.contains("a***@example.com"));
}

#[test]
fn debug_in_nested_struct_masks() {
    #[derive(Debug)]
    struct Wrapper {
        #[expect(dead_code)]
        email: Email,
    }
    let w = Wrapper {
        email: Email::new("alice@example.com").unwrap(),
    };
    let s = format!("{w:?}");
    assert!(
        !s.contains("alice@"),
        "derive(Debug) transitivity leaked: {s}"
    );
}
