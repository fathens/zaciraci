use super::*;
use std::num::NonZeroUsize;

/// Run `ingest_chunked_with` with tiny chunks: 2 receipts per chunk and
/// 1 frame row per chunk, so 5 receipts span 3 chunks.
async fn ingest_small_chunks(receipts: &[RefReceipt]) -> Result<IngestStats> {
    let (rows, txs) = prepare_rows(receipts)?;
    let receipt_chunk =
        crate::batch::chunk_rows_with_budget(NonZeroUsize::new(14).unwrap(), NewDbRefReceipt::COLS);
    let tx_chunk =
        crate::batch::chunk_rows_with_budget(NonZeroUsize::new(2).unwrap(), NewDbRefTx::COLS);
    assert_eq!(receipt_chunk.get(), 2);
    assert_eq!(tx_chunk.get(), 1);
    let conn = connection_pool::get_test_only().await?;
    let stats = conn
        .interact(move |conn| ingest_chunked_with(rows, txs, receipt_chunk, tx_chunk, conn))
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    Ok(stats)
}

/// Multi-chunk happy path with positional assertions: each row carries a
/// distinct predecessor, height, time and ok flag so a chunk-boundary
/// column swap would be detected.
#[tokio::test]
#[serial(ref_flow_db)]
async fn multi_chunk_ingest_round_trips_every_field() -> Result<()> {
    clean_tables().await?;
    let receipts: Vec<RefReceipt> = (1..=5)
        .map(|n| {
            RefReceipt::new(RefReceiptParts {
                receipt_id: receipt_id(n),
                tx_hash: tx_hash(n),
                block_height: 100 + u64::from(n),
                block_time: ts(i64::from(n) * 60),
                predecessor: format!("p{n}.near").parse().unwrap(),
                methods: vec!["swap".to_owned(), format!("m{n}")],
                ok: n % 2 == 1,
            })
            .unwrap()
        })
        .collect();

    let stats = ingest_small_chunks(&receipts).await?;
    assert_eq!(stats.receipts_inserted, 5);
    assert_eq!(
        stats.txs_upserted, 3,
        "only ok receipts (n=1,3,5) get frame rows"
    );

    let range = TimeRange {
        start: ts(0),
        end: ts(10_000),
    };
    let stored = get_in_time_range(&range).await?;
    assert_eq!(stored.len(), 5);
    for (n, r) in (1..=5u32).zip(&stored) {
        assert_eq!(r.receipt_id, receipt_id(n), "row {n}");
        assert_eq!(r.tx_hash, tx_hash(n), "row {n}");
        assert_eq!(r.block_height, 100 + u64::from(n), "row {n}");
        assert_eq!(r.block_time, ts(i64::from(n) * 60), "row {n}");
        assert_eq!(r.predecessor.as_str(), format!("p{n}.near"), "row {n}");
        assert_eq!(r.methods(), ["swap".to_owned(), format!("m{n}")], "row {n}");
        assert_eq!(r.ok, n % 2 == 1, "row {n}");
    }
    assert_eq!(count_txs().await?, 3);
    Ok(())
}

/// Re-ingesting the same batch (and a batch that partly overlaps) inserts
/// nothing new; counts stay put across chunk boundaries.
#[tokio::test]
#[serial(ref_flow_db)]
async fn re_ingest_is_idempotent() -> Result<()> {
    clean_tables().await?;
    let receipts: Vec<RefReceipt> = (1..=5).map(|n| make_receipt(n, n, 0, true)).collect();
    let first = ingest_small_chunks(&receipts).await?;
    assert_eq!(first.receipts_inserted, 5);

    let second = ingest_small_chunks(&receipts).await?;
    assert_eq!(second.receipts_inserted, 0);
    assert_eq!(count_receipts().await?, 5);
    assert_eq!(count_txs().await?, 5);

    // Overlap: receipts 4..=7, of which 6 and 7 are new.
    let overlap: Vec<RefReceipt> = (4..=7).map(|n| make_receipt(n, n, 0, true)).collect();
    let third = ingest_small_chunks(&overlap).await?;
    assert_eq!(third.receipts_inserted, 2);
    assert_eq!(count_receipts().await?, 7);
    assert_eq!(count_txs().await?, 7);
    Ok(())
}

/// A transaction with several receipts gets exactly one frame row whose
/// block_time is the earliest receipt time, even when the earlier receipt
/// arrives in a later batch (gap fill).
#[tokio::test]
#[serial(ref_flow_db)]
async fn frame_row_keeps_earliest_receipt_time_across_batches() -> Result<()> {
    clean_tables().await?;
    ingest_small_chunks(&[
        make_receipt(1, 42, 300, true),
        make_receipt(2, 42, 200, true),
    ])
    .await?;
    assert_eq!(count_txs().await?, 1);
    assert_eq!(frame_time(&tx_hash(42)).await?, ts(200));

    // Later batch lists an older receipt of the same transaction.
    ingest_small_chunks(&[make_receipt(3, 42, 100, true)]).await?;
    assert_eq!(count_txs().await?, 1);
    assert_eq!(frame_time(&tx_hash(42)).await?, ts(100));

    // A newer receipt never moves the frame time forward.
    ingest_small_chunks(&[make_receipt(4, 42, 900, true)]).await?;
    assert_eq!(frame_time(&tx_hash(42)).await?, ts(100));
    assert_eq!(get_by_tx_hash(&tx_hash(42)).await?.len(), 4);
    Ok(())
}

async fn frame_time(tx: &TxHash) -> Result<NaiveDateTime> {
    let key = tx.as_str().to_owned();
    let conn = connection_pool::get_test_only().await?;
    let t = conn
        .interact(move |conn| {
            ref_flow_txs::table
                .filter(ref_flow_txs::tx_hash.eq(key))
                .select(ref_flow_txs::block_time)
                .get_result::<NaiveDateTime>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    Ok(t)
}

/// A CHECK violation in a later chunk rolls back the whole batch,
/// including the frame rows written before it.
#[tokio::test]
#[serial(ref_flow_db)]
async fn constraint_violation_in_later_chunk_rolls_back_everything() -> Result<()> {
    clean_tables().await?;
    let mut receipts: Vec<RefReceipt> = (1..=4).map(|n| make_receipt(n, n, 0, true)).collect();
    // Build an invalid DB row directly: NULL method element violates the CHECK.
    let (mut rows, txs) = prepare_rows(&receipts)?;
    rows[3].methods = vec![None];
    receipts.clear();
    let receipt_chunk =
        crate::batch::chunk_rows_with_budget(NonZeroUsize::new(14).unwrap(), NewDbRefReceipt::COLS);
    let tx_chunk = NewDbRefTx::CHUNK_ROWS;
    let conn = connection_pool::get_test_only().await?;
    let result = conn
        .interact(move |conn| ingest_chunked_with(rows, txs, receipt_chunk, tx_chunk, conn))
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))?;
    assert!(result.is_err(), "CHECK violation must surface as an error");
    assert_eq!(count_receipts().await?, 0, "chunk 1 must be rolled back");
    assert_eq!(count_txs().await?, 0);
    Ok(())
}
