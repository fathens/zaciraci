use super::test_helpers::{raw_delete, raw_upsert};
use super::*;
use serial_test::serial;

fn email(s: &str) -> Email {
    Email::new(s).expect("test email is valid")
}

#[tokio::test]
#[serial]
async fn test_list_all_returns_email_domain_type() {
    let raw = "list-all-norm@example.com";
    raw_upsert(raw, "reader").await.unwrap();

    let all = list_all().await.unwrap();
    let found = all.iter().find(|(e, _)| e == &email(raw));
    assert!(found.is_some());
    assert_eq!(found.unwrap().1, Role::Reader);

    raw_delete(raw).await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_check_constraint_rejects_mixed_case_email() {
    // Defense-in-depth: the `CHECK (email = LOWER(email))` constraint must
    // reject any direct SQL insert that carries a non-lowercase email, so
    // that `list_all`'s per-row tolerance never has to paper over a row the
    // application would reject at read time.
    let raw = "Check-Case-Test@Example.COM";
    let result = raw_upsert(raw, "reader").await;
    assert!(
        result.is_err(),
        "CHECK (email = LOWER(email)) should reject mixed-case email"
    );
    // Cleanup in case some future change relaxes the constraint and the
    // insert unexpectedly succeeded.
    let _ = raw_delete(raw).await;
}

#[tokio::test]
#[serial]
async fn test_list_all_ordered_by_email() {
    raw_upsert("z-user-list@example.com", "reader")
        .await
        .unwrap();
    raw_upsert("a-user-list@example.com", "writer")
        .await
        .unwrap();

    let all = list_all().await.unwrap();
    let pos_a = all
        .iter()
        .position(|(e, _)| e == &email("a-user-list@example.com"));
    let pos_z = all
        .iter()
        .position(|(e, _)| e == &email("z-user-list@example.com"));

    assert!(pos_a.unwrap() < pos_z.unwrap());

    raw_delete("z-user-list@example.com").await.unwrap();
    raw_delete("a-user-list@example.com").await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_check_constraint_rejects_invalid_role() {
    // Defense-in-depth: the DB CHECK constraint must reject any role
    // string outside the allowed set. If this constraint regresses,
    // `list_all`'s `to_role` would start failing on previously-valid
    // rows, so we need to know it is still in place.
    let raw = "check-constraint-test@example.com";
    let result = raw_upsert(raw, "superadmin").await;
    assert!(
        result.is_err(),
        "CHECK constraint should reject role='superadmin'"
    );
    // Cleanup in case some future change relaxes the constraint and the
    // insert unexpectedly succeeded.
    let _ = raw_delete(raw).await;
}
