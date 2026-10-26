use super::*;
use crate::ref_flow::receipt::{RefReceipt, RefReceiptParts, ingest_receipts};
use crate::ref_flow::test_support::*;
use crate::ref_flow::trade::tests::{chain_hops, make_trade};
use crate::ref_flow::tx::{ReplacePolicy, record_enrichment};
use common::types::TimeRange;
use serial_test::serial;

/// One successful receipt (and frame row) for tx `n` at `at`.
async fn seed_at(n: u32, at: NaiveDateTime) -> Result<()> {
    let receipt = RefReceipt::new(RefReceiptParts {
        receipt_id: receipt_id(800_000 + n),
        tx_hash: tx_hash(n),
        block_height: 1,
        block_time: at,
        predecessor: "seed.near".parse().unwrap(),
        methods: vec!["swap".to_owned()],
        ok: true,
    })
    .unwrap();
    ingest_receipts(&[receipt]).await?;
    Ok(())
}

fn days_ago(n: i64) -> NaiveDateTime {
    chrono::Utc::now().naive_utc() - chrono::TimeDelta::days(n)
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn retention_zero_skips_and_short_retention_is_clamped() -> Result<()> {
    clean_tables().await?;
    seed_at(1, days_ago(10)).await?;
    seed_at(2, days_ago(40)).await?;

    assert_eq!(cleanup_old_records(0).await?, CleanupStats::default());
    assert_eq!(count_txs().await?, 2);

    // 5 days is clamped to MIN_RETENTION_DAYS (31): the 10-day-old row
    // survives, the 40-day-old row goes.
    let stats = cleanup_old_records(5).await?;
    assert_eq!(
        stats,
        CleanupStats {
            txs_deleted: 1,
            receipts_deleted: 1
        }
    );
    assert_eq!(count_txs().await?, 1);
    assert_eq!(count_receipts().await?, 1);
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn cleanup_batches_until_done_and_cascades_trades() -> Result<()> {
    clean_tables().await?;
    for n in 1..=3 {
        seed_at(n, ts(i64::from(n))).await?;
    }
    seed_at(4, ts(10_000)).await?;
    record_enrichment(
        &tx_hash(1),
        &[make_trade(1, 1, 1, chain_hops())],
        ReplacePolicy::Always,
        ts(0),
    )
    .await?;
    record_enrichment(
        &tx_hash(4),
        &[make_trade(4, 4, 10_000, chain_hops())],
        ReplacePolicy::Always,
        ts(0),
    )
    .await?;

    // Batch size 1 forces the loop to run three times per table.
    let stats = cleanup_before(ts(5_000), 1).await?;
    assert_eq!(
        stats,
        CleanupStats {
            txs_deleted: 3,
            receipts_deleted: 3
        }
    );
    assert_eq!(count_txs().await?, 1);
    assert_eq!(count_receipts().await?, 1);
    assert!(
        trade::get_by_tx_hash(&tx_hash(1)).await?.is_empty(),
        "CASCADE"
    );
    assert_eq!(
        trade::get_in_time_range(&TimeRange {
            start: ts(0),
            end: ts(20_000)
        })
        .await?
        .len(),
        1
    );
    Ok(())
}
