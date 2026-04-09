use super::*;
use serial_test::serial;

/// Test-only helper to fetch a single authorized user by email.
///
/// This is not used by production code (the runtime path loads all
/// authorized users in a single `list_all()` call into `UserCache`), but
/// the CRUD tests below need a pointwise lookup to verify upsert / delete
/// behaviour. Keeping it inside the `tests` module avoids exposing a
/// dead/test-only API on the production `authorized_users` module.
async fn find_by_email(email: &Email) -> Result<Option<(Email, Role)>> {
    let key = email.as_str().to_string();
    let conn = crate::connection_pool::get().await?;

    let result: Option<DbAuthorizedUser> = conn
        .interact(move |conn| {
            authorized_users::table
                .filter(authorized_users::email.eq(&key))
                .select(DbAuthorizedUser::as_select())
                .first(conn)
                .optional()
        })
        .await
        .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

    Ok(match result {
        Some(user) => {
            let email = to_email(&user.email)?;
            let role = to_role(&user.role)?;
            Some((email, role))
        }
        None => None,
    })
}

fn email(s: &str) -> Email {
    Email::new(s).expect("test email is valid")
}

#[tokio::test]
#[serial]
async fn test_upsert_and_find_by_email() {
    let e = email("test-auth@example.com");
    upsert(&e, Role::Reader).await.unwrap();

    let result = find_by_email(&e).await.unwrap();
    assert!(result.is_some());
    let (found_email, role) = result.unwrap();
    assert_eq!(found_email, e);
    assert_eq!(role, Role::Reader);

    // Cleanup
    delete(&e).await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_upsert_overwrite_role() {
    let e = email("test-overwrite@example.com");
    upsert(&e, Role::Reader).await.unwrap();

    let (_, role) = find_by_email(&e).await.unwrap().unwrap();
    assert_eq!(role, Role::Reader);

    upsert(&e, Role::Writer).await.unwrap();

    let (_, role) = find_by_email(&e).await.unwrap().unwrap();
    assert_eq!(role, Role::Writer);

    // Cleanup
    delete(&e).await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_find_by_email_not_found() {
    let result = find_by_email(&email("nonexistent@example.com"))
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
#[serial]
async fn test_delete() {
    let e = email("test-delete@example.com");
    upsert(&e, Role::Writer).await.unwrap();

    let result = find_by_email(&e).await.unwrap();
    assert!(result.is_some());

    delete(&e).await.unwrap();

    let result = find_by_email(&e).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
#[serial]
async fn test_upsert_normalizes_email_case() {
    // Insert with mixed case + whitespace; the Email newtype normalizes at
    // construction so the DB row, the lookup key, and equality comparisons
    // all agree on the lowercase form.
    let input = email("  Case-Test@Example.COM  ");
    let normalized = email("case-test@example.com");
    upsert(&input, Role::Writer).await.unwrap();

    // Lookup with a differently-cased input must match because both go
    // through `Email::new`, which lowercases.
    let alt = email("CASE-TEST@example.com");
    let found = find_by_email(&alt).await.unwrap();
    assert!(found.is_some());
    let (stored_email, role) = found.unwrap();
    assert_eq!(stored_email, normalized);
    assert_eq!(role, Role::Writer);

    delete(&input).await.unwrap();
    assert!(find_by_email(&normalized).await.unwrap().is_none());
}

#[tokio::test]
#[serial]
async fn test_delete_nonexistent() {
    // Should not error
    delete(&email("never-existed@example.com")).await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_list_all() {
    let email_a = email("test-list-a@example.com");
    let email_b = email("test-list-b@example.com");
    upsert(&email_a, Role::Reader).await.unwrap();
    upsert(&email_b, Role::Writer).await.unwrap();

    let all = list_all().await.unwrap();
    let found_a = all.iter().find(|(e, _)| e == &email_a);
    let found_b = all.iter().find(|(e, _)| e == &email_b);

    assert!(found_a.is_some());
    assert_eq!(found_a.unwrap().1, Role::Reader);
    assert!(found_b.is_some());
    assert_eq!(found_b.unwrap().1, Role::Writer);

    // Cleanup
    delete(&email_a).await.unwrap();
    delete(&email_b).await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_list_all_ordered_by_email() {
    let email_z = email("z-user@example.com");
    let email_a = email("a-user@example.com");
    upsert(&email_z, Role::Reader).await.unwrap();
    upsert(&email_a, Role::Writer).await.unwrap();

    let all = list_all().await.unwrap();
    let pos_a = all.iter().position(|(e, _)| e == &email_a);
    let pos_z = all.iter().position(|(e, _)| e == &email_z);

    assert!(pos_a.unwrap() < pos_z.unwrap());

    // Cleanup
    delete(&email_z).await.unwrap();
    delete(&email_a).await.unwrap();
}
