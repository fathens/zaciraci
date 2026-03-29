use super::*;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn test_get_all_for_instance_global_only() {
    // グローバル設定のみの場合
    upsert("*", "TEST_KEY_A", "global_a", None).await.unwrap();
    upsert("*", "TEST_KEY_B", "global_b", None).await.unwrap();

    let configs = get_all_for_instance("test-instance").await.unwrap();
    assert_eq!(
        configs.get("TEST_KEY_A").map(|s| s.as_str()),
        Some("global_a")
    );
    assert_eq!(
        configs.get("TEST_KEY_B").map(|s| s.as_str()),
        Some("global_b")
    );

    // Cleanup
    delete("*", "TEST_KEY_A").await.unwrap();
    delete("*", "TEST_KEY_B").await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_get_all_for_instance_priority() {
    // インスタンス固有 + グローバル混在時の優先度
    upsert("*", "TEST_PRIORITY_KEY", "global_value", None)
        .await
        .unwrap();
    upsert("inst-1", "TEST_PRIORITY_KEY", "instance_value", None)
        .await
        .unwrap();

    let configs = get_all_for_instance("inst-1").await.unwrap();
    assert_eq!(
        configs.get("TEST_PRIORITY_KEY").map(|s| s.as_str()),
        Some("instance_value")
    );

    // 別のインスタンスはグローバルが返る
    let configs2 = get_all_for_instance("inst-2").await.unwrap();
    assert_eq!(
        configs2.get("TEST_PRIORITY_KEY").map(|s| s.as_str()),
        Some("global_value")
    );

    // Cleanup
    delete("*", "TEST_PRIORITY_KEY").await.unwrap();
    delete("inst-1", "TEST_PRIORITY_KEY").await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_get_one_exists() {
    upsert("*", "TEST_GET_ONE", "value_one", None)
        .await
        .unwrap();

    let result = get_one("any-instance", "TEST_GET_ONE").await.unwrap();
    assert_eq!(result, Some("value_one".to_string()));

    // Cleanup
    delete("*", "TEST_GET_ONE").await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_get_one_not_exists() {
    let result = get_one("any-instance", "TEST_NONEXISTENT_KEY_XYZ")
        .await
        .unwrap();
    assert_eq!(result, None);
}

#[tokio::test]
#[serial]
async fn test_upsert_insert_then_update() {
    // INSERT
    upsert("*", "TEST_UPSERT_KEY", "initial", Some("test desc"))
        .await
        .unwrap();
    let val = get_one("*", "TEST_UPSERT_KEY").await.unwrap();
    assert_eq!(val, Some("initial".to_string()));

    // UPDATE
    upsert("*", "TEST_UPSERT_KEY", "updated", None)
        .await
        .unwrap();
    let val = get_one("*", "TEST_UPSERT_KEY").await.unwrap();
    assert_eq!(val, Some("updated".to_string()));

    // 履歴が記録されていることを確認
    let conn = connection_pool::get().await.unwrap();
    let history_count: i64 = conn
        .interact(|conn| {
            use diesel::dsl::count;
            config_store_history::table
                .filter(config_store_history::key.eq("TEST_UPSERT_KEY"))
                .select(count(config_store_history::id))
                .first::<i64>(conn)
        })
        .await
        .unwrap()
        .unwrap();
    assert!(history_count >= 2, "Should have at least 2 history entries");

    // Cleanup
    delete("*", "TEST_UPSERT_KEY").await.unwrap();
    // 履歴はクリーンアップしない（append-only）
}

#[tokio::test]
#[serial]
async fn test_delete_records_history() {
    upsert("*", "TEST_DELETE_KEY", "to_delete", None)
        .await
        .unwrap();
    delete("*", "TEST_DELETE_KEY").await.unwrap();

    let val = get_one("*", "TEST_DELETE_KEY").await.unwrap();
    assert_eq!(val, None);

    // 履歴に "(deleted)" が記録されていることを確認
    let conn = connection_pool::get().await.unwrap();
    let last_history: Option<String> = conn
        .interact(|conn| {
            config_store_history::table
                .filter(config_store_history::key.eq("TEST_DELETE_KEY"))
                .order_by(config_store_history::changed_at.desc())
                .select(config_store_history::new_value)
                .first::<String>(conn)
                .optional()
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(last_history, Some("(deleted)".to_string()));
}

#[tokio::test]
#[serial]
async fn test_cleanup_old_history() {
    use diesel::sql_types::{Nullable, Text, Timestamp, Varchar};

    // 古い履歴レコードを直接 INSERT
    {
        let conn = connection_pool::get().await.unwrap();
        let old_time = chrono::Utc::now().naive_utc() - chrono::TimeDelta::days(400);
        conn.interact(move |conn| {
            diesel::sql_query(
                "INSERT INTO config_store_history (instance_id, key, old_value, new_value, changed_at) \
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind::<Varchar, _>("*")
            .bind::<Varchar, _>("TEST_CLEANUP_HISTORY_KEY")
            .bind::<Nullable<Text>, _>(None::<String>)
            .bind::<Text, _>("old_value")
            .bind::<Timestamp, _>(old_time)
            .execute(conn)
        })
        .await
        .unwrap()
        .unwrap();
    }

    // cleanup (365日) で古いレコードが消える
    cleanup_old_history(365).await.unwrap();

    // 400日前のレコードが消えていることを確認
    {
        let conn = connection_pool::get().await.unwrap();
        let count: i64 = conn
            .interact(|conn| {
                use diesel::dsl::count;
                config_store_history::table
                    .filter(config_store_history::key.eq("TEST_CLEANUP_HISTORY_KEY"))
                    .select(count(config_store_history::id))
                    .first::<i64>(conn)
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(count, 0, "400-day-old history record should be deleted");
    }
}

#[tokio::test]
#[serial]
async fn test_cleanup_old_history_zero_days_skips() {
    use diesel::sql_types::{Nullable, Text, Timestamp, Varchar};

    // 古い履歴レコードを作成
    {
        let conn = connection_pool::get().await.unwrap();
        let old_time = chrono::Utc::now().naive_utc() - chrono::TimeDelta::days(400);
        let test_key_owned = "TEST_CLEANUP_ZERO_DAYS_KEY".to_string();
        conn.interact(move |conn| {
            diesel::sql_query(
                "INSERT INTO config_store_history (instance_id, key, old_value, new_value, changed_at) \
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind::<Varchar, _>("*")
            .bind::<Varchar, _>(&test_key_owned)
            .bind::<Nullable<Text>, _>(None::<String>)
            .bind::<Text, _>("zero_days_value")
            .bind::<Timestamp, _>(old_time)
            .execute(conn)
        })
        .await
        .unwrap()
        .unwrap();
    }

    // retention_days=0 は何も削除しない
    let result = cleanup_old_history(0).await;
    assert!(result.is_ok());

    // レコードが残っていることを確認
    {
        let conn = connection_pool::get().await.unwrap();
        let count: i64 = conn
            .interact(|conn| {
                use diesel::dsl::count;
                config_store_history::table
                    .filter(config_store_history::key.eq("TEST_CLEANUP_ZERO_DAYS_KEY"))
                    .select(count(config_store_history::id))
                    .first::<i64>(conn)
            })
            .await
            .unwrap()
            .unwrap();
        assert!(count > 0, "record should remain when retention_days is 0");
    }

    // クリーンアップ
    {
        let conn = connection_pool::get().await.unwrap();
        conn.interact(|conn| {
            diesel::delete(
                config_store_history::table
                    .filter(config_store_history::key.eq("TEST_CLEANUP_ZERO_DAYS_KEY")),
            )
            .execute(conn)
        })
        .await
        .unwrap()
        .unwrap();
    }
}

#[tokio::test]
#[serial]
async fn test_reload_to_config() {
    // DB に値を設定
    upsert("*", "TEST_RELOAD_KEY", "db_value", None)
        .await
        .unwrap();

    // reload_to_config を実行
    reload_to_config("*").await.unwrap();

    // common::config::store::get() で取得可能になること
    let val = common::config::store::get("TEST_RELOAD_KEY").unwrap();
    assert_eq!(val, "db_value");

    // Cleanup
    delete("*", "TEST_RELOAD_KEY").await.unwrap();
}
