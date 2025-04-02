use crate::Result;
use crate::persistence::connection_pool;
use crate::persistence::schema::pool_info;
use crate::ref_finance::pool_info::{PoolInfo as RefPoolInfo, PoolInfoBared};
use anyhow::anyhow;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde_json::Value as JsonValue;

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
#[allow(dead_code)]
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
        let mut pool_info = RefPoolInfo::new(db_pool.pool_id as u32, bare);
        // タイムスタンプを設定
        pool_info.timestamp = db_pool.timestamp;
        
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
            shares_total_supply: serde_json::to_value(&self.bare.shares_total_supply)?,
            amp: self.bare.amp as i64,
            timestamp: self.timestamp,
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
        use diesel::ExpressionMethods;

        let pool_id_i32 = pool_id as i32;
        let conn = connection_pool::get().await?;

        // まず最新のタイムスタンプを検索
        let latest_timestamp = conn
            .interact(move |conn| {
                pool_info::table
                    .filter(pool_info::pool_id.eq(&pool_id_i32))
                    .select(max(pool_info::timestamp))
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
                        .filter(pool_info::pool_id.eq(&pool_id_i32))
                        .filter(pool_info::timestamp.eq(&timestamp))
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
        use diesel::ExpressionMethods;

        let conn = connection_pool::get().await?;

        let result = conn
            .interact(move |conn| {
                pool_info::table
                    .filter(pool_info::id.eq(&id))
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
    use chrono::Utc;
    use crate::ref_finance::token_account::TokenAccount;
    use near_sdk::json_types::U128;
    use std::str::FromStr;

    fn create_test_pool_info() -> RefPoolInfo {
        // TokenAccountはタプル構造体なので、FromStrを使ってAccountIdから作成
        let token1 = TokenAccount::from_str("token1.near").unwrap();
        let token2 = TokenAccount::from_str("token2.near").unwrap();

        let bare = PoolInfoBared {
            pool_kind: "STABLE_SWAP".to_string(),
            token_account_ids: vec![token1, token2],
            amounts: vec![U128(1000000), U128(2000000)],
            total_fee: 30,
            shares_total_supply: U128(5000000),
            amp: 100,
        };

        RefPoolInfo::new(123, bare)
    }

    #[tokio::test]
    async fn test_pool_info_insert() -> Result<()> {
        let pool_info = create_test_pool_info();
        pool_info.insert().await?;
        
        // データベースから取得して値を確認
        let retrieved = RefPoolInfo::get_latest(123).await?;
        assert!(retrieved.is_some());
        
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, 123);
        assert_eq!(retrieved.bare.pool_kind, "STABLE_SWAP");
        assert_eq!(retrieved.bare.token_account_ids.len(), 2);
        // TokenAccountはタプル構造体なのでDisplayを使って文字列比較
        assert_eq!(retrieved.bare.token_account_ids[0].to_string(), "token1.near");
        assert_eq!(retrieved.bare.amounts.len(), 2);
        assert_eq!(retrieved.bare.amounts[0].0, 1000000);
        assert_eq!(retrieved.bare.total_fee, 30);
        assert_eq!(retrieved.bare.shares_total_supply.0, 5000000);
        assert_eq!(retrieved.bare.amp, 100);
        
        Ok(())
    }

    #[tokio::test]
    async fn test_pool_info_batch_insert() -> Result<()> {
        let mut pool_info1 = create_test_pool_info();
        pool_info1.id = 124;
        
        let mut pool_info2 = create_test_pool_info();
        pool_info2.id = 125;
        pool_info2.bare.pool_kind = "WEIGHTED_SWAP".to_string();
        
        RefPoolInfo::batch_insert(&[pool_info1, pool_info2]).await?;
        
        // データベースから取得して値を確認
        let retrieved1 = RefPoolInfo::get_latest(124).await?;
        let retrieved2 = RefPoolInfo::get_latest(125).await?;
        
        assert!(retrieved1.is_some());
        assert!(retrieved2.is_some());
        
        let retrieved1 = retrieved1.unwrap();
        let retrieved2 = retrieved2.unwrap();
        
        assert_eq!(retrieved1.id, 124);
        assert_eq!(retrieved1.bare.pool_kind, "STABLE_SWAP");
        
        assert_eq!(retrieved2.id, 125);
        assert_eq!(retrieved2.bare.pool_kind, "WEIGHTED_SWAP");
        
        Ok(())
    }

    #[tokio::test]
    async fn test_pool_info_latest() -> Result<()> {
        let mut pool_info = create_test_pool_info();
        pool_info.id = 126;
        pool_info.bare.pool_kind = "STABLE_SWAP".to_string();
        pool_info.insert().await?;
        
        // 1秒待機して新しいタイムスタンプでデータを更新
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        
        let mut updated_pool_info = pool_info.clone();
        updated_pool_info.bare.pool_kind = "WEIGHTED_SWAP".to_string();
        updated_pool_info.timestamp = Utc::now().naive_utc();
        updated_pool_info.insert().await?;
        
        // 最新のデータを取得
        let retrieved = RefPoolInfo::get_latest(126).await?;
        assert!(retrieved.is_some());
        
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, 126);
        assert_eq!(retrieved.bare.pool_kind, "WEIGHTED_SWAP");
        
        Ok(())
    }

    #[tokio::test]
    async fn test_pool_info_get_by_id() -> Result<()> {
        // まず直接データベースにクエリを実行してテーブルをクリアする
        use diesel::RunQueryDsl;
        
        let conn = connection_pool::get().await?;
        conn.interact(|conn| {
            diesel::delete(pool_info::table)
                .execute(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
        
        // テスト用のデータを作成して挿入
        let pool_info = create_test_pool_info();
        pool_info.insert().await?;
        
        // IDを取得するため直接データベースに問い合わせる
        let conn = connection_pool::get().await?;
        let result = conn.interact(|conn| {
            use diesel::dsl::max;
            pool_info::table
                .select(max(pool_info::id))
                .first::<Option<i32>>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
        
        let db_id = result.unwrap();
        
        // そのIDを使ってget関数でデータを取得
        let result = RefPoolInfo::get(db_id).await?;
        assert!(result.is_some(), "ID {}のプールが見つかりませんでした", db_id);
        
        let result = result.unwrap();
        
        // 元のデータと一致することを確認
        assert_eq!(result.id, 123);
        assert_eq!(result.bare.pool_kind, "STABLE_SWAP");
        assert_eq!(result.bare.token_account_ids.len(), 2);
        assert_eq!(result.bare.token_account_ids[0].to_string(), "token1.near");
        assert_eq!(result.bare.amounts.len(), 2);
        assert_eq!(result.bare.amounts[0].0, 1000000);
        assert_eq!(result.bare.total_fee, 30);
        assert_eq!(result.bare.shares_total_supply.0, 5000000);
        assert_eq!(result.bare.amp, 100);
        
        Ok(())
    }
}