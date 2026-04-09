use super::*;
use diesel::sql_query;
use diesel::sql_types::Text;
use serial_test::serial;

/// Test-only helper to raw-insert an authorized user row.
///
/// Uses `ON CONFLICT ((lower(email)))` so repeat test runs are idempotent
/// even when the previous run left the table with a row under a different
/// case. Bypasses `Email` normalization intentionally — tests need to be
/// able to put mixed-case rows into the table to exercise `list_all`'s
/// normalization at read time.
async fn raw_insert(raw_email: &str, role_str: &str) -> Result<()> {
    let email = raw_email.to_string();
    let role = role_str.to_string();
    let conn = connection_pool::get().await?;
    conn.interact(move |conn| {
        sql_query(
            "INSERT INTO authorized_users (email, role) VALUES ($1, $2) \
             ON CONFLICT ((lower(email))) DO UPDATE SET email = EXCLUDED.email, role = EXCLUDED.role",
        )
        .bind::<Text, _>(email)
        .bind::<Text, _>(role)
        .execute(conn)
    })
    .await
    .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;
    Ok(())
}

async fn raw_delete(raw_email: &str) -> Result<()> {
    let email = raw_email.to_string();
    let conn = connection_pool::get().await?;
    conn.interact(move |conn| {
        sql_query("DELETE FROM authorized_users WHERE lower(email) = lower($1)")
            .bind::<Text, _>(email)
            .execute(conn)
    })
    .await
    .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;
    Ok(())
}

fn email(s: &str) -> Email {
    Email::new(s).expect("test email is valid")
}

#[tokio::test]
#[serial]
async fn test_list_all_returns_normalized_emails() {
    let raw = "List-All-Norm@Example.COM";
    raw_insert(raw, "reader").await.unwrap();

    let all = list_all().await.unwrap();
    let found = all
        .iter()
        .find(|(e, _)| e == &email("list-all-norm@example.com"));
    assert!(found.is_some());
    assert_eq!(found.unwrap().1, Role::Reader);

    raw_delete(raw).await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_list_all_ordered_by_email() {
    raw_insert("z-user-list@example.com", "reader")
        .await
        .unwrap();
    raw_insert("a-user-list@example.com", "writer")
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
    let result = raw_insert(raw, "superadmin").await;
    assert!(
        result.is_err(),
        "CHECK constraint should reject role='superadmin'"
    );
    // Cleanup in case some future change relaxes the constraint and the
    // insert unexpectedly succeeded.
    let _ = raw_delete(raw).await;
}
