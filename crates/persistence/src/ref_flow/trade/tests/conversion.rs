use super::*;

#[test]
fn round_trip_preserves_every_field() {
    let trade = make_trade(1, 1, 0, chain_hops())
        .with_fees(vec![30, 5])
        .unwrap();
    let row = to_new_db(&trade).unwrap();
    assert_eq!(row.pool_ids, vec![Some(6458), Some(5516)]);
    assert_eq!(
        row.hop_tokens_in,
        vec![
            Some("wrap.near".to_owned()),
            Some("token.rhealab.near".to_owned())
        ]
    );
    assert_eq!(row.fee_bps, Some(vec![Some(30), Some(5)]));
    assert_eq!(row.token_in, "wrap.near");
    assert_eq!(row.token_out, "usdt.tether-token.near");
    assert_eq!(row.route, "swap");

    let back = from_db(as_db_row(&row)).unwrap();
    assert_eq!(back, trade);
}

#[test]
fn round_trip_without_fees() {
    let trade = make_trade(2, 1, 0, vec![hop(1, "a.near", "b.near")]);
    let row = to_new_db(&trade).unwrap();
    assert_eq!(row.fee_bps, None);
    assert_eq!(row.route, "ft");
    let back = from_db(as_db_row(&row)).unwrap();
    assert_eq!(back, trade);
    assert_eq!(back.fees(), None);
}

fn expect_err(row: &NewDbRefTrade, needle: &str) {
    let err = from_db(as_db_row(row)).unwrap_err();
    let text = format!("{err:#}");
    assert!(text.contains(needle), "expected {needle:?} in: {text}");
}

#[test]
fn rejects_hop_array_length_mismatch() {
    let mut row = to_new_db(&make_trade(1, 1, 0, chain_hops())).unwrap();
    tweak(&mut row).hop_tokens_out.pop();
    expect_err(&row, "hop array lengths differ");
}

#[test]
fn rejects_null_elements() {
    let mut row = to_new_db(&make_trade(1, 1, 0, chain_hops())).unwrap();
    tweak(&mut row).pool_ids[1] = None;
    expect_err(&row, "NULL element in pool_ids");

    let mut row = to_new_db(&make_trade(1, 1, 0, chain_hops())).unwrap();
    *tweak(&mut row).fee_bps = Some(vec![Some(30), None]);
    expect_err(&row, "NULL element in fee_bps");
}

#[test]
fn rejects_unknown_route_and_bad_ids() {
    let mut row = to_new_db(&make_trade(1, 1, 0, chain_hops())).unwrap();
    *tweak(&mut row).route = "flash".to_owned();
    expect_err(&row, "unknown trade route");

    let mut row = to_new_db(&make_trade(1, 1, 0, chain_hops())).unwrap();
    *tweak(&mut row).tx_hash = "0OIl".to_owned();
    expect_err(&row, "base58");
}

#[test]
fn rejects_non_integer_or_out_of_range_amounts() {
    let mut row = to_new_db(&make_trade(1, 1, 0, chain_hops())).unwrap();
    *tweak(&mut row).amount_in = BigDecimal::from_str("1.5").unwrap();
    expect_err(&row, "amount_in");

    let mut row = to_new_db(&make_trade(1, 1, 0, chain_hops())).unwrap();
    *tweak(&mut row).amount_out = BigDecimal::from(u128::MAX) + BigDecimal::from(1);
    expect_err(&row, "amount_out");
}

#[test]
fn rejects_endpoint_columns_that_disagree_with_hops() {
    let mut row = to_new_db(&make_trade(1, 1, 0, chain_hops())).unwrap();
    *tweak(&mut row).token_out = "wrap.near".to_owned();
    expect_err(&row, "disagree with hops");
}
