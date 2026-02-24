use super::*;

#[test]
fn test_token_holding_serialization() {
    let holdings = vec![
        TokenHolding {
            token: "usdt.tether-token.near".to_string(),
            balance: "1000000".to_string(),
            decimals: 6,
        },
        TokenHolding {
            token: "wrap.near".to_string(),
            balance: "5000000000000000000000000000".to_string(),
            decimals: 24,
        },
    ];

    let json = serde_json::to_value(&holdings).unwrap();
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 2);

    assert_eq!(arr[0]["token"], "usdt.tether-token.near");
    assert_eq!(arr[0]["balance"], "1000000");
    assert_eq!(arr[0]["decimals"], 6);

    assert_eq!(arr[1]["token"], "wrap.near");
    assert_eq!(arr[1]["balance"], "5000000000000000000000000000");
    assert_eq!(arr[1]["decimals"], 24);
}

#[test]
fn test_token_holding_deserialization() {
    let json = serde_json::json!([
        {"token": "usdt.tether-token.near", "balance": "1000000", "decimals": 6},
        {"token": "wrap.near", "balance": "5000000000000000000000000000", "decimals": 24}
    ]);

    let holdings: Vec<TokenHolding> = serde_json::from_value(json).unwrap();
    assert_eq!(holdings.len(), 2);
    assert_eq!(holdings[0].token, "usdt.tether-token.near");
    assert_eq!(holdings[0].balance, "1000000");
    assert_eq!(holdings[0].decimals, 6);
    assert_eq!(holdings[1].token, "wrap.near");
    assert_eq!(holdings[1].decimals, 24);
}

#[test]
fn test_token_holding_roundtrip() {
    let original = vec![
        TokenHolding {
            token: "token-a.near".to_string(),
            balance: "123456789012345678901234".to_string(),
            decimals: 18,
        },
        TokenHolding {
            token: "token-b.near".to_string(),
            balance: "0".to_string(),
            decimals: 8,
        },
    ];

    let json = serde_json::to_value(&original).unwrap();
    let deserialized: Vec<TokenHolding> = serde_json::from_value(json).unwrap();

    assert_eq!(deserialized.len(), 2);
    assert_eq!(deserialized[0].token, original[0].token);
    assert_eq!(deserialized[0].balance, original[0].balance);
    assert_eq!(deserialized[0].decimals, original[0].decimals);
    assert_eq!(deserialized[1].token, original[1].token);
    assert_eq!(deserialized[1].balance, original[1].balance);
    assert_eq!(deserialized[1].decimals, original[1].decimals);
}

#[test]
fn test_parse_holdings_valid_jsonb() {
    let json = serde_json::json!([
        {"token": "wrap.near", "balance": "1000000000000000000000000", "decimals": 24}
    ]);

    let record = DbPortfolioHolding {
        id: 1,
        evaluation_period_id: "eval_test".to_string(),
        timestamp: chrono::Utc::now().naive_utc(),
        token_holdings: json,
        created_at: chrono::Utc::now().naive_utc(),
    };

    let holdings = record.parse_holdings().unwrap();
    assert_eq!(holdings.len(), 1);
    assert_eq!(holdings[0].token, "wrap.near");
    assert_eq!(holdings[0].balance, "1000000000000000000000000");
    assert_eq!(holdings[0].decimals, 24);
}

#[test]
fn test_parse_holdings_empty_array() {
    let record = DbPortfolioHolding {
        id: 1,
        evaluation_period_id: "eval_test".to_string(),
        timestamp: chrono::Utc::now().naive_utc(),
        token_holdings: serde_json::json!([]),
        created_at: chrono::Utc::now().naive_utc(),
    };

    let holdings = record.parse_holdings().unwrap();
    assert!(holdings.is_empty());
}

#[test]
fn test_parse_holdings_invalid_jsonb() {
    let record = DbPortfolioHolding {
        id: 1,
        evaluation_period_id: "eval_test".to_string(),
        timestamp: chrono::Utc::now().naive_utc(),
        token_holdings: serde_json::json!({"not": "an array"}),
        created_at: chrono::Utc::now().naive_utc(),
    };

    let result = record.parse_holdings();
    assert!(result.is_err());
}

#[test]
fn test_new_portfolio_holding_construction() {
    let holdings = vec![TokenHolding {
        token: "wrap.near".to_string(),
        balance: "100".to_string(),
        decimals: 24,
    }];
    let json = serde_json::to_value(&holdings).unwrap();

    let record = NewPortfolioHolding {
        evaluation_period_id: "eval_123".to_string(),
        timestamp: chrono::Utc::now().naive_utc(),
        token_holdings: json.clone(),
    };

    assert_eq!(record.evaluation_period_id, "eval_123");
    assert_eq!(record.token_holdings, json);
}

#[test]
fn test_token_holding_large_balance() {
    // u128::MAX に近い値でもシリアライズ可能
    let holding = TokenHolding {
        token: "wrap.near".to_string(),
        balance: "340282366920938463463374607431768211455".to_string(),
        decimals: 24,
    };

    let json = serde_json::to_value(&holding).unwrap();
    let deserialized: TokenHolding = serde_json::from_value(json).unwrap();
    assert_eq!(deserialized.balance, holding.balance);
}

#[test]
fn test_parse_holdings_multiple_tokens() {
    let json = serde_json::json!([
        {"token": "usdt.tether-token.near", "balance": "1000000", "decimals": 6},
        {"token": "usdc.near", "balance": "2000000", "decimals": 6},
        {"token": "wrap.near", "balance": "5000000000000000000000000", "decimals": 24},
        {"token": "aurora", "balance": "100000000000000000000", "decimals": 18}
    ]);

    let record = DbPortfolioHolding {
        id: 1,
        evaluation_period_id: "eval_test".to_string(),
        timestamp: chrono::Utc::now().naive_utc(),
        token_holdings: json,
        created_at: chrono::Utc::now().naive_utc(),
    };

    let holdings = record.parse_holdings().unwrap();
    assert_eq!(holdings.len(), 4);

    let tokens: Vec<&str> = holdings.iter().map(|h| h.token.as_str()).collect();
    assert!(tokens.contains(&"usdt.tether-token.near"));
    assert!(tokens.contains(&"usdc.near"));
    assert!(tokens.contains(&"wrap.near"));
    assert!(tokens.contains(&"aurora"));
}
