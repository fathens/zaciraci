use super::*;
use crate::ref_flow::receipt::{RefReceipt, RefReceiptParts, ingest_receipts};
use crate::ref_flow::test_ids::*;
use crate::ref_flow::test_support::*;
use crate::ref_flow::trade::tests::{chain_hops, hop, make_trade};
use serial_test::serial;

/// Create the frame row of transaction `tx` at `ts(secs)`.
async fn seed(tx: u32, secs: i64) -> Result<()> {
    let receipt = RefReceipt::new(RefReceiptParts {
        receipt_id: receipt_id(700_000 + tx),
        tx_hash: tx_hash(tx),
        block_height: 1,
        block_time: ts(secs),
        predecessor: "seed.near".parse().unwrap(),
        methods: vec!["swap".to_owned()],
        ok: true,
    })
    .unwrap();
    ingest_receipts(&[receipt]).await?;
    Ok(())
}

async fn record(
    tx: u32,
    trades: &[Trade],
    policy: ReplacePolicy,
    secs: i64,
) -> Result<EnrichmentOutcome> {
    record_enrichment(&tx_hash(tx), trades, policy, ts(secs)).await
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn record_enrichment_replaces_trades_and_marks_once() -> Result<()> {
    clean_tables().await?;
    seed(1, 0).await?;
    let first = vec![
        make_trade(1, 1, 10, chain_hops()),
        make_trade(2, 1, 20, chain_hops()),
    ];
    assert_eq!(
        record(1, &first, ReplacePolicy::Always, 1000).await?,
        EnrichmentOutcome::Recorded { trades: 2 }
    );
    let rec = get(&tx_hash(1)).await?.unwrap();
    assert_eq!(rec.enriched_at, Some(ts(1000)));
    assert_eq!(rec.trade_count, Some(2));
    assert!(rec.is_enriched());

    // Re-fetch with a different result: trades are replaced, enriched_at
    // keeps the first timestamp, trade_count follows the new write.
    let second = vec![make_trade(3, 1, 30, vec![hop(9, "x.near", "y.near")])];
    assert_eq!(
        record(1, &second, ReplacePolicy::Always, 2000).await?,
        EnrichmentOutcome::Recorded { trades: 1 }
    );
    let rec = get(&tx_hash(1)).await?.unwrap();
    assert_eq!(rec.enriched_at, Some(ts(1000)));
    assert_eq!(rec.trade_count, Some(1));
    let stored = trade::get_by_tx_hash(&tx_hash(1)).await?;
    assert_eq!(stored, second);

    // Zero trades is a valid enrichment result.
    assert_eq!(
        record(1, &[], ReplacePolicy::Always, 3000).await?,
        EnrichmentOutcome::Recorded { trades: 0 }
    );
    let rec = get(&tx_hash(1)).await?.unwrap();
    assert_eq!(rec.trade_count, Some(0));
    assert!(trade::get_by_tx_hash(&tx_hash(1)).await?.is_empty());
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn only_if_pending_skips_already_enriched_transactions() -> Result<()> {
    clean_tables().await?;
    seed(1, 0).await?;
    let live = vec![make_trade(1, 1, 10, chain_hops())];
    assert_eq!(
        record(1, &live, ReplacePolicy::OnlyIfPending, 100).await?,
        EnrichmentOutcome::Recorded { trades: 1 }
    );
    let imported = vec![make_trade(2, 1, 10, chain_hops())];
    assert_eq!(
        record(1, &imported, ReplacePolicy::OnlyIfPending, 200).await?,
        EnrichmentOutcome::Skipped
    );
    assert_eq!(
        trade::get_by_tx_hash(&tx_hash(1)).await?,
        live,
        "live rows win"
    );
    assert_eq!(get(&tx_hash(1)).await?.unwrap().enriched_at, Some(ts(100)));
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn record_enrichment_rejects_missing_frame_and_foreign_trades() -> Result<()> {
    clean_tables().await?;
    assert_eq!(
        record(5, &[], ReplacePolicy::Always, 0).await?,
        EnrichmentOutcome::NotInFrame
    );
    seed(5, 0).await?;
    // A trade of another transaction must not be written under tx 5.
    let foreign = vec![make_trade(1, 6, 0, chain_hops())];
    assert!(record(5, &foreign, ReplacePolicy::Always, 0).await.is_err());
    assert!(!get(&tx_hash(5)).await?.unwrap().is_enriched());
    assert_eq!(count_txs().await?, 1);
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn failed_attempts_accumulate_and_isolate_at_max() -> Result<()> {
    clean_tables().await?;
    assert_eq!(mark_failed_attempt(&tx_hash(1), 3, ts(0)).await?, None);
    seed(1, 0).await?;
    assert_eq!(
        mark_failed_attempt(&tx_hash(1), 3, ts(1)).await?,
        Some(FailureOutcome {
            attempts: 1,
            isolated: false
        })
    );
    assert_eq!(
        mark_failed_attempt(&tx_hash(1), 3, ts(2)).await?,
        Some(FailureOutcome {
            attempts: 2,
            isolated: false
        })
    );
    assert!(!get(&tx_hash(1)).await?.unwrap().is_enriched());
    assert_eq!(
        mark_failed_attempt(&tx_hash(1), 3, ts(3)).await?,
        Some(FailureOutcome {
            attempts: 3,
            isolated: true
        })
    );
    let rec = get(&tx_hash(1)).await?.unwrap();
    assert_eq!(rec.enriched_at, Some(ts(3)));
    assert_eq!(rec.trade_count, Some(0));
    assert_eq!(rec.attempts, 3);
    // Isolated transactions no longer appear as pending.
    assert!(pending_candidates(ts(-1), 10).await?.is_empty());
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn pending_candidates_are_newest_first_filtered_and_limited() -> Result<()> {
    clean_tables().await?;
    for (tx, secs) in [(1, 100), (2, 400), (3, 200), (4, 300)] {
        seed(tx, secs).await?;
    }
    record(3, &[], ReplacePolicy::Always, 999).await?;

    let all = pending_candidates(ts(0), 10).await?;
    let order: Vec<u32> = all
        .iter()
        .map(|p| match p.block_time {
            t if t == ts(400) => 2,
            t if t == ts(300) => 4,
            t if t == ts(100) => 1,
            _ => 0,
        })
        .collect();
    assert_eq!(order, vec![2, 4, 1], "newest first, enriched tx 3 excluded");
    assert_eq!(all[0].tx_hash, tx_hash(2));

    let limited = pending_candidates(ts(0), 2).await?;
    assert_eq!(limited.len(), 2);

    let recent = pending_candidates(ts(250), 10).await?;
    assert_eq!(recent.len(), 2, "not_before excludes tx 1 at ts(100)");
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn reset_enrichment_clears_range_and_deletes_trades() -> Result<()> {
    clean_tables().await?;
    seed(1, 100).await?;
    seed(2, 500).await?;
    record(
        1,
        &[make_trade(1, 1, 100, chain_hops())],
        ReplacePolicy::Always,
        1,
    )
    .await?;
    record(
        2,
        &[make_trade(2, 2, 500, chain_hops())],
        ReplacePolicy::Always,
        1,
    )
    .await?;
    mark_failed_attempt(&tx_hash(1), 10, ts(2)).await?;

    let reset = reset_enrichment(&TimeRange {
        start: ts(0),
        end: ts(300),
    })
    .await?;
    assert_eq!(reset, 1);
    let rec = get(&tx_hash(1)).await?.unwrap();
    assert_eq!(rec.enriched_at, None);
    assert_eq!(rec.trade_count, None);
    assert_eq!(rec.attempts, 0);
    assert!(trade::get_by_tx_hash(&tx_hash(1)).await?.is_empty());
    // Outside the range: untouched.
    assert!(get(&tx_hash(2)).await?.unwrap().is_enriched());
    assert_eq!(trade::get_by_tx_hash(&tx_hash(2)).await?.len(), 1);
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn coverage_by_bucket_counts_total_and_enriched() -> Result<()> {
    clean_tables().await?;
    // Bucket A: three txs at 0, 60 and 1700 s; bucket B: one tx at 1900 s.
    for (tx, secs) in [(1, 0), (2, 60), (3, 1700), (4, 1900)] {
        seed(tx, secs).await?;
    }
    record(1, &[], ReplacePolicy::Always, 1).await?;
    record(4, &[], ReplacePolicy::Always, 1).await?;

    let rows = coverage_by_bucket(
        &TimeRange {
            start: ts(0),
            end: ts(3600),
        },
        30,
    )
    .await?;
    assert_eq!(rows.len(), 2);
    assert_eq!((rows[0].total, rows[0].enriched), (3, 1));
    assert_eq!((rows[1].total, rows[1].enriched), (1, 1));
    assert_eq!(
        rows[1].bucket_start - rows[0].bucket_start,
        chrono::TimeDelta::minutes(30)
    );
    assert!(
        rows[0].bucket_start <= ts(0)
            && ts(0) < rows[0].bucket_start + chrono::TimeDelta::minutes(30)
    );

    assert!(
        coverage_by_bucket(
            &TimeRange {
                start: ts(0),
                end: ts(1)
            },
            0
        )
        .await
        .is_err()
    );
    Ok(())
}
