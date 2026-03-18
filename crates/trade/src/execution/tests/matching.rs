use super::*;
use bigdecimal::BigDecimal;
use common::types::{ExchangeRate, NearValue, TokenAccount};
use std::str::FromStr;

fn ta(s: &str) -> TokenAccount {
    s.parse().unwrap()
}

fn sell_op(token: &str, near: i64, rate_raw: &str, decimals: u8) -> SellOperation {
    SellOperation {
        token: ta(token),
        near_value: NearValue::from_near(BigDecimal::from(near)),
        exchange_rate: ExchangeRate::from_raw_rate(
            BigDecimal::from_str(rate_raw).unwrap(),
            decimals,
        ),
    }
}

fn buy_op(token: &str, near: i64) -> BuyOperation {
    BuyOperation {
        token: ta(token),
        near_value: NearValue::from_near(BigDecimal::from(near)),
    }
}

#[test]
fn test_exact_match_single_pair() {
    // 売却80 NEAR, 購入80 NEAR → 1 DirectSwap, Remainder::None
    let sells = vec![sell_op("token_a.near", 80, "500000000000000000000000", 24)];
    let buys = vec![buy_op("token_b.near", 80)];

    let (swaps, remainder) = match_rebalance_operations(sells, buys);

    assert_eq!(swaps.len(), 1);
    assert_eq!(swaps[0].sell_token, ta("token_a.near"));
    assert_eq!(swaps[0].buy_token, ta("token_b.near"));
    assert_eq!(
        swaps[0].near_value,
        NearValue::from_near(BigDecimal::from(80))
    );
    assert_eq!(remainder, Remainder::None);
}

#[test]
fn test_sell_greater_than_buy() {
    // 売却100, 購入60 → 1 DirectSwap(60), Remainder::Sell(40)
    let sells = vec![sell_op("token_a.near", 100, "500000000000000000000000", 24)];
    let buys = vec![buy_op("token_b.near", 60)];

    let (swaps, remainder) = match_rebalance_operations(sells, buys);

    assert_eq!(swaps.len(), 1);
    assert_eq!(
        swaps[0].near_value,
        NearValue::from_near(BigDecimal::from(60))
    );
    match &remainder {
        Remainder::Sell(token, value, _) => {
            assert_eq!(token, &ta("token_a.near"));
            assert_eq!(value, &NearValue::from_near(BigDecimal::from(40)));
        }
        _ => panic!("Expected Remainder::Sell, got {:?}", remainder),
    }
}

#[test]
fn test_buy_greater_than_sell() {
    // 売却50, 購入80 → 1 DirectSwap(50), Remainder::Buy(30)
    let sells = vec![sell_op("token_a.near", 50, "500000000000000000000000", 24)];
    let buys = vec![buy_op("token_b.near", 80)];

    let (swaps, remainder) = match_rebalance_operations(sells, buys);

    assert_eq!(swaps.len(), 1);
    assert_eq!(
        swaps[0].near_value,
        NearValue::from_near(BigDecimal::from(50))
    );
    match &remainder {
        Remainder::Buy(token, value) => {
            assert_eq!(token, &ta("token_b.near"));
            assert_eq!(value, &NearValue::from_near(BigDecimal::from(30)));
        }
        _ => panic!("Expected Remainder::Buy, got {:?}", remainder),
    }
}

#[test]
fn test_multiple_sells_multiple_buys_exact() {
    // 計画の例: 売却 A(80), C(20) / 購入 B(60), D(40)
    // → A→B(60), A→D(20), C→D(20), Remainder::None
    let sells = vec![
        sell_op("token_a.near", 80, "500000000000000000000000", 24),
        sell_op("token_c.near", 20, "1000000000000000000000000", 24),
    ];
    let buys = vec![buy_op("token_b.near", 60), buy_op("token_d.near", 40)];

    let (swaps, remainder) = match_rebalance_operations(sells, buys);

    assert_eq!(swaps.len(), 3);

    // 降順ソートされるので: A(80) first, then C(20); B(60) first, then D(40)
    // Match 1: A→B min(80,60)=60, A_rem=20, B消化
    assert_eq!(swaps[0].sell_token, ta("token_a.near"));
    assert_eq!(swaps[0].buy_token, ta("token_b.near"));
    assert_eq!(
        swaps[0].near_value,
        NearValue::from_near(BigDecimal::from(60))
    );

    // Match 2: A→D min(20,40)=20, A消化, D_rem=20
    assert_eq!(swaps[1].sell_token, ta("token_a.near"));
    assert_eq!(swaps[1].buy_token, ta("token_d.near"));
    assert_eq!(
        swaps[1].near_value,
        NearValue::from_near(BigDecimal::from(20))
    );

    // Match 3: C→D min(20,20)=20
    assert_eq!(swaps[2].sell_token, ta("token_c.near"));
    assert_eq!(swaps[2].buy_token, ta("token_d.near"));
    assert_eq!(
        swaps[2].near_value,
        NearValue::from_near(BigDecimal::from(20))
    );

    assert_eq!(remainder, Remainder::None);
}

#[test]
fn test_empty_sells() {
    let sells = vec![];
    let buys = vec![buy_op("token_b.near", 60)];

    let (swaps, remainder) = match_rebalance_operations(sells, buys);

    assert!(swaps.is_empty());
    assert_eq!(remainder, Remainder::None);
}

#[test]
fn test_empty_buys() {
    let sells = vec![sell_op("token_a.near", 80, "500000000000000000000000", 24)];
    let buys = vec![];

    let (swaps, remainder) = match_rebalance_operations(sells, buys);

    assert!(swaps.is_empty());
    assert_eq!(remainder, Remainder::None);
}

#[test]
fn test_both_empty() {
    let (swaps, remainder) = match_rebalance_operations(vec![], vec![]);

    assert!(swaps.is_empty());
    assert_eq!(remainder, Remainder::None);
}

#[test]
fn test_single_sell_multiple_buys() {
    // 売却 A(100) / 購入 B(30), C(30), D(40)
    let sells = vec![sell_op("token_a.near", 100, "500000000000000000000000", 24)];
    let buys = vec![
        buy_op("token_b.near", 30),
        buy_op("token_c.near", 30),
        buy_op("token_d.near", 40),
    ];

    let (swaps, remainder) = match_rebalance_operations(sells, buys);

    assert_eq!(swaps.len(), 3);

    // 購入は降順: D(40), B(30), C(30)
    // Match 1: A→D min(100,40)=40, A_rem=60
    assert_eq!(swaps[0].buy_token, ta("token_d.near"));
    assert_eq!(
        swaps[0].near_value,
        NearValue::from_near(BigDecimal::from(40))
    );

    // Match 2: A→B min(60,30)=30, A_rem=30
    assert_eq!(swaps[1].buy_token, ta("token_b.near"));
    assert_eq!(
        swaps[1].near_value,
        NearValue::from_near(BigDecimal::from(30))
    );

    // Match 3: A→C min(30,30)=30
    assert_eq!(swaps[2].buy_token, ta("token_c.near"));
    assert_eq!(
        swaps[2].near_value,
        NearValue::from_near(BigDecimal::from(30))
    );

    assert_eq!(remainder, Remainder::None);
}

#[test]
fn test_large_value_difference() {
    // 売却 A(1000) / 購入 B(1)
    let sells = vec![sell_op(
        "token_a.near",
        1000,
        "500000000000000000000000",
        24,
    )];
    let buys = vec![buy_op("token_b.near", 1)];

    let (swaps, remainder) = match_rebalance_operations(sells, buys);

    assert_eq!(swaps.len(), 1);
    assert_eq!(
        swaps[0].near_value,
        NearValue::from_near(BigDecimal::from(1))
    );
    match &remainder {
        Remainder::Sell(token, value, _) => {
            assert_eq!(token, &ta("token_a.near"));
            assert_eq!(value, &NearValue::from_near(BigDecimal::from(999)));
        }
        _ => panic!("Expected Remainder::Sell"),
    }
}

#[test]
fn test_direct_swaps_preserve_exchange_rate() {
    let rate_str = "500000000000000000000000";
    let sells = vec![sell_op("token_a.near", 80, rate_str, 24)];
    let buys = vec![buy_op("token_b.near", 80)];

    let (swaps, _) = match_rebalance_operations(sells, buys);

    assert_eq!(
        swaps[0].sell_exchange_rate,
        ExchangeRate::from_raw_rate(BigDecimal::from_str(rate_str).unwrap(), 24)
    );
}

#[test]
fn test_adjust_buy_to_available_sufficient() {
    let buy = NearValue::from_near(BigDecimal::from(50));
    let available = NearValue::from_near(BigDecimal::from(100));
    let result = adjust_buy_to_available(&buy, &available);
    assert_eq!(result, buy);
}

#[test]
fn test_adjust_buy_to_available_insufficient() {
    let buy = NearValue::from_near(BigDecimal::from(100));
    let available = NearValue::from_near(BigDecimal::from(50));
    let result = adjust_buy_to_available(&buy, &available);
    assert_eq!(result, available);
}

#[test]
fn test_adjust_buy_to_available_exact() {
    let buy = NearValue::from_near(BigDecimal::from(50));
    let available = NearValue::from_near(BigDecimal::from(50));
    let result = adjust_buy_to_available(&buy, &available);
    assert_eq!(result, buy);
}

#[test]
fn test_multiple_sells_single_buy() {
    // 売却 A(30), B(50) / 購入 C(80)
    let sells = vec![
        sell_op("token_a.near", 30, "500000000000000000000000", 24),
        sell_op("token_b.near", 50, "1000000000000000000000000", 24),
    ];
    let buys = vec![buy_op("token_c.near", 80)];

    let (swaps, remainder) = match_rebalance_operations(sells, buys);

    assert_eq!(swaps.len(), 2);

    // 降順: B(50), A(30); C(80)
    // Match 1: B→C min(50,80)=50, C_rem=30
    assert_eq!(swaps[0].sell_token, ta("token_b.near"));
    assert_eq!(swaps[0].buy_token, ta("token_c.near"));
    assert_eq!(
        swaps[0].near_value,
        NearValue::from_near(BigDecimal::from(50))
    );

    // Match 2: A→C min(30,30)=30
    assert_eq!(swaps[1].sell_token, ta("token_a.near"));
    assert_eq!(swaps[1].buy_token, ta("token_c.near"));
    assert_eq!(
        swaps[1].near_value,
        NearValue::from_near(BigDecimal::from(30))
    );

    assert_eq!(remainder, Remainder::None);
}
