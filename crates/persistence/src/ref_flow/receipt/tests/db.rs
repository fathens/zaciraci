use super::*;

#[tokio::test]
#[serial(ref_flow_db)]
async fn ingest_empty_batch_is_a_noop() -> Result<()> {
    clean_tables().await?;
    let stats = ingest_receipts(&[]).await?;
    assert_eq!(stats, IngestStats::default());
    assert_eq!(count_receipts().await?, 0);
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn production_ingest_and_lookup_by_tx_hash() -> Result<()> {
    clean_tables().await?;
    let receipts = vec![
        make_receipt(1, 7, 10, true),
        make_receipt(2, 7, 5, false),
        make_receipt(3, 8, 20, true),
    ];
    let stats = ingest_receipts(&receipts).await?;
    assert_eq!(stats.receipts_inserted, 3);
    assert_eq!(stats.txs_upserted, 2);

    let by_tx = get_by_tx_hash(&tx_hash(7)).await?;
    assert_eq!(by_tx.len(), 2);
    // Ordered by block height (receipt 1 has the lower height).
    assert_eq!(by_tx[0].receipt_id, receipt_id(1));
    assert_eq!(by_tx[1].receipt_id, receipt_id(2));
    assert!(!by_tx[1].ok);

    assert!(get_by_tx_hash(&tx_hash(99)).await?.is_empty());
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn time_range_is_half_open_and_ordered() -> Result<()> {
    clean_tables().await?;
    let receipts = vec![
        make_receipt(1, 1, 100, true),
        make_receipt(2, 2, 200, true),
        make_receipt(3, 3, 300, true),
        make_receipt(4, 4, 400, true),
    ];
    ingest_receipts(&receipts).await?;

    let range = TimeRange {
        start: ts(200),
        end: ts(400),
    };
    let rows = get_in_time_range(&range).await?;
    let ids: Vec<ReceiptId> = rows.into_iter().map(|r| r.receipt_id).collect();
    assert_eq!(ids, vec![receipt_id(2), receipt_id(3)]);
    Ok(())
}
