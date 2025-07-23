use super::TimeRange;
use crate::Result;
use crate::logging::*;
use crate::persistence::connection_pool;
use crate::persistence::schema::pool_info;
use crate::ref_finance::pool_info::{PoolInfo as RefPoolInfo, PoolInfoBared};
use anyhow::anyhow;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde_json::Value as JsonValue;
use std::sync::Arc;

// データベース用モデル
#[allow(dead_code)]
#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = pool_info)]
struct DbPoolInfo {
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

// 変換ロジックの実装
impl RefPoolInfo {
    // DbPoolInfoからRefPoolInfoへの変換
    fn from_db(db_pool: DbPoolInfo) -> Result<Self> {
        // token_account_idsをJSONからVec<TokenAccount>に変換
        let token_account_ids = serde_json::from_value(db_pool.token_account_ids)?;

        // amountsをJSONからVec<U128>に変換
        let amounts = serde_json::from_value(db_pool.amounts)?;

        // shares_total_supplyをJSONからU128に変換
        let shares_total_supply = serde_json::from_value(db_pool.shares_total_supply)?;

        // PoolInfoBaredを構築
        let bare = PoolInfoBared {
            pool_kind: db_pool.pool_kind,
            token_account_ids,
            amounts,
            total_fee: db_pool.total_fee as u32,
            shares_total_supply,
            amp: db_pool.amp as u64,
        };

        // RefPoolInfoを作成
        let pool_info = RefPoolInfo::new(db_pool.pool_id as u32, bare, db_pool.timestamp);

        // RefPoolInfoを返す
        Ok(pool_info)
    }

    // RefPoolInfoからNewDbPoolInfoへの変換
    fn to_new_db(&self) -> Result<NewDbPoolInfo> {
        Ok(NewDbPoolInfo {
            pool_id: self.id as i32,
            pool_kind: self.bare.pool_kind.clone(),
            // Vec<TokenAccount>をJSONに変換
            token_account_ids: serde_json::to_value(&self.bare.token_account_ids)?,
            // Vec<U128>をJSONに変換
            amounts: serde_json::to_value(&self.bare.amounts)?,
            total_fee: self.bare.total_fee as i32,
            // U128をJSONに変換
            shares_total_supply: serde_json::to_value(self.bare.shares_total_supply)?,
            amp: self.bare.amp as i64,
            timestamp: self.timestamp,
        })
    }

    // データベースに挿入
    #[allow(dead_code)]
    pub async fn insert(&self) -> Result<()> {
        use diesel::RunQueryDsl;

        let new_pool = self.to_new_db()?;
        let conn = connection_pool::get().await?;

        conn.interact(move |conn| {
            diesel::insert_into(pool_info::table)
                .values(&new_pool)
                .execute(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

        Ok(())
    }

    // 複数レコードを一括挿入
    pub async fn batch_insert(pool_infos: &[Arc<RefPoolInfo>]) -> Result<()> {
        let log = DEFAULT.new(o!(
            "function" => "batch_insert",
            "pool_infos" => pool_infos.len(),
        ));
        info!(log, "start");
        use diesel::RunQueryDsl;

        if pool_infos.is_empty() {
            return Ok(());
        }

        let new_pools: Result<Vec<NewDbPoolInfo>> =
            pool_infos.iter().map(|pool| pool.to_new_db()).collect();

        let new_pools = new_pools?;
        let conn = connection_pool::get().await?;

        conn.interact(move |conn| {
            diesel::insert_into(pool_info::table)
                .values(&new_pools)
                .execute(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

        info!(log, "finish");
        Ok(())
    }

    // 特定のプールIDの最新情報を取得
    pub async fn get_latest(pool_id: u32) -> Result<Option<RefPoolInfo>> {
        use diesel::ExpressionMethods;
        use diesel::QueryDsl;
        use diesel::prelude::*;

        let pool_id_i32 = pool_id as i32;
        let conn = connection_pool::get().await?;

        // 1回のクエリで最新のレコードを取得
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
            Ok(Some(RefPoolInfo::from_db(db_pool)?))
        } else {
            Ok(None)
        }
    }

    pub async fn get_latest_before(
        pool_id: u32,
        timestamp: NaiveDateTime,
    ) -> Result<Option<RefPoolInfo>> {
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
            Ok(Some(RefPoolInfo::from_db(db_pool)?))
        } else {
            Ok(None)
        }
    }

    pub async fn get_all_unique_between(range: TimeRange) -> Result<Vec<RefPoolInfo>> {
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
            pool_infos.push(RefPoolInfo::from_db(db_pool)?);
        }
        Ok(pool_infos)
    }

    // データベースIDによる取得
    pub async fn get(id: i32) -> Result<Option<RefPoolInfo>> {
        use diesel::ExpressionMethods;
        use diesel::QueryDsl;

        let conn = connection_pool::get().await?;

        let result = conn
            .interact(move |conn| {
                pool_info::table
                    .filter(pool_info::id.eq(&id))
                    .first(conn)
                    .optional()
            })
            .await
            .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

        if let Some(db_pool) = result {
            Ok(Some(RefPoolInfo::from_db(db_pool)?))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests;
