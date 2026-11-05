use super::*;
use chrono::Timelike;

use crate::ref_flow::test_support::{
    FEE_FIXTURE_POOL_ID_MIN, clear_pool_fee_snapshots, insert_pool_fee_snapshot,
};

/// Pool ids far above anything the other pool_info tests use.
const POOL_A: u32 = FEE_FIXTURE_POOL_ID_MIN + 1;
const POOL_B: u32 = FEE_FIXTURE_POOL_ID_MIN + 2;

async fn clear_fee_pools() -> Result<()> {
    clear_pool_fee_snapshots().await
}

async fn snapshot(pool_id: u32, fee: u32, at: NaiveDateTime) -> Result<()> {
    insert_pool_fee_snapshot(pool_id, fee, at).await
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

fn point(pool_id: u32, at: NaiveDateTime) -> PoolFeePoint {
    PoolFeePoint { pool_id, at }
}

#[tokio::test]
#[serial(ref_flow_db, pool_info)]
async fn pool_fees_at_resolve_each_point_at_its_own_instant_in_input_order() -> Result<()> {
    clear_fee_pools().await?;
    let now = chrono::Utc::now().naive_utc();
    let h = |n: i64| now - chrono::TimeDelta::hours(n);
    snapshot(POOL_A, 30, h(3)).await?;
    snapshot(POOL_A, 40, h(1)).await?;
    snapshot(POOL_B, 7, h(2)).await?;
    let fees = lookup_pool_fees_at(&[
        point(POOL_A, now),
        point(POOL_A, h(2)), // between the two snapshots: the older fee
        point(POOL_B, h(3)), // before POOL_B's first snapshot
        point(900_003, now), // unknown pool
        point(POOL_B, now),
        point(POOL_A, h(1)), // exactly at a snapshot counts
    ])
    .await?;
    assert_eq!(
        fees,
        vec![Some(40), Some(30), None, None, Some(7), Some(40)]
    );
    assert!(lookup_pool_fees_at(&[]).await?.is_empty());
    clear_fee_pools().await
}

#[tokio::test]
#[serial(ref_flow_db, pool_info)]
async fn pool_fees_at_survive_nanosecond_block_times() -> Result<()> {
    // Block times carry nanoseconds; `timestamp` has microseconds. The
    // result is matched by ordinality, so the precision loss on the bound
    // value must not lose the fee.
    clear_fee_pools().await?;
    let now = chrono::Utc::now().naive_utc();
    snapshot(POOL_A, 30, now - chrono::TimeDelta::hours(1)).await?;
    let ns = (now - chrono::TimeDelta::minutes(30))
        .with_nanosecond(123_456_789)
        .unwrap();
    assert_eq!(ns.and_utc().timestamp_subsec_nanos(), 123_456_789);
    assert_eq!(
        lookup_pool_fees_at(&[point(POOL_A, ns)]).await?,
        vec![Some(30)]
    );
    let hops = vec![hop(POOL_A, "a.near", "b.near")];
    assert_eq!(lookup_hop_fees(&hops, ns).await?, Some(vec![30]));
    clear_fee_pools().await
}

#[test]
fn all_hop_fees_is_all_or_nothing() {
    assert_eq!(all_hop_fees(&[Some(1), Some(2)]), Some(vec![1, 2]));
    assert_eq!(all_hop_fees(&[Some(1), None]), None);
    assert_eq!(all_hop_fees(&[]), Some(vec![]));
}
