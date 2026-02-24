use super::*;
use bigdecimal::BigDecimal;
use common::types::TokenAmount;
use persistence::portfolio_holding::TokenHolding;
use std::collections::BTreeMap;

fn ta(s: &str) -> TokenAccount {
    s.parse().unwrap()
}

#[test]
fn test_balances_to_holdings_basic() {
    let mut balances = BTreeMap::new();
    balances.insert(
        ta("wrap.near"),
        TokenAmount::from_smallest_units(
            BigDecimal::from(1_000_000_000_000_000_000_000_000u128),
            24,
        ),
    );
    balances.insert(
        ta("usdt.tether-token.near"),
        TokenAmount::from_smallest_units(BigDecimal::from(1_000_000u64), 6),
    );

    let holdings = balances_to_holdings(&balances);

    assert_eq!(holdings.len(), 2);

    // BTreeMap は key でソート済み
    let usdt = holdings
        .iter()
        .find(|h| h.token == "usdt.tether-token.near")
        .unwrap();
    assert_eq!(usdt.balance, "1000000");
    assert_eq!(usdt.decimals, 6);

    let wnear = holdings.iter().find(|h| h.token == "wrap.near").unwrap();
    assert_eq!(wnear.balance, "1000000000000000000000000");
    assert_eq!(wnear.decimals, 24);
}

#[test]
fn test_balances_to_holdings_filters_zero() {
    let mut balances = BTreeMap::new();
    balances.insert(
        ta("token-a.near"),
        TokenAmount::from_smallest_units(BigDecimal::from(100u64), 18),
    );
    balances.insert(
        ta("token-b.near"),
        TokenAmount::from_smallest_units(BigDecimal::from(0u64), 18),
    );
    balances.insert(
        ta("token-c.near"),
        TokenAmount::from_smallest_units(BigDecimal::from(999u64), 6),
    );

    let holdings = balances_to_holdings(&balances);

    assert_eq!(holdings.len(), 2);
    let tokens: Vec<&str> = holdings.iter().map(|h| h.token.as_str()).collect();
    assert!(tokens.contains(&"token-a.near"));
    assert!(tokens.contains(&"token-c.near"));
    assert!(!tokens.contains(&"token-b.near"));
}

#[test]
fn test_balances_to_holdings_empty() {
    let balances = BTreeMap::new();
    let holdings = balances_to_holdings(&balances);
    assert!(holdings.is_empty());
}

#[test]
fn test_balances_to_holdings_all_zero() {
    let mut balances = BTreeMap::new();
    balances.insert(
        ta("token-a.near"),
        TokenAmount::from_smallest_units(BigDecimal::from(0u64), 18),
    );
    balances.insert(
        ta("token-b.near"),
        TokenAmount::from_smallest_units(BigDecimal::from(0u64), 6),
    );

    let holdings = balances_to_holdings(&balances);
    assert!(holdings.is_empty());
}

#[test]
fn test_holdings_to_balances_basic() {
    let holdings = vec![
        TokenHolding {
            token: "wrap.near".to_string(),
            balance: "5000000000000000000000000".to_string(),
            decimals: 24,
        },
        TokenHolding {
            token: "usdt.tether-token.near".to_string(),
            balance: "1000000".to_string(),
            decimals: 6,
        },
    ];

    let balances = holdings_to_balances(&holdings).unwrap();

    assert_eq!(balances.len(), 2);

    let wnear = &balances[&ta("wrap.near")];
    assert_eq!(
        wnear.smallest_units(),
        &BigDecimal::from(5_000_000_000_000_000_000_000_000u128)
    );
    assert_eq!(wnear.decimals(), 24);

    let usdt = &balances[&ta("usdt.tether-token.near")];
    assert_eq!(usdt.smallest_units(), &BigDecimal::from(1_000_000u64));
    assert_eq!(usdt.decimals(), 6);
}

#[test]
fn test_holdings_to_balances_empty() {
    let holdings: Vec<TokenHolding> = vec![];
    let balances = holdings_to_balances(&holdings).unwrap();
    assert!(balances.is_empty());
}

#[test]
fn test_holdings_to_balances_invalid_balance() {
    let holdings = vec![TokenHolding {
        token: "wrap.near".to_string(),
        balance: "not_a_number".to_string(),
        decimals: 24,
    }];

    let result = holdings_to_balances(&holdings);
    assert!(result.is_err());
}

#[test]
fn test_holdings_to_balances_large_values() {
    // u128::MAX に近い値
    let holdings = vec![TokenHolding {
        token: "wrap.near".to_string(),
        balance: "340282366920938463463374607431768211455".to_string(),
        decimals: 24,
    }];

    let balances = holdings_to_balances(&holdings).unwrap();
    let wnear = &balances[&ta("wrap.near")];
    assert_eq!(
        wnear.smallest_units().to_string(),
        "340282366920938463463374607431768211455"
    );
    assert_eq!(wnear.decimals(), 24);
}

#[test]
fn test_roundtrip_balances_to_holdings_to_balances() {
    let mut original = BTreeMap::new();
    original.insert(
        ta("wrap.near"),
        TokenAmount::from_smallest_units(BigDecimal::from(1_234_567_890u64), 24),
    );
    original.insert(
        ta("usdt.tether-token.near"),
        TokenAmount::from_smallest_units(BigDecimal::from(999_999u64), 6),
    );
    original.insert(
        ta("aurora"),
        TokenAmount::from_smallest_units(BigDecimal::from(50_000_000_000_000_000_000u128), 18),
    );

    let holdings = balances_to_holdings(&original);
    let restored = holdings_to_balances(&holdings).unwrap();

    assert_eq!(original.len(), restored.len());
    for (token, amount) in &original {
        let restored_amount = &restored[token];
        assert_eq!(amount.smallest_units(), restored_amount.smallest_units());
        assert_eq!(amount.decimals(), restored_amount.decimals());
    }
}

#[test]
fn test_roundtrip_excludes_zero_balances() {
    let mut original = BTreeMap::new();
    original.insert(
        ta("wrap.near"),
        TokenAmount::from_smallest_units(BigDecimal::from(100u64), 24),
    );
    original.insert(
        ta("zero-token.near"),
        TokenAmount::from_smallest_units(BigDecimal::from(0u64), 18),
    );

    let holdings = balances_to_holdings(&original);
    assert_eq!(holdings.len(), 1);

    let restored = holdings_to_balances(&holdings).unwrap();
    assert_eq!(restored.len(), 1);
    assert!(restored.contains_key(&ta("wrap.near")));
    assert!(!restored.contains_key(&ta("zero-token.near")));
}

#[test]
fn test_holdings_preserves_decimals() {
    let mut balances = BTreeMap::new();
    // 異なる decimals のトークンを複数追加
    balances.insert(
        ta("token-6.near"),
        TokenAmount::from_smallest_units(BigDecimal::from(1u64), 6),
    );
    balances.insert(
        ta("token-8.near"),
        TokenAmount::from_smallest_units(BigDecimal::from(1u64), 8),
    );
    balances.insert(
        ta("token-18.near"),
        TokenAmount::from_smallest_units(BigDecimal::from(1u64), 18),
    );
    balances.insert(
        ta("token-24.near"),
        TokenAmount::from_smallest_units(BigDecimal::from(1u64), 24),
    );

    let holdings = balances_to_holdings(&balances);
    let restored = holdings_to_balances(&holdings).unwrap();

    assert_eq!(restored[&ta("token-6.near")].decimals(), 6);
    assert_eq!(restored[&ta("token-8.near")].decimals(), 8);
    assert_eq!(restored[&ta("token-18.near")].decimals(), 18);
    assert_eq!(restored[&ta("token-24.near")].decimals(), 24);
}

#[test]
fn test_holdings_to_balances_zero_balance_string() {
    let holdings = vec![TokenHolding {
        token: "wrap.near".to_string(),
        balance: "0".to_string(),
        decimals: 24,
    }];

    let balances = holdings_to_balances(&holdings).unwrap();
    assert_eq!(balances.len(), 1);
    assert!(balances[&ta("wrap.near")].is_zero());
}
