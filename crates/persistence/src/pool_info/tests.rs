use super::*;
use common::config::ConfigResolver;
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

    let cfg = ConfigResolver;
    batch_insert(&[Arc::new(pool_info1), Arc::new(pool_info2)], &cfg).await?;

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
    {
        let conn = connection_pool::get().await?;
        conn.interact(move |conn| {
            diesel::insert_into(pool_info::table)
                .values(&new_pool)
                .execute(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    }

    // 1秒待機して新しいタイムスタンプでデータを更新
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    let mut updated_pool_info = pool_info.clone();
    updated_pool_info.bare.pool_kind = "WEIGHTED_SWAP".to_string();
    updated_pool_info.timestamp = chrono::Utc::now().naive_utc();

    let new_pool = to_new_db(&updated_pool_info)?;
    {
        let conn = connection_pool::get().await?;
        conn.interact(move |conn| {
            diesel::insert_into(pool_info::table)
                .values(&new_pool)
                .execute(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    }

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
    drop(conn);

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
    drop(conn);

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

/// 日数ベースの cleanup: 指定日数より古いレコードが削除されることを検証
#[tokio::test]
#[serial(pool_info)]
async fn test_cleanup_old_records_by_days() -> Result<()> {
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

    let now = chrono::Utc::now().naive_utc();
    let pool_id_1: u32 = 100;
    let pool_id_2: u32 = 200;

    let test_pool_info = create_test_pool_info();
    let mut pool_infos = Vec::new();

    // pool_id_1: 40日前、20日前、10日前、1日前のレコード
    for days_ago in [40, 20, 10, 1] {
        let mut pi = test_pool_info.clone();
        pi.id = pool_id_1;
        pi.timestamp = now - chrono::TimeDelta::days(days_ago);
        pool_infos.push(to_new_db(&pi)?);
    }

    // pool_id_2: 50日前、5日前のレコード
    for days_ago in [50, 5] {
        let mut pi = test_pool_info.clone();
        pi.id = pool_id_2;
        pi.timestamp = now - chrono::TimeDelta::days(days_ago);
        pool_infos.push(to_new_db(&pi)?);
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
    drop(conn);

    // 30日以上古いレコードを削除
    cleanup_old_records(30).await?;

    // pool_id_1: 40日前が削除され、20日前・10日前・1日前の3件が残る
    let count_pool_1 = {
        let conn = connection_pool::get().await?;
        conn.interact(move |conn| {
            use diesel::dsl::count;
            pool_info::table
                .filter(pool_info::pool_id.eq(pool_id_1 as i32))
                .select(count(pool_info::id))
                .first::<i64>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??
    };

    assert_eq!(
        count_pool_1, 3,
        "pool_id 100: 40日前のレコードが削除され3件残るべき"
    );

    // pool_id_2: 50日前が削除され、5日前の1件が残る
    let count_pool_2 = {
        let conn = connection_pool::get().await?;
        conn.interact(move |conn| {
            use diesel::dsl::count;
            pool_info::table
                .filter(pool_info::pool_id.eq(pool_id_2 as i32))
                .select(count(pool_info::id))
                .first::<i64>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??
    };

    assert_eq!(
        count_pool_2, 1,
        "pool_id 200: 50日前のレコードが削除され1件残るべき"
    );

    // 最古のタイムスタンプが20日前であることを確認
    let oldest_timestamp = {
        let conn = connection_pool::get().await?;
        conn.interact(move |conn| {
            use diesel::dsl::min;
            pool_info::table
                .filter(pool_info::pool_id.eq(pool_id_1 as i32))
                .select(min(pool_info::timestamp))
                .first::<Option<NaiveDateTime>>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??
    };

    let expected_oldest = now - chrono::TimeDelta::days(20);
    let diff = (oldest_timestamp.unwrap() - expected_oldest)
        .num_seconds()
        .abs();
    assert!(
        diff < 2,
        "最古のレコードは約20日前であるべき (差: {}秒)",
        diff
    );

    Ok(())
}

/// 境界値テスト: ちょうど retention_days 前のレコードは削除される（timestamp < cutoff）
#[tokio::test]
#[serial(pool_info)]
async fn test_cleanup_old_records_boundary() -> Result<()> {
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

    let now = chrono::Utc::now().naive_utc();
    let pool_id: u32 = 300;

    let test_pool_info = create_test_pool_info();
    let mut pool_infos = Vec::new();

    // ちょうど30日前（秒単位のズレを考慮して1秒前）→ 削除されるべき
    let mut pi_exactly = test_pool_info.clone();
    pi_exactly.id = pool_id;
    pi_exactly.timestamp = now - chrono::TimeDelta::days(30) - chrono::TimeDelta::seconds(1);
    pool_infos.push(to_new_db(&pi_exactly)?);

    // 30日より少し新しい → 残るべき
    let mut pi_just_inside = test_pool_info.clone();
    pi_just_inside.id = pool_id;
    pi_just_inside.timestamp = now - chrono::TimeDelta::days(30) + chrono::TimeDelta::minutes(1);
    pool_infos.push(to_new_db(&pi_just_inside)?);

    // 1日前 → 確実に残る
    let mut pi_recent = test_pool_info.clone();
    pi_recent.id = pool_id;
    pi_recent.timestamp = now - chrono::TimeDelta::days(1);
    pool_infos.push(to_new_db(&pi_recent)?);

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
    drop(conn);

    cleanup_old_records(30).await?;

    let count = {
        let conn = connection_pool::get().await?;
        conn.interact(move |conn| {
            use diesel::dsl::count;
            pool_info::table
                .filter(pool_info::pool_id.eq(pool_id as i32))
                .select(count(pool_info::id))
                .first::<i64>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??
    };

    assert_eq!(
        count, 2,
        "ちょうど30日前のレコードは削除され、30日未満の2件が残るべき"
    );

    Ok(())
}

/// retention_days が MIN_RETENTION_DAYS 未満の場合、最小値に引き上げられることを検証
#[tokio::test]
#[serial(pool_info)]
async fn test_cleanup_old_records_minimum_retention() -> Result<()> {
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

    let now = chrono::Utc::now().naive_utc();
    let pool_id: u32 = 400;

    let test_pool_info = create_test_pool_info();
    let mut pool_infos = Vec::new();

    // 10日前 → MIN_RETENTION_DAYS(7) より古いが、retention_days=1 で呼んでも
    // 実効値が7日に引き上げられるため削除されない
    let mut pi_10days = test_pool_info.clone();
    pi_10days.id = pool_id;
    pi_10days.timestamp = now - chrono::TimeDelta::days(10);
    pool_infos.push(to_new_db(&pi_10days)?);

    // 3日前 → 確実に残る
    let mut pi_3days = test_pool_info.clone();
    pi_3days.id = pool_id;
    pi_3days.timestamp = now - chrono::TimeDelta::days(3);
    pool_infos.push(to_new_db(&pi_3days)?);

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
    drop(conn);

    // retention_days=1 で呼び出すが、MIN_RETENTION_DAYS=7 に引き上げられる
    // → 7日以上古いレコード（10日前）のみ削除、3日前は残る
    cleanup_old_records(1).await?;

    let count = {
        let conn = connection_pool::get().await?;
        conn.interact(move |conn| {
            use diesel::dsl::count;
            pool_info::table
                .filter(pool_info::pool_id.eq(pool_id as i32))
                .select(count(pool_info::id))
                .first::<i64>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??
    };

    assert_eq!(
        count, 1,
        "retention_days=1 でも MIN_RETENTION_DAYS=7 が適用され、10日前のレコードのみ削除、3日前は残るべき"
    );

    Ok(())
}
