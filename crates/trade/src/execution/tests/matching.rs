use super::helpers::{RATE_24, buy_op, sell_op, ta};
use super::*;
use bigdecimal::BigDecimal;
use common::types::{ExchangeRate, NearValue};
use std::str::FromStr;

#[test]
fn test_exact_match_single_pair() {
    let sells = vec![sell_op("token_a.near", 80, RATE_24, 24)];
    let buys = vec![buy_op("token_b.near", 80)];

    let result = match_rebalance_operations(sells, buys);

    assert_eq!(result.direct_swaps.len(), 1);
    assert_eq!(result.direct_swaps[0].sell_token, ta("token_a.near"));
    assert_eq!(result.direct_swaps[0].buy_token, ta("token_b.near"));
    assert_eq!(
        result.direct_swaps[0].near_value,
        NearValue::from_near(BigDecimal::from(80))
    );
    assert!(result.remaining_sells.is_empty());
    assert!(result.remaining_buys.is_empty());
}

#[test]
fn test_sell_greater_than_buy() {
    let sells = vec![sell_op("token_a.near", 100, RATE_24, 24)];
    let buys = vec![buy_op("token_b.near", 60)];

    let result = match_rebalance_operations(sells, buys);

    assert_eq!(result.direct_swaps.len(), 1);
    assert_eq!(
        result.direct_swaps[0].near_value,
        NearValue::from_near(BigDecimal::from(60))
    );
    assert_eq!(result.remaining_sells.len(), 1);
    assert_eq!(result.remaining_sells[0].token, ta("token_a.near"));
    assert_eq!(
        result.remaining_sells[0].near_value,
        NearValue::from_near(BigDecimal::from(40))
    );
    assert!(result.remaining_buys.is_empty());
}

#[test]
fn test_buy_greater_than_sell() {
    let sells = vec![sell_op("token_a.near", 50, RATE_24, 24)];
    let buys = vec![buy_op("token_b.near", 80)];

    let result = match_rebalance_operations(sells, buys);

    assert_eq!(result.direct_swaps.len(), 1);
    assert_eq!(
        result.direct_swaps[0].near_value,
        NearValue::from_near(BigDecimal::from(50))
    );
    assert!(result.remaining_sells.is_empty());
    assert_eq!(result.remaining_buys.len(), 1);
    assert_eq!(result.remaining_buys[0].token, ta("token_b.near"));
    assert_eq!(
        result.remaining_buys[0].near_value,
        NearValue::from_near(BigDecimal::from(30))
    );
}

#[test]
fn test_multiple_sells_multiple_buys_exact() {
    // 売却 A(80), C(20) / 購入 B(60), D(40)
    let sells = vec![
        sell_op("token_a.near", 80, RATE_24, 24),
        sell_op("token_c.near", 20, "1000000000000000000000000", 24),
    ];
    let buys = vec![buy_op("token_b.near", 60), buy_op("token_d.near", 40)];

    let result = match_rebalance_operations(sells, buys);

    assert_eq!(result.direct_swaps.len(), 3);

    // 降順: A(80), C(20); B(60), D(40)
    // Match 1: A→B min(80,60)=60, A_rem=20
    assert_eq!(result.direct_swaps[0].sell_token, ta("token_a.near"));
    assert_eq!(result.direct_swaps[0].buy_token, ta("token_b.near"));
    assert_eq!(
        result.direct_swaps[0].near_value,
        NearValue::from_near(BigDecimal::from(60))
    );

    // Match 2: A→D min(20,40)=20, D_rem=20
    assert_eq!(result.direct_swaps[1].sell_token, ta("token_a.near"));
    assert_eq!(result.direct_swaps[1].buy_token, ta("token_d.near"));
    assert_eq!(
        result.direct_swaps[1].near_value,
        NearValue::from_near(BigDecimal::from(20))
    );

    // Match 3: C→D min(20,20)=20
    assert_eq!(result.direct_swaps[2].sell_token, ta("token_c.near"));
    assert_eq!(result.direct_swaps[2].buy_token, ta("token_d.near"));
    assert_eq!(
        result.direct_swaps[2].near_value,
        NearValue::from_near(BigDecimal::from(20))
    );

    assert!(result.remaining_sells.is_empty());
    assert!(result.remaining_buys.is_empty());
}

#[test]
fn test_empty_sells() {
    let sells = vec![];
    let buys = vec![buy_op("token_b.near", 60)];

    let result = match_rebalance_operations(sells, buys);

    assert!(result.direct_swaps.is_empty());
    assert!(result.remaining_sells.is_empty());
    assert_eq!(result.remaining_buys.len(), 1);
    assert_eq!(result.remaining_buys[0].token, ta("token_b.near"));
}

#[test]
fn test_empty_buys() {
    let sells = vec![sell_op("token_a.near", 80, RATE_24, 24)];
    let buys = vec![];

    let result = match_rebalance_operations(sells, buys);

    assert!(result.direct_swaps.is_empty());
    assert_eq!(result.remaining_sells.len(), 1);
    assert_eq!(result.remaining_sells[0].token, ta("token_a.near"));
    assert!(result.remaining_buys.is_empty());
}

#[test]
fn test_both_empty() {
    let result = match_rebalance_operations(vec![], vec![]);

    assert!(result.direct_swaps.is_empty());
    assert!(result.remaining_sells.is_empty());
    assert!(result.remaining_buys.is_empty());
}

#[test]
fn test_single_sell_multiple_buys() {
    // 売却 A(100) / 購入 B(30), C(30), D(40)
    let sells = vec![sell_op("token_a.near", 100, RATE_24, 24)];
    let buys = vec![
        buy_op("token_b.near", 30),
        buy_op("token_c.near", 30),
        buy_op("token_d.near", 40),
    ];

    let result = match_rebalance_operations(sells, buys);

    assert_eq!(result.direct_swaps.len(), 3);

    // 購入は降順: D(40), B(30), C(30)
    assert_eq!(result.direct_swaps[0].buy_token, ta("token_d.near"));
    assert_eq!(
        result.direct_swaps[0].near_value,
        NearValue::from_near(BigDecimal::from(40))
    );
    assert_eq!(result.direct_swaps[1].buy_token, ta("token_b.near"));
    assert_eq!(
        result.direct_swaps[1].near_value,
        NearValue::from_near(BigDecimal::from(30))
    );
    assert_eq!(result.direct_swaps[2].buy_token, ta("token_c.near"));
    assert_eq!(
        result.direct_swaps[2].near_value,
        NearValue::from_near(BigDecimal::from(30))
    );

    assert!(result.remaining_sells.is_empty());
    assert!(result.remaining_buys.is_empty());
}

#[test]
fn test_large_value_difference() {
    // 売却 A(1000) / 購入 B(1)
    let sells = vec![sell_op("token_a.near", 1000, RATE_24, 24)];
    let buys = vec![buy_op("token_b.near", 1)];

    let result = match_rebalance_operations(sells, buys);

    assert_eq!(result.direct_swaps.len(), 1);
    assert_eq!(
        result.direct_swaps[0].near_value,
        NearValue::from_near(BigDecimal::from(1))
    );
    assert_eq!(result.remaining_sells.len(), 1);
    assert_eq!(result.remaining_sells[0].token, ta("token_a.near"));
    assert_eq!(
        result.remaining_sells[0].near_value,
        NearValue::from_near(BigDecimal::from(999))
    );
    assert!(result.remaining_buys.is_empty());
}

#[test]
fn test_direct_swaps_preserve_exchange_rate() {
    let sells = vec![sell_op("token_a.near", 80, RATE_24, 24)];
    let buys = vec![buy_op("token_b.near", 80)];

    let result = match_rebalance_operations(sells, buys);

    assert_eq!(
        result.direct_swaps[0].sell_exchange_rate,
        ExchangeRate::from_raw_rate(BigDecimal::from_str(RATE_24).unwrap(), 24)
    );
}

#[test]
fn test_multiple_sells_single_buy() {
    // 売却 A(30), B(50) / 購入 C(80)
    let sells = vec![
        sell_op("token_a.near", 30, RATE_24, 24),
        sell_op("token_b.near", 50, "1000000000000000000000000", 24),
    ];
    let buys = vec![buy_op("token_c.near", 80)];

    let result = match_rebalance_operations(sells, buys);

    assert_eq!(result.direct_swaps.len(), 2);

    // 降順: B(50), A(30); C(80)
    assert_eq!(result.direct_swaps[0].sell_token, ta("token_b.near"));
    assert_eq!(result.direct_swaps[0].buy_token, ta("token_c.near"));
    assert_eq!(
        result.direct_swaps[0].near_value,
        NearValue::from_near(BigDecimal::from(50))
    );

    assert_eq!(result.direct_swaps[1].sell_token, ta("token_a.near"));
    assert_eq!(result.direct_swaps[1].buy_token, ta("token_c.near"));
    assert_eq!(
        result.direct_swaps[1].near_value,
        NearValue::from_near(BigDecimal::from(30))
    );

    assert!(result.remaining_sells.is_empty());
    assert!(result.remaining_buys.is_empty());
}

// --- CRITICAL バグ再現テスト ---

#[test]
fn test_multiple_sells_vs_single_buy_remainder() {
    // 3売却 vs 1購入: 残余売却が全て保持されることを検証
    // sells=[A(50), B(50), C(50)], buys=[D(30)]
    // 降順: A(50), B(50), C(50); D(30)
    // Match: A→D min(50,30)=30, A_rem=20
    // → remaining_sells: A(20), B(50), C(50)
    let sells = vec![
        sell_op("token_a.near", 50, RATE_24, 24),
        sell_op("token_b.near", 50, RATE_24, 24),
        sell_op("token_c.near", 50, RATE_24, 24),
    ];
    let buys = vec![buy_op("token_d.near", 30)];

    let result = match_rebalance_operations(sells, buys);

    assert_eq!(result.direct_swaps.len(), 1);
    assert_eq!(result.direct_swaps[0].sell_token, ta("token_a.near"));
    assert_eq!(result.direct_swaps[0].buy_token, ta("token_d.near"));
    assert_eq!(
        result.direct_swaps[0].near_value,
        NearValue::from_near(BigDecimal::from(30))
    );

    // 全ての未消化売却が remaining_sells に含まれること
    assert_eq!(result.remaining_sells.len(), 3);
    assert_eq!(result.remaining_sells[0].token, ta("token_a.near"));
    assert_eq!(
        result.remaining_sells[0].near_value,
        NearValue::from_near(BigDecimal::from(20))
    );
    assert_eq!(result.remaining_sells[1].token, ta("token_b.near"));
    assert_eq!(
        result.remaining_sells[1].near_value,
        NearValue::from_near(BigDecimal::from(50))
    );
    assert_eq!(result.remaining_sells[2].token, ta("token_c.near"));
    assert_eq!(
        result.remaining_sells[2].near_value,
        NearValue::from_near(BigDecimal::from(50))
    );
    assert!(result.remaining_buys.is_empty());
}

#[test]
fn test_single_sell_vs_multiple_buys_remainder() {
    // 1売却 vs 3購入: 残余購入が全て保持されることを検証（対称ケース）
    // sells=[A(30)], buys=[B(50), C(50), D(50)]
    // 降順: A(30); B(50), C(50), D(50)
    // Match: A→B min(30,50)=30, B_rem=20
    // → remaining_buys: B(20), C(50), D(50)
    let sells = vec![sell_op("token_a.near", 30, RATE_24, 24)];
    let buys = vec![
        buy_op("token_b.near", 50),
        buy_op("token_c.near", 50),
        buy_op("token_d.near", 50),
    ];

    let result = match_rebalance_operations(sells, buys);

    assert_eq!(result.direct_swaps.len(), 1);
    assert_eq!(result.direct_swaps[0].sell_token, ta("token_a.near"));
    assert_eq!(result.direct_swaps[0].buy_token, ta("token_b.near"));
    assert_eq!(
        result.direct_swaps[0].near_value,
        NearValue::from_near(BigDecimal::from(30))
    );

    assert!(result.remaining_sells.is_empty());
    assert_eq!(result.remaining_buys.len(), 3);
    assert_eq!(result.remaining_buys[0].token, ta("token_b.near"));
    assert_eq!(
        result.remaining_buys[0].near_value,
        NearValue::from_near(BigDecimal::from(20))
    );
    assert_eq!(result.remaining_buys[1].token, ta("token_c.near"));
    assert_eq!(
        result.remaining_buys[1].near_value,
        NearValue::from_near(BigDecimal::from(50))
    );
    assert_eq!(result.remaining_buys[2].token, ta("token_d.near"));
    assert_eq!(
        result.remaining_buys[2].near_value,
        NearValue::from_near(BigDecimal::from(50))
    );
}

#[test]
fn test_invariant_total_value_preserved() {
    // 不変条件検証: sum(direct_swaps) + sum(remaining) == total
    let sells = vec![
        sell_op("token_a.near", 80, RATE_24, 24),
        sell_op("token_b.near", 30, RATE_24, 24),
    ];
    let buys = vec![buy_op("token_c.near", 60), buy_op("token_d.near", 20)];

    let result = match_rebalance_operations(sells, buys);

    let total_sell = NearValue::from_near(BigDecimal::from(110)); // 80 + 30
    let total_buy = NearValue::from_near(BigDecimal::from(80)); // 60 + 20

    let swap_sum: NearValue = result
        .direct_swaps
        .iter()
        .map(|ds| ds.near_value.clone())
        .fold(NearValue::zero(), |acc, v| acc + v);
    let remaining_sell_sum: NearValue = result
        .remaining_sells
        .iter()
        .map(|s| s.near_value.clone())
        .fold(NearValue::zero(), |acc, v| acc + v);
    let remaining_buy_sum: NearValue = result
        .remaining_buys
        .iter()
        .map(|b| b.near_value.clone())
        .fold(NearValue::zero(), |acc, v| acc + v);

    // 直接スワップ合計 = min(total_sell, total_buy) = total_buy = 80
    assert_eq!(swap_sum, total_buy);
    // 残余売却合計 = total_sell - total_buy = 30
    assert_eq!(
        remaining_sell_sum,
        NearValue::from_near(BigDecimal::from(30))
    );
    // 残余購入なし
    assert_eq!(remaining_buy_sum, NearValue::zero());
    // 不変条件: swap_sum + remaining_sell_sum = total_sell
    assert_eq!(swap_sum + remaining_sell_sum, total_sell);
}
