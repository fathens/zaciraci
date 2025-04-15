use crate::Result;
use crate::persistence::connection_pool;
use crate::persistence::schema::pool_info;
use crate::ref_finance::pool_info::{PoolInfo as RefPoolInfo, PoolInfoBared};
use anyhow::anyhow;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde_json::Value as JsonValue;
use std::sync::Arc;

use super::TimeRange;

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
mod tests {
    use super::*;
    use crate::ref_finance::token_account::TokenAccount;
    use near_sdk::json_types::U128;
    use serial_test::serial;
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

        RefPoolInfo::new(123, bare, chrono::Utc::now().naive_utc())
    }

    #[tokio::test]
    #[serial(pool_info)]
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
        assert_eq!(
            retrieved.bare.token_account_ids[0].to_string(),
            "token1.near"
        );
        assert_eq!(retrieved.bare.amounts.len(), 2);
        assert_eq!(retrieved.bare.amounts[0].0, 1000000);
        assert_eq!(retrieved.bare.total_fee, 30);
        assert_eq!(retrieved.bare.shares_total_supply.0, 5000000);
        assert_eq!(retrieved.bare.amp, 100);

        Ok(())
    }

    #[tokio::test]
    #[serial(pool_info)]
    async fn test_pool_info_batch_insert() -> Result<()> {
        let mut pool_info1 = create_test_pool_info();
        pool_info1.id = 124;

        let mut pool_info2 = create_test_pool_info();
        pool_info2.id = 125;
        pool_info2.bare.pool_kind = "WEIGHTED_SWAP".to_string();

        RefPoolInfo::batch_insert(&[Arc::new(pool_info1), Arc::new(pool_info2)]).await?;

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
    #[serial(pool_info)]
    async fn test_pool_info_latest() -> Result<()> {
        let mut pool_info = create_test_pool_info();
        pool_info.id = 126;
        pool_info.bare.pool_kind = "STABLE_SWAP".to_string();
        pool_info.insert().await?;

        // 1秒待機して新しいタイムスタンプでデータを更新
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let mut updated_pool_info = pool_info.clone();
        updated_pool_info.bare.pool_kind = "WEIGHTED_SWAP".to_string();
        updated_pool_info.timestamp = chrono::Utc::now().naive_utc();
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
    #[serial(pool_info)]
    async fn test_pool_info_get_by_id() -> Result<()> {
        // まず直接データベースにクエリを実行してテーブルをクリアする
        use diesel::RunQueryDsl;

        let conn = connection_pool::get().await?;
        conn.interact(|conn| diesel::delete(pool_info::table).execute(conn))
            .await
            .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

        // テスト用のデータを作成して挿入
        let pool_info = create_test_pool_info();
        pool_info.insert().await?;

        // IDを取得するため直接データベースに問い合わせる
        let conn = connection_pool::get().await?;
        let result = conn
            .interact(|conn| {
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
        assert!(
            result.is_some(),
            "ID {}のプールが見つかりませんでした",
            db_id
        );

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

    #[tokio::test]
    #[serial(pool_info)]
    async fn test_pool_info_get_latest_before() -> Result<()> {
        use chrono::NaiveDateTime;
        use diesel::Connection;
        use diesel::prelude::*;

        let conn = connection_pool::get().await?;

        // 通常のトランザクションを開始
        match conn
            .interact(|conn| {
                conn.transaction(|conn| {
                    // データベーステーブルをクリーンアップ
                    diesel::delete(pool_info::table).execute(conn)
                })
            })
            .await
        {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => return Err(anyhow!("Failed to clear table: {}", e)),
            Err(e) => return Err(anyhow!("Failed to interact with DB: {}", e)),
        };

        // テストデータの作成
        let timestamp1 = NaiveDateTime::parse_from_str("2023-01-01 00:00:00", "%Y-%m-%d %H:%M:%S")?;
        let timestamp2 = NaiveDateTime::parse_from_str("2023-01-02 00:00:00", "%Y-%m-%d %H:%M:%S")?;
        let timestamp3 = NaiveDateTime::parse_from_str("2023-01-03 00:00:00", "%Y-%m-%d %H:%M:%S")?;

        // プールIDを設定
        let pool_id_1: u32 = 1;
        let pool_id_2: u32 = 2;

        // テストデータを作成
        let test_pool_info = create_test_pool_info();

        // プールID 1 のデータを3つ作成（異なるタイムスタンプで）
        let mut pool_info1 = test_pool_info.clone();
        pool_info1.id = pool_id_1; // プールID 1に設定
        pool_info1.timestamp = timestamp1;

        let mut pool_info2 = test_pool_info.clone();
        pool_info2.id = pool_id_1; // プールID 1に設定
        pool_info2.timestamp = timestamp2;

        let mut pool_info3 = test_pool_info.clone();
        pool_info3.id = pool_id_1; // プールID 1に設定
        pool_info3.timestamp = timestamp3;

        // プールID 2 のデータを1つ作成
        let mut pool_info4 = test_pool_info.clone();
        pool_info4.id = pool_id_2; // プールID 2に設定
        pool_info4.timestamp = timestamp2;

        // データをデータベースに挿入
        let new_db1 = pool_info1.to_new_db()?;
        let new_db2 = pool_info2.to_new_db()?;
        let new_db3 = pool_info3.to_new_db()?;
        let new_db4 = pool_info4.to_new_db()?;

        // データベースに挿入
        // トランザクション内で実行（テスト内でのみ有効）
        match conn
            .interact(move |conn| {
                conn.transaction(|conn| {
                    diesel::insert_into(pool_info::table)
                        .values(&[new_db1, new_db2, new_db3, new_db4])
                        .execute(conn)
                })
            })
            .await
        {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => return Err(anyhow!("Insert error: {}", e)),
            Err(e) => return Err(anyhow!("DB error: {}", e)),
        };

        // テストケース1: プールID 1、timestamp2より前の最新データを取得（timestamp1が返されるはず）
        let result1 = RefPoolInfo::get_latest_before(pool_id_1, timestamp2).await?;
        assert!(
            result1.is_some(),
            "timestamp2より前のデータが見つかりませんでした"
        );
        assert_eq!(
            result1.unwrap().timestamp,
            timestamp1,
            "timestamp1が返されるべきです"
        );

        // テストケース2: プールID 1、timestamp3より前の最新データを取得（timestamp2が返されるはず）
        let result2 = RefPoolInfo::get_latest_before(pool_id_1, timestamp3).await?;
        assert!(
            result2.is_some(),
            "timestamp3より前のデータが見つかりませんでした"
        );
        assert_eq!(
            result2.unwrap().timestamp,
            timestamp2,
            "timestamp2が返されるべきです"
        );

        // テストケース3: プールID 1、timestamp1より前のデータを取得（存在しないのでNoneが返されるはず）
        let result3 = RefPoolInfo::get_latest_before(pool_id_1, timestamp1).await?;
        assert!(
            result3.is_none(),
            "timestamp1より前のデータが存在するべきではありません"
        );

        // テストケース4: 存在しないプールIDでデータを取得（Noneが返されるはず）
        let result4 = RefPoolInfo::get_latest_before(pool_id_2, timestamp2).await?;
        assert!(
            result4.is_none(),
            "存在しないプールIDのデータが見つかりました"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial(pool_info)]
    async fn test_pool_info_get_all_unique_between() -> Result<()> {
        use chrono::NaiveDateTime;
        use diesel::Connection;
        use diesel::prelude::*;

        // データベース接続を取得
        let conn = connection_pool::get().await?;

        // 通常のトランザクションを開始してテーブルをクリーンアップ
        match conn
            .interact(|conn| {
                conn.transaction(|conn| diesel::delete(pool_info::table).execute(conn))
            })
            .await
        {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => return Err(anyhow!("Failed to clear table: {}", e)),
            Err(e) => return Err(anyhow!("Failed to interact with DB: {}", e)),
        };

        // テストデータに使用するタイムスタンプを定義
        let timestamp1 = NaiveDateTime::parse_from_str("2023-01-01 00:00:00", "%Y-%m-%d %H:%M:%S")?;
        let timestamp2 = NaiveDateTime::parse_from_str("2023-01-02 00:00:00", "%Y-%m-%d %H:%M:%S")?;
        let timestamp3 = NaiveDateTime::parse_from_str("2023-01-03 00:00:00", "%Y-%m-%d %H:%M:%S")?;
        let timestamp4 = NaiveDateTime::parse_from_str("2023-01-04 00:00:00", "%Y-%m-%d %H:%M:%S")?;

        // プールIDを設定
        let pool_id_1: u32 = 1;
        let pool_id_2: u32 = 2;
        let pool_id_3: u32 = 3;

        // テストデータのベースを作成
        let test_pool_info = create_test_pool_info();

        // 異なるプールIDと異なるタイムスタンプでテストデータを作成
        let mut pool_infos = Vec::new();

        // プールID 1のデータ (timestamp1, timestamp3)
        let mut pool_info1_1 = test_pool_info.clone();
        pool_info1_1.id = pool_id_1;
        pool_info1_1.timestamp = timestamp1;
        pool_infos.push(pool_info1_1.to_new_db()?);

        let mut pool_info1_3 = test_pool_info.clone();
        pool_info1_3.id = pool_id_1;
        pool_info1_3.timestamp = timestamp3;
        pool_infos.push(pool_info1_3.to_new_db()?);

        // プールID 2のデータ (timestamp2)
        let mut pool_info2_2 = test_pool_info.clone();
        pool_info2_2.id = pool_id_2;
        pool_info2_2.timestamp = timestamp2;
        pool_infos.push(pool_info2_2.to_new_db()?);

        // プールID 3のデータ (timestamp4) - 指定期間外のデータ
        let mut pool_info3_4 = test_pool_info.clone();
        pool_info3_4.id = pool_id_3;
        pool_info3_4.timestamp = timestamp4;
        pool_infos.push(pool_info3_4.to_new_db()?);

        pool_infos.reverse();
        // データベースに挿入
        match conn
            .interact(move |conn| {
                conn.transaction(|conn| {
                    diesel::insert_into(pool_info::table)
                        .values(&pool_infos)
                        .execute(conn)
                })
            })
            .await
        {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => return Err(anyhow!("Insert error: {}", e)),
            Err(e) => return Err(anyhow!("DB error: {}", e)),
        };

        // テストケース1: timestamp1からtimestamp3までの期間のユニークなプール情報を取得
        let results = RefPoolInfo::get_all_unique_between(TimeRange {
            start: timestamp1,
            end: timestamp3,
        })
        .await?;

        // プールID 1と2が含まれていることを確認
        assert_eq!(
            results,
            vec![pool_info1_3.clone(), pool_info2_2.clone()],
            "プールID 1と2が含まれていません"
        );

        // 期間外のテストケース2: timestamp2からtimestamp4までの期間のユニークなプール情報を取得
        let results2 = RefPoolInfo::get_all_unique_between(TimeRange {
            start: timestamp2,
            end: timestamp4,
        })
        .await?;

        // プールID 1, 2, 3の情報が取得されるはず（3件）
        assert_eq!(
            results2,
            vec![
                pool_info1_3.clone(),
                pool_info2_2.clone(),
                pool_info3_4.clone()
            ],
            "プールIDユニークなデータは3件あるはずです"
        );

        // テストケース3: 範囲内にデータがない場合
        let empty_results = RefPoolInfo::get_all_unique_between(TimeRange {
            start: NaiveDateTime::parse_from_str("2022-01-01 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            end: NaiveDateTime::parse_from_str("2022-12-31 23:59:59", "%Y-%m-%d %H:%M:%S").unwrap(),
        })
        .await?;
        assert!(
            empty_results.is_empty(),
            "データがない期間では空の配列が返されるべきです"
        );

        // テストケース4: 1-2 の範囲のユニークなプール情報を取得
        let results4 = RefPoolInfo::get_all_unique_between(TimeRange {
            start: timestamp1,
            end: timestamp2,
        })
        .await?;
        assert_eq!(
            results4,
            vec![pool_info1_1.clone(), pool_info2_2.clone()],
            "プールIDユニークなデータは2件あります"
        );

        Ok(())
    }
}
