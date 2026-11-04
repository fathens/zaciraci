use super::*;

#[tokio::test]
#[serial(ref_flow_db)]
async fn insert_and_read_back_by_tx_and_range() -> Result<()> {
    clean_tables().await?;
    seed_frame(1, 0).await?;
    seed_frame(2, 0).await?;
    let trades = vec![
        make_trade(1, 1, 100, chain_hops())
            .with_fees(vec![30, 5])
            .unwrap(),
        make_trade(2, 1, 50, vec![hop(3515, "wrap.near", "usdc.near")]),
        make_trade(
            3,
            2,
            200,
            vec![hop(1, "a.near", "b.near"), hop(2, "x.near", "b.near")],
        ),
    ];
    assert_eq!(insert_trades(&trades).await?, 3);

    let by_tx = get_by_tx_hash(&tx_hash(1)).await?;
    assert_eq!(by_tx.len(), 2);
    assert_eq!(by_tx[0], trades[1], "earlier receipt first");
    assert_eq!(by_tx[1], trades[0]);

    let range = TimeRange {
        start: ts(100),
        end: ts(300),
    };
    let in_range = get_in_time_range(&range).await?;
    assert_eq!(in_range, vec![trades[0].clone(), trades[2].clone()]);
    assert_eq!(in_range[1].shape(), dex::RouteShape::Split);
    Ok(())
}

async fn expect_insert_fails(row: NewDbRefTrade, what: &str) -> Result<()> {
    let result = insert_rows(vec![row]).await;
    assert!(result.is_err(), "{what}: insert must fail");
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn check_constraints_reject_invalid_rows() -> Result<()> {
    clean_tables().await?;
    seed_frame(1, 0).await?;
    let valid = to_new_db(&make_trade(1, 1, 0, chain_hops())).unwrap();

    let mut row = valid.clone();
    tweak(&mut row)
        .hop_tokens_in
        .push(Some("extra.near".to_owned()));
    expect_insert_fails(row, "hop array length mismatch").await?;

    let mut row = valid.clone();
    tweak(&mut row).pool_ids[0] = None;
    expect_insert_fails(row, "NULL pool_id element").await?;

    let mut row = valid.clone();
    tweak(&mut row).hop_amounts_in.pop();
    expect_insert_fails(row, "hop_amounts_in length mismatch").await?;

    let mut row = valid.clone();
    tweak(&mut row).hop_amounts_out[1] = None;
    expect_insert_fails(row, "NULL hop_amounts_out element").await?;

    let mut row = valid.clone();
    tweak(&mut row).hop_amounts_out[1] = Some(BigDecimal::from(-1));
    expect_insert_fails(row, "negative hop amount").await?;

    let mut row = valid.clone();
    *tweak(&mut row).token_in = "other.near".to_owned();
    expect_insert_fails(row, "token_in != hop_tokens_in[1]").await?;

    let mut row = valid.clone();
    *tweak(&mut row).token_out = "wrap.near".to_owned();
    expect_insert_fails(row, "token_out != last hop").await?;

    let mut row = valid.clone();
    *tweak(&mut row).fee_bps = Some(vec![Some(30)]);
    expect_insert_fails(row, "fee_bps length mismatch").await?;

    let mut row = valid.clone();
    *tweak(&mut row).fee_bps = Some(vec![Some(30), None]);
    expect_insert_fails(row, "fee_bps NULL element").await?;

    let mut row = valid.clone();
    *tweak(&mut row).fee_bps = Some(vec![Some(30), Some(10_000)]);
    expect_insert_fails(row, "fee_bps == 10000").await?;

    let mut row = valid.clone();
    *tweak(&mut row).route = "flash".to_owned();
    expect_insert_fails(row, "unknown route").await?;

    let mut row = valid.clone();
    *tweak(&mut row).amount_in = BigDecimal::from(0);
    expect_insert_fails(row, "amount_in == 0").await?;

    let mut row = valid.clone();
    *tweak(&mut row).amount_out = BigDecimal::from(u128::MAX) + BigDecimal::from(1);
    expect_insert_fails(row, "amount_out > u128::MAX").await?;

    // Boundary: u128::MAX itself is accepted.
    let mut row = valid.clone();
    *tweak(&mut row).amount_out = BigDecimal::from(u128::MAX);
    assert_eq!(insert_rows(vec![row]).await?, 1);
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn foreign_key_requires_frame_row_and_cascades_on_delete() -> Result<()> {
    clean_tables().await?;
    // No frame row for tx 9.
    let orphan = to_new_db(&make_trade(1, 9, 0, chain_hops())).unwrap();
    expect_insert_fails(orphan, "missing frame row").await?;

    seed_frame(9, 0).await?;
    insert_trades(&[make_trade(1, 9, 0, chain_hops())]).await?;
    assert_eq!(get_by_tx_hash(&tx_hash(9)).await?.len(), 1);

    let conn = connection_pool::get_test_only().await?;
    conn.interact(|conn| diesel::delete(crate::schema::ref_flow_txs::table).execute(conn))
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    assert!(get_by_tx_hash(&tx_hash(9)).await?.is_empty(), "CASCADE");
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn delete_by_tx_hash_removes_only_that_transaction() -> Result<()> {
    clean_tables().await?;
    seed_frame(1, 0).await?;
    seed_frame(2, 0).await?;
    insert_trades(&[
        make_trade(1, 1, 0, chain_hops()),
        make_trade(2, 1, 0, chain_hops()),
        make_trade(3, 2, 0, chain_hops()),
    ])
    .await?;
    let key = tx_hash(1).as_str().to_owned();
    let conn = connection_pool::get_test_only().await?;
    let deleted = conn
        .interact(move |conn| delete_by_tx_hash(&key, conn))
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    assert_eq!(deleted, 2);
    assert!(get_by_tx_hash(&tx_hash(1)).await?.is_empty());
    assert_eq!(get_by_tx_hash(&tx_hash(2)).await?.len(), 1);
    Ok(())
}
