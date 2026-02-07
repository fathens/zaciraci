use bigdecimal::BigDecimal;
use num_traits::Zero;
use std::collections::BTreeMap;
use std::str::FromStr;
use zaciraci_common::algorithm::types::TradingAction;
use zaciraci_common::types::TokenOutAccount;

fn token_out(s: &str) -> TokenOutAccount {
    s.parse().unwrap()
}

// Note: swap.rs の関数は外部依存(jsonrpc, wallet, database)が多いため、
// ここでは構造体やロジックのユニットテストのみを実施します。
// 実際のスワップ実行テストは統合テストで実施する必要があります。

#[test]
fn test_trading_action_hold() {
    // Hold アクションは何もしないことを確認
    let action = TradingAction::Hold;

    match action {
        TradingAction::Hold => {} // Success
        _ => panic!("Expected Hold action"),
    }
}

fn token(s: &str) -> zaciraci_common::types::TokenOutAccount {
    s.parse().unwrap()
}

#[test]
fn test_trading_action_sell_structure() {
    let action = TradingAction::Sell {
        token: token("token1.near"),
        target: token("token2.near"),
    };

    match action {
        TradingAction::Sell { token: t, target } => {
            assert_eq!(t, token("token1.near"));
            assert_eq!(target, token("token2.near"));
        }
        _ => panic!("Expected Sell action"),
    }
}

#[test]
fn test_trading_action_rebalance_structure() {
    let mut weights = BTreeMap::new();
    weights.insert(token_out("token1.near"), 0.5);
    weights.insert(token_out("token2.near"), 0.5);

    let action = TradingAction::Rebalance {
        target_weights: weights.clone(),
    };

    match action {
        TradingAction::Rebalance { target_weights } => {
            assert_eq!(target_weights.len(), 2);
            assert_eq!(target_weights.get(&token_out("token1.near")), Some(&0.5));
            assert_eq!(target_weights.get(&token_out("token2.near")), Some(&0.5));
        }
        _ => panic!("Expected Rebalance action"),
    }
}

// Note: エラーケースのテストは統合テストで実施
// 実際のデータベースやクライアントが必要なため、ここでは省略

// BigDecimal計算のユニットテスト
#[test]
fn test_bigdecimal_division_precision() {
    let balance = BigDecimal::from(1000000000000000000000000u128);
    let rate = BigDecimal::from_str("1.5").unwrap();

    let result = balance / rate;

    assert!(result > 0, "Division should produce positive result");
}

#[test]
fn test_bigdecimal_zero_handling() {
    let zero = BigDecimal::from(0);

    assert!(
        zero.is_zero(),
        "Zero BigDecimal should be identified as zero"
    );
}
