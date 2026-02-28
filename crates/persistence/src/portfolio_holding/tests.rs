use super::*;
use crate::evaluation_period::NewEvaluationPeriod;
use bigdecimal::BigDecimal;
use chrono::Timelike;
use serial_test::serial;

fn create_test_holdings_json() -> serde_json::Value {
    serde_json::to_value(vec![
        TokenHolding {
            token: "wrap.near".to_string(),
            balance: "1000000000000000000000000".to_string(),
            decimals: 24,
        },
        TokenHolding {
            token: "usdt.tether-token.near".to_string(),
            balance: "5000000".to_string(),
            decimals: 6,
        },
    ])
    .unwrap()
}

async fn create_test_evaluation_period() -> String {
    let new_period =
        NewEvaluationPeriod::new(BigDecimal::from(100000000000000000000000000i128), vec![]);
    let created = new_period.insert_async().await.unwrap();
    created.period_id
}

async fn cleanup_holdings_for_period(period_id: &str) {
    let conn = crate::connection_pool::get().await.unwrap();
    let pid = period_id.to_string();
    conn.interact(move |conn| {
        diesel::delete(
            portfolio_holdings::table.filter(portfolio_holdings::evaluation_period_id.eq(&pid)),
        )
        .execute(conn)
    })
    .await
    .unwrap()
    .unwrap();
}

// --- DB integration tests ---

#[tokio::test]
#[serial(portfolio_holding)]
async fn test_insert_and_get_by_period() -> Result<()> {
    let period_id = create_test_evaluation_period().await;
    let holdings_json = create_test_holdings_json();
    let now = chrono::Utc::now().naive_utc();

    // Insert 3 records with different timestamps
    for i in 0..3 {
        let record = NewPortfolioHolding {
            evaluation_period_id: period_id.clone(),
            timestamp: now - chrono::Duration::seconds(i),
            token_holdings: holdings_json.clone(),
        };
        PortfolioHolding::insert_async(record).await?;
    }

    let results = PortfolioHolding::get_by_period_async(period_id.clone()).await?;
    assert_eq!(results.len(), 3);

    // Verify timestamp descending order
    for i in 0..results.len() - 1 {
        assert!(results[i].timestamp >= results[i + 1].timestamp);
    }

    // Verify JSONB round-trip via parse_holdings
    let holdings = results[0].parse_holdings()?;
    assert_eq!(holdings.len(), 2);
    assert_eq!(holdings[0].token, "wrap.near");
    assert_eq!(holdings[0].balance, "1000000000000000000000000");
    assert_eq!(holdings[1].token, "usdt.tether-token.near");
    assert_eq!(holdings[1].balance, "5000000");

    cleanup_holdings_for_period(&period_id).await;
    Ok(())
}

#[tokio::test]
#[serial(portfolio_holding)]
async fn test_get_latest_for_period() -> Result<()> {
    let period_id = create_test_evaluation_period().await;
    let holdings_json = create_test_holdings_json();
    // PostgreSQL TIMESTAMP has microsecond precision, so truncate nanoseconds
    let now = chrono::Utc::now().naive_utc();
    let now = now
        .with_nanosecond(now.nanosecond() / 1_000 * 1_000)
        .unwrap();

    let older = NewPortfolioHolding {
        evaluation_period_id: period_id.clone(),
        timestamp: now - chrono::Duration::seconds(10),
        token_holdings: holdings_json.clone(),
    };
    PortfolioHolding::insert_async(older).await?;

    let newer = NewPortfolioHolding {
        evaluation_period_id: period_id.clone(),
        timestamp: now,
        token_holdings: holdings_json,
    };
    PortfolioHolding::insert_async(newer).await?;

    // Should return the record with the latest timestamp
    let latest = PortfolioHolding::get_latest_for_period_async(period_id.clone()).await?;
    assert!(latest.is_some());
    assert_eq!(latest.unwrap().timestamp, now);

    // Non-existent period_id returns None
    let none =
        PortfolioHolding::get_latest_for_period_async("non_existent_period".to_string()).await?;
    assert!(none.is_none());

    cleanup_holdings_for_period(&period_id).await;
    Ok(())
}

#[tokio::test]
#[serial(portfolio_holding)]
async fn test_cleanup_old_records() -> Result<()> {
    let period_id = create_test_evaluation_period().await;
    let holdings_json = create_test_holdings_json();
    let now = chrono::Utc::now().naive_utc();

    // Insert an old record (10 days ago)
    let old_record = NewPortfolioHolding {
        evaluation_period_id: period_id.clone(),
        timestamp: now - chrono::Duration::days(10),
        token_holdings: holdings_json.clone(),
    };
    PortfolioHolding::insert_async(old_record).await?;

    // Insert a recent record (now)
    let new_record = NewPortfolioHolding {
        evaluation_period_id: period_id.clone(),
        timestamp: now,
        token_holdings: holdings_json,
    };
    PortfolioHolding::insert_async(new_record).await?;

    // Cleanup records older than 5 days
    let deleted = PortfolioHolding::cleanup_old_records(5).await?;
    assert!(deleted >= 1);

    // Only the recent record should remain for this period
    let remaining = PortfolioHolding::get_by_period_async(period_id.clone()).await?;
    assert_eq!(remaining.len(), 1);
    assert!(remaining[0].timestamp > now - chrono::Duration::days(5));

    cleanup_holdings_for_period(&period_id).await;
    Ok(())
}

#[tokio::test]
#[serial(portfolio_holding)]
async fn test_insert_with_invalid_period_id() {
    let holdings_json = create_test_holdings_json();
    let now = chrono::Utc::now().naive_utc();

    let record = NewPortfolioHolding {
        evaluation_period_id: "non_existent_period_id".to_string(),
        timestamp: now,
        token_holdings: holdings_json,
    };

    // FK constraint should cause an error
    let result = PortfolioHolding::insert_async(record).await;
    assert!(result.is_err());
}

// --- Unit tests ---

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
