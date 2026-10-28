use super::*;
use bigdecimal::BigDecimal;
use common::types::TokenAccount;

const TX: &str = "6YzxCS3V2yFFaVc1oSG5C9Y5VL6Md5kytehV9MHpERKt";
const RECEIPT: &str = "AtQMD91kr9UgmWY5ghBuV68u6APu2jTC8qawZVN9wQhx";

fn tok(name: &str) -> TokenAccount {
    name.parse().unwrap()
}

fn hop(pool_id: u32, token_in: &str, token_out: &str) -> Hop {
    Hop {
        pool_id,
        token_in: tok(token_in).to_in(),
        token_out: tok(token_out).to_out(),
        amount_in: units(1_000),
        amount_out: units(999),
    }
}

fn units(v: u128) -> TokenSmallestUnits {
    TokenSmallestUnits::from_u128(v)
}

fn parts(hops: Vec<Hop>) -> TradeParts {
    TradeParts {
        receipt_id: ReceiptId::new(RECEIPT).unwrap(),
        tx_hash: TxHash::new(TX).unwrap(),
        block_height: 221_385_535,
        block_time: chrono::DateTime::from_timestamp(1_792_843_194, 0)
            .unwrap()
            .naive_utc(),
        sender: "alice.near".parse().unwrap(),
        route: TradeRoute::Swap,
        hops,
        fees: None,
        amount_in: units(573_855_506_835_779_282_057),
        amount_out: units(39_827_853_876_926_886),
    }
}

fn chain_hops() -> Vec<Hop> {
    vec![
        hop(6458, "wrap.near", "token.rhealab.near"),
        hop(5516, "token.rhealab.near", "usdt.tether-token.near"),
    ]
}

#[test]
fn builds_a_single_hop_trade_and_exposes_endpoints() {
    let trade = Trade::new(parts(vec![hop(6458, "wrap.near", "token.rhealab.near")])).unwrap();
    assert_eq!(trade.hops().len(), 1);
    assert_eq!(trade.token_in().0, tok("wrap.near"));
    assert_eq!(trade.token_out().0, tok("token.rhealab.near"));
    assert_eq!(trade.shape(), RouteShape::Chain);
    assert_eq!(trade.fees(), None);
    assert_eq!(trade.amount_in(), &units(573_855_506_835_779_282_057));
    assert_eq!(trade.amount_out(), &units(39_827_853_876_926_886));
}

#[test]
fn chain_shape_when_every_hop_feeds_the_next() {
    let trade = Trade::new(parts(chain_hops())).unwrap();
    assert_eq!(trade.shape(), RouteShape::Chain);
    assert_eq!(trade.token_in().0, tok("wrap.near"));
    assert_eq!(trade.token_out().0, tok("usdt.tether-token.near"));
}

#[test]
fn split_shape_when_hops_are_not_connected() {
    // Two parallel legs wrap.near -> usdc via different pools.
    let trade = Trade::new(parts(vec![
        hop(3515, "wrap.near", "usdc.near"),
        hop(4512, "wrap.near", "usdc.near"),
    ]))
    .unwrap();
    assert_eq!(trade.shape(), RouteShape::Split);

    // A chain whose middle link is broken is also Split.
    let trade = Trade::new(parts(vec![
        hop(1, "a.near", "b.near"),
        hop(2, "b.near", "c.near"),
        hop(3, "x.near", "c.near"),
    ]))
    .unwrap();
    assert_eq!(trade.shape(), RouteShape::Split);
}

#[test]
fn rejects_empty_hops() {
    assert_eq!(Trade::new(parts(vec![])).unwrap_err(), TradeError::NoHops);
}

#[test]
fn rejects_more_than_max_hops() {
    let hops: Vec<Hop> = (0..=MAX_HOPS as u32)
        .map(|i| hop(i, &format!("t{i}.near"), &format!("t{}.near", i + 1)))
        .collect();
    assert_eq!(
        Trade::new(parts(hops)).unwrap_err(),
        TradeError::TooManyHops(MAX_HOPS + 1)
    );
    let hops: Vec<Hop> = (0..MAX_HOPS as u32)
        .map(|i| hop(i, &format!("t{i}.near"), &format!("t{}.near", i + 1)))
        .collect();
    assert!(Trade::new(parts(hops)).is_ok());
}

#[test]
fn fees_must_match_hop_count_and_be_below_divisor() {
    let mut p = parts(chain_hops());
    p.fees = Some(vec![30]);
    assert_eq!(
        Trade::new(p).unwrap_err(),
        TradeError::FeeCountMismatch { hops: 2, fees: 1 }
    );

    let mut p = parts(chain_hops());
    p.fees = Some(vec![30, FEE_DIVISOR]);
    assert_eq!(
        Trade::new(p).unwrap_err(),
        TradeError::FeeOutOfRange(FEE_DIVISOR)
    );

    let mut p = parts(chain_hops());
    p.fees = Some(vec![30, 0]);
    let trade = Trade::new(p).unwrap();
    assert_eq!(trade.fees(), Some(&[30, 0][..]));
    let zipped: Vec<(u32, Option<u32>)> =
        trade.hops_with_fee().map(|(h, f)| (h.pool_id, f)).collect();
    assert_eq!(zipped, vec![(6458, Some(30)), (5516, Some(0))]);
}

#[test]
fn hops_with_fee_yields_none_when_fees_unknown() {
    let trade = Trade::new(parts(chain_hops())).unwrap();
    let fees: Vec<Option<u32>> = trade.hops_with_fee().map(|(_, f)| f).collect();
    assert_eq!(fees, vec![None, None]);
}

#[test]
fn amount_in_must_be_positive_integer_within_u128() {
    let zero = units(0);
    let mut p = parts(chain_hops());
    p.amount_in = zero.clone();
    assert_eq!(
        Trade::new(p).unwrap_err(),
        TradeError::InvalidAmountIn(zero)
    );

    let fractional = TokenSmallestUnits::from_bigdecimal(BigDecimal::from_str("1.5").unwrap());
    let mut p = parts(chain_hops());
    p.amount_in = fractional.clone();
    assert_eq!(
        Trade::new(p).unwrap_err(),
        TradeError::InvalidAmountIn(fractional)
    );

    let negative = TokenSmallestUnits::from_bigdecimal(BigDecimal::from(-1));
    let mut p = parts(chain_hops());
    p.amount_in = negative.clone();
    assert_eq!(
        Trade::new(p).unwrap_err(),
        TradeError::InvalidAmountIn(negative)
    );

    let too_big =
        TokenSmallestUnits::from_bigdecimal(BigDecimal::from(u128::MAX) + BigDecimal::from(1));
    let mut p = parts(chain_hops());
    p.amount_in = too_big.clone();
    assert_eq!(
        Trade::new(p).unwrap_err(),
        TradeError::InvalidAmountIn(too_big)
    );

    // Integers written with a positive exponent are still integers.
    let exp = TokenSmallestUnits::from_bigdecimal(BigDecimal::from_str("1E+3").unwrap());
    let mut p = parts(chain_hops());
    p.amount_in = exp;
    assert!(Trade::new(p).is_ok());

    let max = units(u128::MAX);
    let mut p = parts(chain_hops());
    p.amount_in = max;
    assert!(Trade::new(p).is_ok());
}

#[test]
fn amount_out_may_be_zero_but_must_be_integer_within_u128() {
    let mut p = parts(chain_hops());
    p.amount_out = units(0);
    assert!(Trade::new(p).is_ok());

    let fractional = TokenSmallestUnits::from_bigdecimal(BigDecimal::from_str("0.1").unwrap());
    let mut p = parts(chain_hops());
    p.amount_out = fractional.clone();
    assert_eq!(
        Trade::new(p).unwrap_err(),
        TradeError::InvalidAmountOut(fractional)
    );
}

#[test]
fn trade_route_round_trips_through_db_strings() {
    for route in TradeRoute::ALL {
        let text = route.to_string();
        assert_eq!(text.parse::<TradeRoute>().unwrap(), route);
        assert_eq!(route.as_str(), text);
    }
    assert_eq!(TradeRoute::Swap.as_str(), "swap");
    assert_eq!(TradeRoute::Ft.as_str(), "ft");
    let err = "SWAP".parse::<TradeRoute>().unwrap_err();
    assert_eq!(err.to_string(), "unknown trade route: \"SWAP\"");
}

#[test]
fn trade_route_all_is_exhaustive() {
    fn _exhaustive(route: TradeRoute) {
        match route {
            TradeRoute::Swap | TradeRoute::Ft => {}
        }
    }
    assert_eq!(TradeRoute::ALL.len(), 2);
}

#[test]
fn with_fees_attaches_validated_fees() {
    let trade = Trade::new(parts(chain_hops())).unwrap();
    assert_eq!(
        trade.clone().with_fees(vec![30]).unwrap_err(),
        TradeError::FeeCountMismatch { hops: 2, fees: 1 }
    );
    assert_eq!(
        trade.clone().with_fees(vec![30, FEE_DIVISOR]).unwrap_err(),
        TradeError::FeeOutOfRange(FEE_DIVISOR)
    );
    let with = trade.with_fees(vec![30, 5]).unwrap();
    assert_eq!(with.fees(), Some(&[30, 5][..]));
}

#[test]
fn hop_amounts_must_be_integers_within_u128() {
    let mut hops = chain_hops();
    hops[1].amount_out = TokenSmallestUnits::from_bigdecimal(BigDecimal::from_str("0.5").unwrap());
    let err = Trade::new(parts(hops)).unwrap_err();
    assert!(
        matches!(err, TradeError::InvalidHopAmount { index: 1, .. }),
        "{err}"
    );

    let mut hops = chain_hops();
    hops[0].amount_in = units(0);
    assert!(
        Trade::new(parts(hops)).is_ok(),
        "zero hop amounts are allowed"
    );
}
