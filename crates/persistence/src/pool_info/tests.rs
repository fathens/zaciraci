use super::*;
use common::types::TokenAccount;
use dex::PoolInfoBared;
use near_sdk::json_types::U128;
use serial_test::serial;
use std::str::FromStr;
use std::sync::Arc;

fn create_test_pool_info() -> PoolInfo {
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

    PoolInfo::new(123, bare, chrono::Utc::now().naive_utc())
}

#[tokio::test]
#[serial(pool_info)]
async fn test_pool_info_batch_insert() -> Result<()> {
    let mut pool_info1 = create_test_pool_info();
    pool_info1.id = 124;

    let mut pool_info2 = create_test_pool_info();
    pool_info2.id = 125;
    pool_info2.bare.pool_kind = "WEIGHTED_SWAP".to_string();

    batch_insert(&[Arc::new(pool_info1), Arc::new(pool_info2)]).await?;

    // データベースから取得して値を確認
    let retrieved1 = get_latest(124).await?;
    let retrieved2 = get_latest(125).await?;

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
    use diesel::RunQueryDsl;

    let mut pool_info = create_test_pool_info();
    pool_info.id = 126;
    pool_info.bare.pool_kind = "STABLE_SWAP".to_string();

    let new_pool = to_new_db(&pool_info)?;
    let conn = connection_pool::get().await?;
    conn.interact(move |conn| {
        diesel::insert_into(pool_info::table)
            .values(&new_pool)
            .execute(conn)
    })
    .await
    .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    // 1秒待機して新しいタイムスタンプでデータを更新
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    let mut updated_pool_info = pool_info.clone();
    updated_pool_info.bare.pool_kind = "WEIGHTED_SWAP".to_string();
    updated_pool_info.timestamp = chrono::Utc::now().naive_utc();

    let new_pool = to_new_db(&updated_pool_info)?;
    let conn = connection_pool::get().await?;
    conn.interact(move |conn| {
        diesel::insert_into(pool_info::table)
            .values(&new_pool)
            .execute(conn)
    })
    .await
    .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    // 最新のデータを取得
    let retrieved = get_latest(126).await?;
    assert!(retrieved.is_some());

    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.id, 126);
    assert_eq!(retrieved.bare.pool_kind, "WEIGHTED_SWAP");

    Ok(())
}

#[tokio::test]
#[serial(pool_info)]
async fn test_pool_info_get_latest_before() -> Result<()> {
    use chrono::NaiveDateTime;
    use diesel::Connection;
    use diesel::prelude::*;

    let conn = connection_pool::get().await?;

    match conn
        .interact(|conn| conn.transaction(|conn| diesel::delete(pool_info::table).execute(conn)))
        .await
    {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => return Err(anyhow!("Failed to clear table: {}", e)),
        Err(e) => return Err(anyhow!("Failed to interact with DB: {}", e)),
    };

    let timestamp1 = NaiveDateTime::parse_from_str("2023-01-01 00:00:00", "%Y-%m-%d %H:%M:%S")?;
    let timestamp2 = NaiveDateTime::parse_from_str("2023-01-02 00:00:00", "%Y-%m-%d %H:%M:%S")?;
    let timestamp3 = NaiveDateTime::parse_from_str("2023-01-03 00:00:00", "%Y-%m-%d %H:%M:%S")?;

    let pool_id_1: u32 = 1;
    let pool_id_2: u32 = 2;

    let test_pool_info = create_test_pool_info();

    let mut pool_info1 = test_pool_info.clone();
    pool_info1.id = pool_id_1;
    pool_info1.timestamp = timestamp1;

    let mut pool_info2 = test_pool_info.clone();
    pool_info2.id = pool_id_1;
    pool_info2.timestamp = timestamp2;

    let mut pool_info3 = test_pool_info.clone();
    pool_info3.id = pool_id_1;
    pool_info3.timestamp = timestamp3;

    let mut pool_info4 = test_pool_info.clone();
    pool_info4.id = pool_id_2;
    pool_info4.timestamp = timestamp2;

    let new_db1 = to_new_db(&pool_info1)?;
    let new_db2 = to_new_db(&pool_info2)?;
    let new_db3 = to_new_db(&pool_info3)?;
    let new_db4 = to_new_db(&pool_info4)?;

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

    let result1 = get_latest_before(pool_id_1, timestamp2).await?;
    assert!(
        result1.is_some(),
        "timestamp2より前のデータが見つかりませんでした"
    );
    assert_eq!(
        result1.unwrap().timestamp,
        timestamp1,
        "timestamp1が返されるべきです"
    );

    let result2 = get_latest_before(pool_id_1, timestamp3).await?;
    assert!(
        result2.is_some(),
        "timestamp3より前のデータが見つかりませんでした"
    );
    assert_eq!(
        result2.unwrap().timestamp,
        timestamp2,
        "timestamp2が返されるべきです"
    );

    let result3 = get_latest_before(pool_id_1, timestamp1).await?;
    assert!(
        result3.is_none(),
        "timestamp1より前のデータが存在するべきではありません"
    );

    let result4 = get_latest_before(pool_id_2, timestamp2).await?;
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

    let conn = connection_pool::get().await?;

    match conn
        .interact(|conn| conn.transaction(|conn| diesel::delete(pool_info::table).execute(conn)))
        .await
    {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => return Err(anyhow!("Failed to clear table: {}", e)),
        Err(e) => return Err(anyhow!("Failed to interact with DB: {}", e)),
    };

    let timestamp1 = NaiveDateTime::parse_from_str("2023-01-01 00:00:00", "%Y-%m-%d %H:%M:%S")?;
    let timestamp2 = NaiveDateTime::parse_from_str("2023-01-02 00:00:00", "%Y-%m-%d %H:%M:%S")?;
    let timestamp3 = NaiveDateTime::parse_from_str("2023-01-03 00:00:00", "%Y-%m-%d %H:%M:%S")?;
    let timestamp4 = NaiveDateTime::parse_from_str("2023-01-04 00:00:00", "%Y-%m-%d %H:%M:%S")?;

    let pool_id_1: u32 = 1;
    let pool_id_2: u32 = 2;
    let pool_id_3: u32 = 3;

    let test_pool_info = create_test_pool_info();

    let mut pool_infos = Vec::new();

    let mut pool_info1_1 = test_pool_info.clone();
    pool_info1_1.id = pool_id_1;
    pool_info1_1.timestamp = timestamp1;
    pool_infos.push(to_new_db(&pool_info1_1)?);

    let mut pool_info1_3 = test_pool_info.clone();
    pool_info1_3.id = pool_id_1;
    pool_info1_3.timestamp = timestamp3;
    pool_infos.push(to_new_db(&pool_info1_3)?);

    let mut pool_info2_2 = test_pool_info.clone();
    pool_info2_2.id = pool_id_2;
    pool_info2_2.timestamp = timestamp2;
    pool_infos.push(to_new_db(&pool_info2_2)?);

    let mut pool_info3_4 = test_pool_info.clone();
    pool_info3_4.id = pool_id_3;
    pool_info3_4.timestamp = timestamp4;
    pool_infos.push(to_new_db(&pool_info3_4)?);

    pool_infos.reverse();
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

    let results = get_all_unique_between(TimeRange {
        start: timestamp1,
        end: timestamp3,
    })
    .await?;

    assert_eq!(
        results,
        vec![pool_info1_3.clone(), pool_info2_2.clone()],
        "プールID 1と2が含まれていません"
    );

    let results2 = get_all_unique_between(TimeRange {
        start: timestamp2,
        end: timestamp4,
    })
    .await?;

    assert_eq!(
        results2,
        vec![
            pool_info1_3.clone(),
            pool_info2_2.clone(),
            pool_info3_4.clone()
        ],
        "プールIDユニークなデータは3件あるはずです"
    );

    let empty_results = get_all_unique_between(TimeRange {
        start: NaiveDateTime::parse_from_str("2022-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
        end: NaiveDateTime::parse_from_str("2022-12-31 23:59:59", "%Y-%m-%d %H:%M:%S").unwrap(),
    })
    .await?;
    assert!(
        empty_results.is_empty(),
        "データがない期間では空の配列が返されるべきです"
    );

    let results4 = get_all_unique_between(TimeRange {
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

#[tokio::test]
#[serial(pool_info)]
async fn test_cleanup_old_records() -> Result<()> {
    use chrono::NaiveDateTime;
    use diesel::Connection;
    use diesel::prelude::*;

    let conn = connection_pool::get().await?;

    match conn
        .interact(|conn| conn.transaction(|conn| diesel::delete(pool_info::table).execute(conn)))
        .await
    {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => return Err(anyhow!("Failed to clear table: {}", e)),
        Err(e) => return Err(anyhow!("Failed to interact with DB: {}", e)),
    };

    let base_time = NaiveDateTime::parse_from_str("2023-01-01 00:00:00", "%Y-%m-%d %H:%M:%S")?;
    let pool_id_1: u32 = 100;
    let pool_id_2: u32 = 200;

    let test_pool_info = create_test_pool_info();
    let mut pool_infos = Vec::new();

    for i in 0..15 {
        let mut pool_info = test_pool_info.clone();
        pool_info.id = pool_id_1;
        pool_info.timestamp = base_time + chrono::Duration::seconds(i);
        pool_infos.push(to_new_db(&pool_info)?);
    }

    for i in 0..5 {
        let mut pool_info = test_pool_info.clone();
        pool_info.id = pool_id_2;
        pool_info.timestamp = base_time + chrono::Duration::seconds(i);
        pool_infos.push(to_new_db(&pool_info)?);
    }

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

    cleanup_old_records(10).await?;

    let conn = connection_pool::get().await?;
    let count_pool_1 = conn
        .interact(move |conn| {
            use diesel::dsl::count;
            pool_info::table
                .filter(pool_info::pool_id.eq(pool_id_1 as i32))
                .select(count(pool_info::id))
                .first::<i64>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    assert_eq!(
        count_pool_1, 10,
        "プールID 100 のレコード数は10件であるべきです"
    );

    let conn = connection_pool::get().await?;
    let count_pool_2 = conn
        .interact(move |conn| {
            use diesel::dsl::count;
            pool_info::table
                .filter(pool_info::pool_id.eq(pool_id_2 as i32))
                .select(count(pool_info::id))
                .first::<i64>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    assert_eq!(
        count_pool_2, 5,
        "プールID 200 のレコード数は5件のままであるべきです"
    );

    let conn = connection_pool::get().await?;
    let latest_timestamp = conn
        .interact(move |conn| {
            use diesel::dsl::max;
            pool_info::table
                .filter(pool_info::pool_id.eq(pool_id_1 as i32))
                .select(max(pool_info::timestamp))
                .first::<Option<NaiveDateTime>>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    assert_eq!(
        latest_timestamp,
        Some(base_time + chrono::Duration::seconds(14)),
        "最新のタイムスタンプは14秒後であるべきです"
    );

    let conn = connection_pool::get().await?;
    let oldest_timestamp = conn
        .interact(move |conn| {
            use diesel::dsl::min;
            pool_info::table
                .filter(pool_info::pool_id.eq(pool_id_1 as i32))
                .select(min(pool_info::timestamp))
                .first::<Option<NaiveDateTime>>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    assert_eq!(
        oldest_timestamp,
        Some(base_time + chrono::Duration::seconds(5)),
        "最古のタイムスタンプは5秒後であるべきです（0-4秒が削除される）"
    );

    Ok(())
}
