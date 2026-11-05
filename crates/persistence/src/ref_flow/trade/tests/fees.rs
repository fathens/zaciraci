use super::*;

/// Pool ids far above anything the other pool_info tests use.
const POOL_A: u32 = 900_001;
const POOL_B: u32 = 900_002;

async fn clear_fee_pools() -> Result<()> {
    let conn = connection_pool::get_test_only().await?;
    conn.interact(|conn| {
        diesel::delete(pool_info::table.filter(pool_info::pool_id.ge(900_000))).execute(conn)
    })
    .await
    .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    Ok(())
}

/// Insert one pool_info snapshot directly. `pool_info::batch_insert` is
/// avoided on purpose: it spawns the retention DELETE over the whole
/// (multi-million row) table, which makes these tests slow and flaky.
async fn snapshot(pool_id: u32, fee: u32, at: NaiveDateTime) -> Result<()> {
    let conn = connection_pool::get_test_only().await?;
    conn.interact(move |conn| {
        diesel::insert_into(pool_info::table)
            .values((
                pool_info::pool_id.eq(pool_id as i32),
                pool_info::pool_kind.eq("SIMPLE_POOL"),
                pool_info::token_account_ids.eq(serde_json::json!(["a.near", "b.near"])),
                pool_info::amounts.eq(serde_json::json!(["1", "1"])),
                pool_info::total_fee.eq(fee as i32),
                pool_info::shares_total_supply.eq(serde_json::json!("1")),
                pool_info::amp.eq(0i64),
                pool_info::timestamp.eq(at),
            ))
            .execute(conn)
    })
    .await
    .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db, pool_info)]
async fn picks_latest_snapshot_at_or_before_block_time() -> Result<()> {
    clear_fee_pools().await?;
    // Timestamps relative to now so the pool_info retention cleanup never
    // removes them mid-test.
    let now = chrono::Utc::now().naive_utc();
    let h = |n: i64| now - chrono::TimeDelta::hours(n);
    snapshot(POOL_A, 30, h(3)).await?;
    snapshot(POOL_A, 40, h(1)).await?;
    snapshot(POOL_A, 50, now + chrono::TimeDelta::hours(1)).await?;
    let hops = vec![hop(POOL_A, "a.near", "b.near")];

    assert_eq!(lookup_hop_fees(&hops, now).await?, Some(vec![40]));
    assert_eq!(lookup_hop_fees(&hops, h(2)).await?, Some(vec![30]));
    // Exactly at the snapshot time counts ("at or before").
    assert_eq!(lookup_hop_fees(&hops, h(1)).await?, Some(vec![40]));
    // Before the first snapshot: unknown.
    assert_eq!(lookup_hop_fees(&hops, h(4)).await?, None);
    clear_fee_pools().await
}

#[tokio::test]
#[serial(ref_flow_db, pool_info)]
async fn missing_pool_makes_the_whole_fee_list_unknown() -> Result<()> {
    clear_fee_pools().await?;
    let now = chrono::Utc::now().naive_utc();
    snapshot(POOL_A, 30, now - chrono::TimeDelta::hours(1)).await?;
    let hops = vec![
        hop(POOL_A, "a.near", "b.near"),
        hop(POOL_B, "b.near", "c.near"),
    ];
    assert_eq!(lookup_hop_fees(&hops, now).await?, None);

    snapshot(POOL_B, 7, now - chrono::TimeDelta::minutes(30)).await?;
    assert_eq!(lookup_hop_fees(&hops, now).await?, Some(vec![30, 7]));
    clear_fee_pools().await
}

#[tokio::test]
#[serial(ref_flow_db, pool_info)]
async fn pool_fees_at_returns_the_latest_snapshot_per_known_pool() -> Result<()> {
    clear_fee_pools().await?;
    let now = chrono::Utc::now().naive_utc();
    let h = |n: i64| now - chrono::TimeDelta::hours(n);
    snapshot(POOL_A, 30, h(3)).await?;
    snapshot(POOL_A, 40, h(1)).await?;
    snapshot(POOL_B, 7, h(2)).await?;
    let fees = lookup_pool_fees_at(&[POOL_A, POOL_B, 900_003], now).await?;
    assert_eq!(fees.get(&POOL_A), Some(&40));
    assert_eq!(fees.get(&POOL_B), Some(&7));
    assert!(!fees.contains_key(&900_003), "no snapshot: absent");
    // Only snapshots at or before `at` count.
    let fees = lookup_pool_fees_at(&[POOL_A, POOL_B], h(2)).await?;
    assert_eq!(fees.get(&POOL_A), Some(&30));
    assert_eq!(fees.get(&POOL_B), Some(&7));
    assert!(lookup_pool_fees_at(&[], now).await?.is_empty());
    clear_fee_pools().await
}
