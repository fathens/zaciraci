use crate::Result;
use crate::logging::*;
use crate::persistence::connection_pool;
use crate::persistence::schema::pool_info;
use crate::ref_finance::pool_info::{PoolInfo as RefPoolInfo, PoolInfoBared};
use anyhow::anyhow;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use near_sdk::json_types::U128;
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
    pub updated_at: NaiveDateTime,
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
    pub updated_at: NaiveDateTime,
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
        
        // RefPoolInfoを返す
        Ok(RefPoolInfo::new(db_pool.pool_id as u32, bare))
    }
    
    // RefPoolInfoからNewDbPoolInfoへの変換
    fn to_new_db(&self) -> Result<NewDbPoolInfo> {
        Ok(NewDbPoolInfo {
            pool_id: self.id() as i32,
            pool_kind: self.bare().pool_kind.clone(),
            // Vec<TokenAccount>をJSONに変換
            token_account_ids: serde_json::to_value(&self.bare().token_account_ids)?,
            // Vec<U128>をJSONに変換
            amounts: serde_json::to_value(&self.bare().amounts)?,
            total_fee: self.bare().total_fee as i32,
            // U128をJSONに変換
            shares_total_supply: serde_json::to_value(&self.bare().shares_total_supply)?,
            amp: self.bare().amp as i64,
            updated_at: self.updated_at(),
        })
    }

    // データベースに挿入
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
    pub async fn batch_insert(pool_infos: &[RefPoolInfo]) -> Result<()> {
        use diesel::RunQueryDsl;

        if pool_infos.is_empty() {
            return Ok(());
        }

        let new_pools: Result<Vec<NewDbPoolInfo>> = pool_infos
            .iter()
            .map(|pool| pool.to_new_db())
            .collect();
        
        let new_pools = new_pools?;
        let conn = connection_pool::get().await?;

        conn.interact(move |conn| {
            diesel::insert_into(pool_info::table)
                .values(&new_pools)
                .execute(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

        Ok(())
    }

    // 特定のプールIDの最新情報を取得
    pub async fn get_latest(pool_id: u32) -> Result<Option<RefPoolInfo>> {
        use diesel::QueryDsl;
        use diesel::dsl::max;

        let pool_id_i32 = pool_id as i32;
        let conn = connection_pool::get().await?;

        // まず最新のタイムスタンプを検索
        let latest_timestamp = conn
            .interact(move |conn| {
                pool_info::table
                    .filter(pool_info::pool_id.eq(pool_id_i32))
                    .select(max(pool_info::updated_at))
                    .first::<Option<NaiveDateTime>>(conn)
                    .optional()
            })
            .await
            .map_err(|e| anyhow!("Database interaction error: {:?}", e))??
            .flatten();

        // タイムスタンプが存在する場合、そのレコードを取得
        if let Some(timestamp) = latest_timestamp {
            let pool_id_i32 = pool_id as i32;
            let conn = connection_pool::get().await?;

            let result = conn
                .interact(move |conn| {
                    pool_info::table
                        .filter(pool_info::pool_id.eq(pool_id_i32))
                        .filter(pool_info::updated_at.eq(timestamp))
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
        } else {
            Ok(None)
        }
    }

    // データベースIDによる取得
    pub async fn get(id: i32) -> Result<Option<RefPoolInfo>> {
        use diesel::QueryDsl;

        let conn = connection_pool::get().await?;

        let result = conn
            .interact(move |conn| {
                pool_info::table
                    .filter(pool_info::id.eq(id))
                    .first::<DbPoolInfo>(conn)
                    .optional()
            })
            .await
            .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

        match result {
            Some(db_pool) => Ok(Some(RefPoolInfo::from_db(db_pool)?)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ref_finance::token_account::TokenAccount;
    use std::str::FromStr;

    // テスト用のヘルパー関数 - テーブルをクリーンアップ
    async fn clean_table() -> Result<()> {
        use diesel::RunQueryDsl;
        
        let conn = connection_pool::get().await?;
        
        conn.interact(|conn| {
            diesel::delete(pool_info::table).execute(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
        
        Ok(())
    }
    
    // テスト用のプールデータを作成
    fn create_test_pool_info(id: u32) -> RefPoolInfo {
        let token1 = TokenAccount::from_str("token1.near").unwrap();
        let token2 = TokenAccount::from_str("token2.near").unwrap();
        
        let bare = PoolInfoBared {
            pool_kind: "SIMPLE_POOL".to_string(),
            token_account_ids: vec![token1, token2],
            amounts: vec![U128(1000), U128(2000)],
            total_fee: 30,
            shares_total_supply: U128(500),
            amp: 0,
        };
        
        RefPoolInfo::new(id, bare)
    }
    
    #[tokio::test]
    async fn test_pool_info_single_insert() -> Result<()> {
        // テスト前にテーブルをクリーンアップ
        clean_table().await?;
        
        // テスト用データを作成
        let pool = create_test_pool_info(1);
        
        // データベースに挿入
        pool.insert().await?;
        
        // 挿入したデータを取得
        let retrieved = RefPoolInfo::get_latest(1).await?.unwrap();
        
        // 元のデータと一致するか確認
        assert_eq!(retrieved.id(), pool.id());
        assert_eq!(retrieved.bare().pool_kind, pool.bare().pool_kind);
        assert_eq!(retrieved.bare().token_account_ids, pool.bare().token_account_ids);
        assert_eq!(retrieved.bare().total_fee, pool.bare().total_fee);
        
        // 後始末
        clean_table().await?;
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_pool_info_batch_insert() -> Result<()> {
        // テスト前にテーブルをクリーンアップ
        clean_table().await?;
        
        // テスト用データを作成
        let pool1 = create_test_pool_info(1);
        let pool2 = create_test_pool_info(2);
        let pools = vec![pool1.clone(), pool2.clone()];
        
        // 一括挿入
        RefPoolInfo::batch_insert(&pools).await?;
        
        // データを個別に取得して確認
        let retrieved1 = RefPoolInfo::get_latest(1).await?.unwrap();
        let retrieved2 = RefPoolInfo::get_latest(2).await?.unwrap();
        
        assert_eq!(retrieved1.id(), pool1.id());
        assert_eq!(retrieved2.id(), pool2.id());
        
        // 後始末
        clean_table().await?;
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_pool_info_latest() -> Result<()> {
        // テスト前にテーブルをクリーンアップ
        clean_table().await?;
        
        // テスト用データを作成（同じpool_idで異なるタイムスタンプ）
        let mut pool1 = create_test_pool_info(1);
        
        // 1つ目のレコードを挿入
        pool1.insert().await?;
        
        // 少し待ってから2つ目のレコードを挿入（タイムスタンプを変える）
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        // 2つ目のレコード用に金額を変更
        let mut pool2 = create_test_pool_info(1);
        let mut bare = pool2.bare().clone();
        bare.amounts = vec![U128(1500), U128(2500)];
        let pool2 = RefPoolInfo::new(1, bare);
        
        pool2.insert().await?;
        
        // 最新のデータを取得
        let latest = RefPoolInfo::get_latest(1).await?.unwrap();
        
        // 最新のデータが2つ目のレコードと一致するか確認
        assert_eq!(latest.id(), pool2.id());
        assert_eq!(latest.bare().amounts[0].0, 1500);
        assert_eq!(latest.bare().amounts[1].0, 2500);
        
        // 後始末
        clean_table().await?;
        
        Ok(())
    }
}