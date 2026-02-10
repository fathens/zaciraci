use crate::Result;
use crate::connection_pool;
use crate::schema::{config_store, config_store_history};
use anyhow::anyhow;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use std::collections::HashMap;

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = config_store)]
#[diesel(check_for_backend(diesel::pg::Pg))]
struct DbConfigEntry {
    pub instance_id: String,
    pub key: String,
    pub value: String,
    #[allow(dead_code)]
    pub description: Option<String>,
    #[allow(dead_code)]
    pub updated_at: NaiveDateTime,
    #[allow(dead_code)]
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = config_store)]
struct NewConfigEntry {
    pub instance_id: String,
    pub key: String,
    pub value: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = config_store_history)]
struct NewConfigHistory {
    pub instance_id: String,
    pub key: String,
    pub old_value: Option<String>,
    pub new_value: String,
}

/// インスタンスに該当する全設定を取得
///
/// `WHERE instance_id IN (instance_id, '*')` でクエリし、
/// インスタンス固有の値がグローバル値 (`*`) より優先される。
pub async fn get_all_for_instance(instance_id: &str) -> Result<HashMap<String, String>> {
    let instance_id = instance_id.to_string();
    let conn = connection_pool::get().await?;

    let results: Vec<DbConfigEntry> = conn
        .interact(move |conn| {
            config_store::table
                .filter(
                    config_store::instance_id
                        .eq(&instance_id)
                        .or(config_store::instance_id.eq("*")),
                )
                .order_by(config_store::instance_id.desc()) // instance_id > '*' (ASCII order)
                .load::<DbConfigEntry>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    // インスタンス固有の値が優先されるように HashMap に入れる
    // DESC ソートなのでインスタンス固有が先に来る
    // → ただし HashMap は後勝ちなので、グローバルが先に入ってからインスタンスで上書きする必要がある
    // → ASCにして先にグローバル、後にインスタンス固有が上書き
    // 実際: DESC = instance_id が先、'*' が後 → 後勝ちだと '*' が勝つ → 逆
    // → ASC にしよう

    // やり直し: HashMap は insert で上書きされるので、グローバル → インスタンス固有 の順で入れる
    let mut map = HashMap::new();
    // まずグローバル設定を入れる
    for entry in &results {
        if entry.instance_id == "*" {
            map.insert(entry.key.clone(), entry.value.clone());
        }
    }
    // インスタンス固有で上書き
    for entry in &results {
        if entry.instance_id != "*" {
            map.insert(entry.key.clone(), entry.value.clone());
        }
    }

    Ok(map)
}

/// 単一キーの値を取得（インスタンス固有 > グローバル）
pub async fn get_one(instance_id: &str, key: &str) -> Result<Option<String>> {
    let instance_id = instance_id.to_string();
    let key = key.to_string();
    let conn = connection_pool::get().await?;

    let results: Vec<DbConfigEntry> = conn
        .interact(move |conn| {
            config_store::table
                .filter(
                    config_store::instance_id
                        .eq(&instance_id)
                        .or(config_store::instance_id.eq("*")),
                )
                .filter(config_store::key.eq(&key))
                .order_by(config_store::instance_id.desc())
                .load::<DbConfigEntry>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    // インスタンス固有が先 (DESC) → 最初のものを返す
    // もしインスタンス固有がなければグローバルが返る
    Ok(results.into_iter().next().map(|e| e.value))
}

/// 設定を upsert（存在しなければ INSERT、存在すれば UPDATE）+ 履歴記録
#[allow(dead_code)]
pub async fn upsert(
    instance_id: &str,
    key: &str,
    value: &str,
    description: Option<&str>,
) -> Result<()> {
    let instance_id = instance_id.to_string();
    let key = key.to_string();
    let value = value.to_string();
    let description = description.map(|s| s.to_string());
    let conn = connection_pool::get().await?;

    conn.interact(move |conn| {
        conn.transaction::<_, diesel::result::Error, _>(|conn| {
            // 既存の値を取得
            let existing: Option<DbConfigEntry> = config_store::table
                .filter(config_store::instance_id.eq(&instance_id))
                .filter(config_store::key.eq(&key))
                .first::<DbConfigEntry>(conn)
                .optional()?;

            let old_value = existing.map(|e| e.value);

            // UPSERT
            diesel::insert_into(config_store::table)
                .values(&NewConfigEntry {
                    instance_id: instance_id.clone(),
                    key: key.clone(),
                    value: value.clone(),
                    description: description.clone(),
                })
                .on_conflict((config_store::instance_id, config_store::key))
                .do_update()
                .set((
                    config_store::value.eq(&value),
                    config_store::description.eq(&description),
                    config_store::updated_at.eq(diesel::dsl::now),
                ))
                .execute(conn)?;

            // 履歴記録
            diesel::insert_into(config_store_history::table)
                .values(&NewConfigHistory {
                    instance_id,
                    key,
                    old_value,
                    new_value: value,
                })
                .execute(conn)?;

            Ok(())
        })
    })
    .await
    .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    Ok(())
}

/// 設定を削除 + 履歴記録
#[allow(dead_code)]
pub async fn delete(instance_id: &str, key: &str) -> Result<()> {
    let instance_id = instance_id.to_string();
    let key = key.to_string();
    let conn = connection_pool::get().await?;

    conn.interact(move |conn| {
        conn.transaction::<_, diesel::result::Error, _>(|conn| {
            // 既存の値を取得
            let existing: Option<DbConfigEntry> = config_store::table
                .filter(config_store::instance_id.eq(&instance_id))
                .filter(config_store::key.eq(&key))
                .first::<DbConfigEntry>(conn)
                .optional()?;

            if let Some(entry) = existing {
                // 削除
                diesel::delete(
                    config_store::table
                        .filter(config_store::instance_id.eq(&instance_id))
                        .filter(config_store::key.eq(&key)),
                )
                .execute(conn)?;

                // 履歴記録（new_value に "(deleted)" を記録）
                diesel::insert_into(config_store_history::table)
                    .values(&NewConfigHistory {
                        instance_id,
                        key,
                        old_value: Some(entry.value),
                        new_value: "(deleted)".to_string(),
                    })
                    .execute(conn)?;
            }

            Ok(())
        })
    })
    .await
    .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    Ok(())
}

/// DB から設定をロードし common::config に反映
///
/// `INSTANCE_ID` は env/TOML から取得する。DB 接続に依存するキーは
/// DB からは取得しない前提。
pub async fn reload_to_config() -> Result<()> {
    let instance_id = common::config::get("INSTANCE_ID").unwrap_or_else(|_| "*".to_string());
    let configs = get_all_for_instance(&instance_id).await?;
    common::config::load_db_config(configs);
    Ok(())
}

#[cfg(test)]
mod tests {
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
    async fn test_reload_to_config() {
        // DB に値を設定
        upsert("*", "TEST_RELOAD_KEY", "db_value", None)
            .await
            .unwrap();

        // reload_to_config を実行
        reload_to_config().await.unwrap();

        // common::config::get() で取得可能になること
        let val = common::config::get("TEST_RELOAD_KEY").unwrap();
        assert_eq!(val, "db_value");

        // Cleanup
        delete("*", "TEST_RELOAD_KEY").await.unwrap();
    }
}
