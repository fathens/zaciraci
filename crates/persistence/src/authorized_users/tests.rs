use super::*;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn test_upsert_and_find_by_email() {
    let email = "test-auth@example.com";
    upsert(email, Role::Reader).await.unwrap();

    let result = find_by_email(email).await.unwrap();
    assert!(result.is_some());
    let (found_email, role) = result.unwrap();
    assert_eq!(found_email, email);
    assert_eq!(role, Role::Reader);

    // Cleanup
    delete(email).await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_upsert_overwrite_role() {
    let email = "test-overwrite@example.com";
    upsert(email, Role::Reader).await.unwrap();

    let (_, role) = find_by_email(email).await.unwrap().unwrap();
    assert_eq!(role, Role::Reader);

    upsert(email, Role::Writer).await.unwrap();

    let (_, role) = find_by_email(email).await.unwrap().unwrap();
    assert_eq!(role, Role::Writer);

    // Cleanup
    delete(email).await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_find_by_email_not_found() {
    let result = find_by_email("nonexistent@example.com").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
#[serial]
async fn test_delete() {
    let email = "test-delete@example.com";
    upsert(email, Role::Writer).await.unwrap();

    let result = find_by_email(email).await.unwrap();
    assert!(result.is_some());

    delete(email).await.unwrap();

    let result = find_by_email(email).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
#[serial]
async fn test_delete_nonexistent() {
    // Should not error
    delete("never-existed@example.com").await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_list_all() {
    let email_a = "test-list-a@example.com";
    let email_b = "test-list-b@example.com";
    upsert(email_a, Role::Reader).await.unwrap();
    upsert(email_b, Role::Writer).await.unwrap();

    let all = list_all().await.unwrap();
    let found_a = all.iter().find(|(e, _)| e == email_a);
    let found_b = all.iter().find(|(e, _)| e == email_b);

    assert!(found_a.is_some());
    assert_eq!(found_a.unwrap().1, Role::Reader);
    assert!(found_b.is_some());
    assert_eq!(found_b.unwrap().1, Role::Writer);

    // Cleanup
    delete(email_a).await.unwrap();
    delete(email_b).await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_list_all_ordered_by_email() {
    let email_z = "z-user@example.com";
    let email_a = "a-user@example.com";
    upsert(email_z, Role::Reader).await.unwrap();
    upsert(email_a, Role::Writer).await.unwrap();

    let all = list_all().await.unwrap();
    let emails: Vec<&str> = all.iter().map(|(e, _)| e.as_str()).collect();
    let pos_a = emails.iter().position(|e| *e == email_a);
    let pos_z = emails.iter().position(|e| *e == email_z);

    assert!(pos_a.unwrap() < pos_z.unwrap());

    // Cleanup
    delete(email_z).await.unwrap();
    delete(email_a).await.unwrap();
}
