use crate::Result;
use crate::connection_pool;
use crate::schema::{config_store, config_store_history};
use anyhow::anyhow;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use logging::*;
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
/// `instance_id` は起動時設定から取得する。DB 接続に依存するキーは
/// DB からは取得しない前提。
pub async fn reload_to_config(instance_id: &str) -> Result<()> {
    let configs = get_all_for_instance(instance_id).await?;
    common::config::store::load_db_config(configs);
    Ok(())
}

/// Minimum retention period to prevent accidental mass deletion
const MIN_RETENTION_DAYS: u32 = 7;

/// 指定日数より古い config_store_history レコードを削除
pub async fn cleanup_old_history(retention_days: u32) -> Result<()> {
    use diesel::sql_types::Timestamp;

    let log = DEFAULT.new(o!(
        "function" => "config_store::cleanup_old_history",
        "retention_days" => retention_days,
    ));

    if retention_days == 0 {
        warn!(
            log,
            "retention_days is 0, skipping cleanup to prevent deleting all records"
        );
        return Ok(());
    }

    let effective_days = retention_days.max(MIN_RETENTION_DAYS);
    if effective_days != retention_days {
        warn!(log, "retention_days below minimum, using minimum";
            "requested" => retention_days, "effective" => effective_days);
    }

    trace!(log, "start");

    let cutoff_date =
        chrono::Utc::now().naive_utc() - chrono::TimeDelta::days(i64::from(effective_days));

    let conn = connection_pool::get().await?;

    let deleted_count = conn
        .interact(move |conn| {
            diesel::sql_query("DELETE FROM config_store_history WHERE changed_at < $1")
                .bind::<Timestamp, _>(cutoff_date)
                .execute(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    info!(log, "finish"; "deleted_count" => deleted_count);
    Ok(())
}

#[cfg(test)]
mod tests;
