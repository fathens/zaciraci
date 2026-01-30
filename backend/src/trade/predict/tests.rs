use super::*;
use crate::Result;
use crate::persistence::token_rate::TokenRate;
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use bigdecimal::BigDecimal;
use chrono::{Duration, NaiveDateTime, Utc};
use serial_test::serial;
use std::str::FromStr;
use zaciraci_common::prediction::ChronosPredictionResponse;
use zaciraci_common::types::{ExchangeRate, TokenPrice};

fn price(s: &str) -> TokenPrice {
    TokenPrice::from_near_per_token(BigDecimal::from_str(s).unwrap())
}

/// テスト用ヘルパー: 文字列から ExchangeRate を作成 (decimals = 24)
fn make_rate_from_str(s: &str) -> ExchangeRate {
    ExchangeRate::from_raw_rate(BigDecimal::from_str(s).unwrap(), 24)
}

/// テスト用ヘルパー: TokenRate を簡潔に作成
fn make_token_rate(
    base: TokenOutAccount,
    quote: TokenInAccount,
    rate_str: &str,
    timestamp: NaiveDateTime,
) -> TokenRate {
    TokenRate {
        base,
        quote,
        exchange_rate: make_rate_from_str(rate_str),
        timestamp,
    }
}

// テスト用のヘルパー構造体
struct TestFixture {
    pub quote_token: TokenInAccount,
    pub test_tokens: Vec<(TokenOutAccount, String)>,
}

impl TestFixture {
    fn new() -> Self {
        Self {
            quote_token: "wrap.near".parse::<TokenAccount>().unwrap().into(),
            test_tokens: vec![
                (
                    "test_token1.near".parse::<TokenAccount>().unwrap().into(),
                    "test_token1.near".to_string(),
                ),
                (
                    "test_token2.near".parse::<TokenAccount>().unwrap().into(),
                    "test_token2.near".to_string(),
                ),
            ],
        }
    }

    async fn setup_volatility_data(&self) -> Result<()> {
        let now = Utc::now().naive_utc();
        let mut rates = Vec::new();

        // 各トークンに異なるボラティリティパターンを設定
        for (i, (token, _)) in self.test_tokens.iter().enumerate() {
            let base_price = 1.0 + i as f64;
            let volatility_factor = 0.1 + i as f64 * 0.05;

            for hour in 1..=24 {
                let price_variation = (hour as f64 * 0.1).sin() * volatility_factor;
                let price = base_price + price_variation;

                let rate = make_token_rate(
                    token.clone(),
                    self.quote_token.clone(),
                    &format!("{:.4}", price),
                    now - chrono::Duration::hours(hour),
                );
                rates.push(rate);
            }
        }

        TokenRate::batch_insert(&rates).await?;
        Ok(())
    }

    async fn setup_price_history(&self, token: &TokenOutAccount, prices: &[f64]) -> Result<()> {
        let now = Utc::now().naive_utc();
        let mut rates = Vec::new();

        for (i, price) in prices.iter().enumerate() {
            let timestamp = now - chrono::Duration::hours((prices.len() - i - 1) as i64);
            let rate = make_token_rate(
                token.clone(),
                self.quote_token.clone(),
                &price.to_string(),
                timestamp,
            );
            rates.push(rate);
        }

        TokenRate::batch_insert(&rates).await?;
        Ok(())
    }
}

// テーブルクリーンアップ用関数（テスト専用）
async fn clean_test_tokens() -> Result<()> {
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_get_top_tokens_with_specific_volatility() -> Result<()> {
    clean_test_tokens().await?;

    let fixture = TestFixture::new();
    fixture.setup_volatility_data().await?;

    let service = PredictionService::new();
    let start_date = Utc::now() - Duration::days(1);
    let end_date = Utc::now();

    let result = service
        .get_tokens_by_volatility(start_date, end_date, &fixture.quote_token)
        .await;

    assert!(result.is_ok(), "get_tokens_by_volatility should succeed");
    let tokens = result.unwrap();

    assert!(tokens.len() >= 2, "Should return at least 2 test tokens");

    for token in &tokens {
        assert!(
            !token.token.to_string().is_empty(),
            "Token name should not be empty"
        );
        assert!(
            token.volatility >= BigDecimal::from(0),
            "Volatility should be non-negative"
        );
    }

    for i in 1..tokens.len() {
        assert!(
            tokens[i - 1].volatility >= tokens[i].volatility,
            "Tokens should be ordered by volatility (descending): {} >= {}",
            tokens[i - 1].volatility,
            tokens[i].volatility
        );
    }

    clean_test_tokens().await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_get_price_history_data_integrity() -> Result<()> {
    clean_test_tokens().await?;

    let fixture = TestFixture::new();
    let test_token: TokenOutAccount = "price_test.near".parse::<TokenAccount>().unwrap().into();
    let expected_prices = vec![1.0, 1.1, 1.05, 1.12, 1.15, 1.18];

    fixture
        .setup_price_history(&test_token, &expected_prices)
        .await?;

    let service = PredictionService::new();
    let start_date = Utc::now() - Duration::hours(10);
    let end_date = Utc::now();

    let result = service
        .get_price_history(&test_token, &fixture.quote_token, start_date, end_date)
        .await;

    assert!(result.is_ok(), "get_price_history should succeed");
    let history = result.unwrap();

    assert_eq!(history.token, test_token);
    assert_eq!(history.quote_token, fixture.quote_token);
    assert_eq!(
        history.prices.len(),
        expected_prices.len(),
        "Should return all inserted prices"
    );

    for i in 1..history.prices.len() {
        assert!(
            history.prices[i - 1].timestamp <= history.prices[i].timestamp,
            "Prices should be ordered chronologically: {} <= {}",
            history.prices[i - 1].timestamp,
            history.prices[i].timestamp
        );
    }

    let yocto_per_near = BigDecimal::from_str("1000000000000000000000000").unwrap();
    let expected_token_prices: Vec<f64> = expected_prices
        .iter()
        .map(|rate| {
            let rate_bd = BigDecimal::from_str(&rate.to_string()).unwrap();
            (&yocto_per_near / rate_bd)
                .to_string()
                .parse::<f64>()
                .unwrap()
        })
        .collect();

    let actual_prices: Vec<f64> = history
        .prices
        .iter()
        .map(|p| p.price.to_f64().as_f64())
        .collect();

    for expected_price in &expected_token_prices {
        assert!(
            actual_prices
                .iter()
                .any(|&p| ((p - expected_price) / expected_price).abs() < 0.01),
            "Expected TokenPrice {} should be present in history (actual: {:?})",
            expected_price,
            actual_prices
        );
    }

    clean_test_tokens().await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_convert_prediction_result() {
    let service = PredictionService::new();

    let now = Utc::now();
    let chronos_response = ChronosPredictionResponse {
        forecast_timestamp: vec![
            now + Duration::hours(1),
            now + Duration::hours(2),
            now + Duration::hours(3),
            now + Duration::hours(4),
        ],
        forecast_values: vec![
            "1.2".parse().unwrap(),
            "1.3".parse().unwrap(),
            "1.4".parse().unwrap(),
            "1.5".parse().unwrap(),
        ],
        model_name: "chronos-t5-large".to_string(),
        confidence_intervals: None,
        metrics: None,
        strategy_name: Some("ensemble".to_string()),
        processing_time_secs: Some(1.5),
        model_count: Some(3),
    };

    let predictions = service.convert_prediction_result(&chronos_response, 3);

    assert!(predictions.is_ok());
    let preds = predictions.unwrap();
    assert_eq!(preds.len(), 3);
    assert_eq!(preds[0].price, price("1.2"));
    assert_eq!(preds[1].price, price("1.3"));
    assert_eq!(preds[2].price, price("1.4"));

    // タイムスタンプが正しく設定されていることを確認
    assert_eq!(preds[0].timestamp, now + Duration::hours(1));
    assert_eq!(preds[1].timestamp, now + Duration::hours(2));
    assert_eq!(preds[2].timestamp, now + Duration::hours(3));
}

#[tokio::test]
#[serial]
async fn test_error_handling_comprehensive() -> Result<()> {
    let service = PredictionService::new();

    let start_date = Utc::now() - Duration::days(7);
    let end_date = Utc::now();

    let invalid_tokens = vec![
        ("", "Empty token name"),
        ("invalid token with spaces", "Token with spaces"),
        ("token\nwith\nnewlines", "Token with newlines"),
        ("token\twith\ttabs", "Token with tabs"),
        ("token with unicode: ❌", "Token with unicode"),
    ];

    for (invalid_token_str, description) in invalid_tokens {
        let parse_result = invalid_token_str.parse::<TokenAccount>();

        if let Ok(token) = parse_result {
            let invalid_token: TokenInAccount = token.into();
            let result = service
                .get_tokens_by_volatility(start_date, end_date, &invalid_token)
                .await;

            assert!(result.is_err(), "{} should cause an error", description);

            let error = result.unwrap_err();
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("Failed to parse quote token")
                    || error_msg.contains("parse")
                    || error_msg.contains("invalid"),
                "Error message should indicate parsing failure for {}: {}",
                description,
                error_msg
            );
        } else {
            println!("{} failed to parse as expected", description);
        }
    }

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_empty_price_history() {
    let service = PredictionService::new();

    let test_token: TokenOutAccount = "test.near".parse::<TokenAccount>().unwrap().into();
    let quote_token: TokenInAccount = "wrap.near".parse::<TokenAccount>().unwrap().into();
    let history = TokenPriceHistory {
        token: test_token,
        quote_token,
        prices: vec![],
    };

    let result = service.predict_price(&history, 24).await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("No price history available")
    );
}

#[test]
fn test_token_prediction_serialization_roundtrip() {
    let test_token: TokenOutAccount = "test.near".parse::<TokenAccount>().unwrap().into();
    let quote_token: TokenInAccount = "wrap.near".parse::<TokenAccount>().unwrap().into();
    let prediction = TokenPrediction {
        token: test_token,
        quote_token,
        prediction_time: Utc::now(),
        predictions: vec![PredictedPrice {
            timestamp: Utc::now(),
            price: price("1.5"),
            confidence: Some(BigDecimal::from_str("0.85").unwrap()),
        }],
    };

    let json = serde_json::to_string(&prediction).expect("Should serialize successfully");
    let deserialized: TokenPrediction =
        serde_json::from_str(&json).expect("Should deserialize successfully");

    assert_eq!(deserialized.token.to_string(), prediction.token.to_string());
    assert_eq!(
        deserialized.quote_token.to_string(),
        prediction.quote_token.to_string()
    );
    assert_eq!(deserialized.prediction_time, prediction.prediction_time);
    assert_eq!(deserialized.predictions.len(), prediction.predictions.len());

    for (orig, deser) in prediction
        .predictions
        .iter()
        .zip(deserialized.predictions.iter())
    {
        assert_eq!(orig.timestamp, deser.timestamp);
        assert_eq!(orig.price, deser.price);
        assert_eq!(orig.confidence, deser.confidence);
    }
}

#[tokio::test]
#[serial]
async fn test_batch_processing_database_operations() -> Result<()> {
    clean_test_tokens().await?;

    let fixture = TestFixture::new();
    let tokens: Vec<String> = (1..=5).map(|i| format!("batch{}.near", i)).collect();

    let mut all_rates = Vec::new();
    let now = Utc::now().naive_utc();

    for (token_idx, token_name) in tokens.iter().enumerate() {
        let base_token: TokenOutAccount = token_name.parse::<TokenAccount>().unwrap().into();
        let base_price = 1.0 + token_idx as f64 * 0.5;

        for hour in 1..=6 {
            let price = base_price + (hour as f64 * 0.1);
            let rate = make_token_rate(
                base_token.clone(),
                fixture.quote_token.clone(),
                &format!("{:.3}", price),
                now - chrono::Duration::hours(hour),
            );
            all_rates.push(rate);
        }
    }

    TokenRate::batch_insert(&all_rates).await?;

    let service = PredictionService::new();

    let start_date = Utc::now() - Duration::hours(10);
    let end_date = Utc::now();

    let mut successful_retrievals = 0;
    for token_name in &tokens {
        let token: TokenOutAccount = token_name.parse::<TokenAccount>().unwrap().into();
        let result = service
            .get_price_history(&token, &fixture.quote_token, start_date, end_date)
            .await;

        if result.is_ok() {
            let history = result.unwrap();
            assert_eq!(history.token, token, "Token name should match");
            assert_eq!(
                history.quote_token, fixture.quote_token,
                "Quote token should match"
            );
            assert!(
                !history.prices.is_empty(),
                "Should have price data for {}",
                token_name
            );

            for price_point in &history.prices {
                assert!(
                    price_point.price.as_bigdecimal() > &BigDecimal::from(0),
                    "Price should be positive"
                );
            }

            successful_retrievals += 1;
        }
    }

    assert!(
        successful_retrievals == tokens.len(),
        "All tokens should be processed successfully, got {}/{}",
        successful_retrievals,
        tokens.len()
    );

    clean_test_tokens().await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_predict_multiple_tokens_partial_success() -> Result<()> {
    clean_test_tokens().await?;

    let fixture = TestFixture::new();

    let existing_token: TokenOutAccount = "existing.near".parse::<TokenAccount>().unwrap().into();
    let prices = vec![1.0, 1.1, 1.05, 1.12, 1.15];
    fixture
        .setup_price_history(&existing_token, &prices)
        .await?;

    let service = PredictionService::new();

    let tokens: Vec<TokenOutAccount> = vec![
        "existing.near".parse::<TokenAccount>().unwrap().into(),
        "nonexistent1.near".parse::<TokenAccount>().unwrap().into(),
        "nonexistent2.near".parse::<TokenAccount>().unwrap().into(),
    ];

    let _result = service
        .predict_multiple_tokens(tokens, &fixture.quote_token, 1, 24)
        .await;

    clean_test_tokens().await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_predict_multiple_tokens_all_fail() -> Result<()> {
    clean_test_tokens().await?;

    let service = PredictionService::new();
    let quote_token: TokenInAccount = "wrap.near".parse::<TokenAccount>().unwrap().into();

    let tokens: Vec<TokenOutAccount> = vec![
        "nonexistent1.near".parse::<TokenAccount>().unwrap().into(),
        "nonexistent2.near".parse::<TokenAccount>().unwrap().into(),
        "nonexistent3.near".parse::<TokenAccount>().unwrap().into(),
    ];

    let result = service
        .predict_multiple_tokens(tokens, &quote_token, 1, 24)
        .await;

    assert!(result.is_err(), "Should fail when all tokens fail");

    let error = result.unwrap_err();
    let error_msg = error.to_string();
    assert!(
        error_msg.contains("Failed to predict any tokens")
            || error_msg.contains("Failed to get price history"),
        "Error message should indicate failure: {}",
        error_msg
    );

    clean_test_tokens().await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_retry_configuration() {
    let service = PredictionService::new();

    assert_eq!(
        service.max_retries, 2,
        "max_retries should be 2 from config"
    );
    assert_eq!(
        service.retry_delay_seconds, 5,
        "retry_delay_seconds should be 5 from config"
    );
}

#[tokio::test]
#[serial]
async fn test_empty_token_list() -> Result<()> {
    let service = PredictionService::new();
    let tokens: Vec<TokenOutAccount> = vec![];
    let quote_token: TokenInAccount = "wrap.near".parse::<TokenAccount>().unwrap().into();

    let result = service
        .predict_multiple_tokens(tokens, &quote_token, 1, 24)
        .await;

    assert!(result.is_err(), "Should fail with empty token list");

    let error = result.unwrap_err();
    assert!(
        error.to_string().contains("Failed to predict any tokens"),
        "Error should indicate no tokens were predicted"
    );

    Ok(())
}

#[test]
fn test_price_point_validation() {
    use zaciraci_common::algorithm::types::PricePoint;

    let now = Utc::now();
    let price_point = PricePoint {
        timestamp: now,
        price: price("1.5"),
        volume: Some(BigDecimal::from_str("1000.0").unwrap()),
    };

    let json = serde_json::to_string(&price_point).expect("Should serialize");
    let deserialized: PricePoint = serde_json::from_str(&json).expect("Should deserialize");

    assert_eq!(price_point.timestamp, deserialized.timestamp);
    assert_eq!(price_point.price, deserialized.price);
    assert_eq!(price_point.volume, deserialized.volume);
}

#[tokio::test]
#[serial]
async fn test_invalid_quote_token() -> Result<()> {
    let invalid_token_str = "invalid token name";
    let parse_result = invalid_token_str.parse::<TokenAccount>();

    assert!(
        parse_result.is_err(),
        "Invalid token name should fail to parse"
    );

    Ok(())
}
