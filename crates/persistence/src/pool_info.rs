use crate::Result;
use crate::connection_pool;
use crate::schema::pool_info;
use anyhow::anyhow;
use chrono::NaiveDateTime;
use common::config;
use common::types::TimeRange;
use dex::{PoolInfo, PoolInfoBared, PoolInfoList};
use diesel::prelude::*;
use logging::*;
use serde_json::Value as JsonValue;
use std::sync::Arc;

// データベース用モデル
#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = pool_info)]
struct DbPoolInfo {
    #[allow(dead_code)] // Diesel Queryable でDBスキーマと一致させるため必要
    pub id: i32,
    pub pool_id: i32,
    pub pool_kind: String,
    pub token_account_ids: JsonValue,
    pub amounts: JsonValue,
    pub total_fee: i32,
    pub shares_total_supply: JsonValue,
    pub amp: i64,
    pub timestamp: NaiveDateTime,
}

// データベース挿入用モデル
#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = pool_info)]
struct NewDbPoolInfo {
    pub pool_id: i32,
    pub pool_kind: String,
    pub token_account_ids: JsonValue,
    pub amounts: JsonValue,
    pub total_fee: i32,
    pub shares_total_supply: JsonValue,
    pub amp: i64,
    pub timestamp: NaiveDateTime,
}

// DbPoolInfoからPoolInfoへの変換
fn from_db(db_pool: DbPoolInfo) -> Result<PoolInfo> {
    let token_account_ids = serde_json::from_value(db_pool.token_account_ids)?;
    let amounts = serde_json::from_value(db_pool.amounts)?;
    let shares_total_supply = serde_json::from_value(db_pool.shares_total_supply)?;

    let bare = PoolInfoBared {
        pool_kind: db_pool.pool_kind,
        token_account_ids,
        amounts,
        total_fee: db_pool.total_fee as u32,
        shares_total_supply,
        amp: db_pool.amp as u64,
    };

    Ok(PoolInfo::new(
        db_pool.pool_id as u32,
        bare,
        db_pool.timestamp,
    ))
}

// PoolInfoからNewDbPoolInfoへの変換
fn to_new_db(pool: &PoolInfo) -> Result<NewDbPoolInfo> {
    Ok(NewDbPoolInfo {
        pool_id: pool.id as i32,
        pool_kind: pool.bare.pool_kind.clone(),
        token_account_ids: serde_json::to_value(&pool.bare.token_account_ids)?,
        amounts: serde_json::to_value(&pool.bare.amounts)?,
        total_fee: pool.bare.total_fee as i32,
        shares_total_supply: serde_json::to_value(pool.bare.shares_total_supply)?,
        amp: pool.bare.amp as i64,
        timestamp: pool.timestamp,
    })
}

/// 複数レコードを一括挿入
pub async fn batch_insert(pool_infos: &[Arc<PoolInfo>]) -> Result<()> {
    let log = DEFAULT.new(o!(
        "function" => "pool_info::batch_insert",
        "pool_infos" => pool_infos.len(),
    ));
    trace!(log, "start");
    use diesel::RunQueryDsl;

    if pool_infos.is_empty() {
        return Ok(());
    }

    let new_pools: Result<Vec<NewDbPoolInfo>> =
        pool_infos.iter().map(|pool| to_new_db(pool)).collect();

    let new_pools = new_pools?;
    let conn = connection_pool::get().await?;

    conn.interact(move |conn| {
        diesel::insert_into(pool_info::table)
            .values(&new_pools)
            .execute(conn)
    })
    .await
    .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    // 古いレコードをクリーンアップ
    let retention_count = config::get("POOL_INFO_RETENTION_COUNT")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(10);

    trace!(log, "cleaning up old records"; "retention_count" => retention_count);
    cleanup_old_records(retention_count).await?;

    trace!(log, "finish");
    Ok(())
}

/// 古いレコードを削除し、pool_id ごとに指定数だけ保持する
pub async fn cleanup_old_records(retention_count: u32) -> Result<()> {
    use diesel::prelude::*;
    use diesel::sql_types::BigInt;

    let log = DEFAULT.new(o!(
        "function" => "pool_info::cleanup_old_records",
        "retention_count" => retention_count,
    ));
    trace!(log, "start");

    let retention_count_i64 = retention_count as i64;
    let conn = connection_pool::get().await?;

    let deleted_count = conn
        .interact(move |conn| {
            diesel::sql_query(
                "DELETE FROM pool_info WHERE id IN (
                    SELECT id FROM (
                        SELECT id,
                               ROW_NUMBER() OVER (PARTITION BY pool_id ORDER BY timestamp DESC) as rn
                        FROM pool_info
                    ) t
                    WHERE t.rn > $1
                )"
            )
            .bind::<BigInt, _>(retention_count_i64)
            .execute(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    trace!(log, "finish"; "deleted_count" => deleted_count);
    Ok(())
}

/// 特定のプールIDの最新情報を取得
pub async fn get_latest(pool_id: u32) -> Result<Option<PoolInfo>> {
    use diesel::ExpressionMethods;
    use diesel::QueryDsl;
    use diesel::prelude::*;

    let pool_id_i32 = pool_id as i32;
    let conn = connection_pool::get().await?;

    let result = conn
        .interact(move |conn| {
            pool_info::table
                .filter(pool_info::pool_id.eq(&pool_id_i32))
                .order_by(pool_info::timestamp.desc())
                .first::<DbPoolInfo>(conn)
                .optional()
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    if let Some(db_pool) = result {
        Ok(Some(from_db(db_pool)?))
    } else {
        Ok(None)
    }
}

/// 特定のプールIDの指定タイムスタンプ前の最新情報を取得
pub async fn get_latest_before(pool_id: u32, timestamp: NaiveDateTime) -> Result<Option<PoolInfo>> {
    use diesel::ExpressionMethods;
    use diesel::QueryDsl;

    let pool_id_i32 = pool_id as i32;
    let conn = connection_pool::get().await?;

    let result = conn
        .interact(move |conn| {
            pool_info::table
                .filter(pool_info::pool_id.eq(pool_id_i32))
                .filter(pool_info::timestamp.lt(timestamp))
                .order_by(pool_info::timestamp.desc())
                .first::<DbPoolInfo>(conn)
                .optional()
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    if let Some(db_pool) = result {
        Ok(Some(from_db(db_pool)?))
    } else {
        Ok(None)
    }
}

/// 指定範囲内のプールIDごとにユニークな最新レコードを取得
pub async fn get_all_unique_between(range: TimeRange) -> Result<Vec<PoolInfo>> {
    use diesel::ExpressionMethods;
    use diesel::QueryDsl;

    let conn = connection_pool::get().await?;

    let result = conn
        .interact(move |conn| {
            pool_info::table
                .filter(pool_info::timestamp.ge(range.start))
                .filter(pool_info::timestamp.le(range.end))
                .distinct_on(pool_info::pool_id)
                .order_by((pool_info::pool_id, pool_info::timestamp.desc()))
                .load::<DbPoolInfo>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    let mut pool_infos = Vec::with_capacity(result.len());
    for db_pool in result {
        pool_infos.push(from_db(db_pool)?);
    }
    Ok(pool_infos)
}

/// DBからPoolInfoListを読み込む
pub async fn read_from_db(timestamp: Option<NaiveDateTime>) -> Result<Arc<PoolInfoList>> {
    let first = if let Some(timestamp) = timestamp {
        get_latest_before(0, timestamp).await?
    } else {
        get_latest(0).await?
    }
    .ok_or_else(|| anyhow!("no pool found"))?;

    let range = TimeRange {
        start: first.timestamp,
        end: timestamp.unwrap_or(chrono::Utc::now().naive_utc()),
    };
    let all = get_all_unique_between(range).await?;
    Ok(Arc::new(PoolInfoList::new(
        all.into_iter().map(Arc::new).collect(),
    )))
}

/// PoolInfoListをDBに書き込む
pub async fn write_to_db(list: &PoolInfoList) -> Result<()> {
    batch_insert(list.list()).await
}

#[cfg(test)]
mod tests;
