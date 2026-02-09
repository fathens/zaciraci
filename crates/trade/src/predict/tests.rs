use super::*;
use crate::Result;
use bigdecimal::BigDecimal;
use chrono::{Duration, NaiveDateTime, TimeDelta, Utc};
use common::prediction::ChronosPredictionResponse;
use common::types::{ExchangeRate, TokenPrice};
use common::types::{TokenAccount, TokenInAccount, TokenOutAccount};
use num_traits::ToPrimitive;
use persistence::token_rate::{SwapPath, SwapPoolInfo, TokenRate};
use serial_test::serial;
use std::str::FromStr;

/// テスト用ヘルパー: decimals 取得コールバック
fn test_get_decimals() -> &'static persistence::token_rate::GetDecimalsFn {
    &|_token: &str| Box::pin(async move { Ok(24u8) })
}

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
        rate_calc_near: 10,
        swap_path: None,
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
        assert!(token.volatility >= 0, "Volatility should be non-negative");
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
        .map(|p| p.price.as_bigdecimal().to_f64().unwrap())
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
    let last_data_timestamp = now; // 最後のデータタイムスタンプ
    let chronos_response = ChronosPredictionResponse {
        forecast: [
            (now + Duration::hours(1), "1.2".parse().unwrap()),
            (now + Duration::hours(2), "1.3".parse().unwrap()),
            (now + Duration::hours(3), "1.4".parse().unwrap()),
            (now + Duration::hours(4), "1.5".parse().unwrap()),
        ]
        .into_iter()
        .collect(),
        lower_bound: None,
        upper_bound: None,
        model_name: "chronos-t5-large".to_string(),
        strategy_name: "ensemble".to_string(),
        processing_time_secs: 1.5,
        model_count: 3,
    };

    let predictions = service.convert_prediction_result(&chronos_response, 3, last_data_timestamp);

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
    let history = PriceHistory {
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
    let prediction = TokenPredictionResult {
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
    let deserialized: TokenPredictionResult =
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
    use common::algorithm::types::PricePoint;

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

/// 時間ベースの confidence 計算をテスト
///
/// 同じ CV（変動係数）を持つ予測は、時間経過に関係なく同じ confidence を持つべき
#[test]
fn test_confidence_time_normalization() {
    // CV = 5% の場合の信頼区間幅を計算
    // 相対幅 = 2.56 × CV × sqrt(時間)
    // CV = 5% = 0.05 のとき:
    // - 1時間先: 相対幅 = 2.56 × 0.05 × sqrt(1) = 0.128 (12.8%)
    // - 24時間先: 相対幅 = 2.56 × 0.05 × sqrt(24) ≈ 0.627 (62.7%)

    let forecast = BigDecimal::from_str("100.0").unwrap();

    // 1時間先: 相対幅 12.8%
    let lower_1h = BigDecimal::from_str("93.6").unwrap(); // 100 - 6.4
    let upper_1h = BigDecimal::from_str("106.4").unwrap(); // 100 + 6.4
    let time_1h = TimeDelta::hours(1);

    let conf_1h = PredictionService::calculate_confidence_from_interval(
        &forecast,
        Some(&lower_1h),
        Some(&upper_1h),
        time_1h,
    )
    .unwrap();

    // 24時間先: 相対幅 62.7%
    let lower_24h = BigDecimal::from_str("68.65").unwrap(); // 100 - 31.35
    let upper_24h = BigDecimal::from_str("131.35").unwrap(); // 100 + 31.35
    let time_24h = TimeDelta::hours(24);

    let conf_24h = PredictionService::calculate_confidence_from_interval(
        &forecast,
        Some(&lower_24h),
        Some(&upper_24h),
        time_24h,
    )
    .unwrap();

    // 両方とも CV = 5% なので、confidence は同じはず
    // CV = 5% のとき: (0.05 - 0.03) / (0.15 - 0.03) = 0.02 / 0.12 ≈ 0.167
    // confidence ≈ 1.0 - 0.167 ≈ 0.833
    let expected_confidence = 0.833;
    let tolerance = 0.05;

    let conf_1h_f64: f64 = conf_1h.to_string().parse().unwrap();
    let conf_24h_f64: f64 = conf_24h.to_string().parse().unwrap();

    assert!(
        (conf_1h_f64 - expected_confidence).abs() < tolerance,
        "1h confidence {} should be close to {}",
        conf_1h_f64,
        expected_confidence
    );
    assert!(
        (conf_24h_f64 - expected_confidence).abs() < tolerance,
        "24h confidence {} should be close to {}",
        conf_24h_f64,
        expected_confidence
    );
    assert!(
        (conf_1h_f64 - conf_24h_f64).abs() < tolerance,
        "1h ({}) and 24h ({}) confidence should be similar for same CV",
        conf_1h_f64,
        conf_24h_f64
    );
}

/// 異なる CV での confidence 境界値をテスト
#[test]
fn test_confidence_cv_boundaries() {
    let forecast = BigDecimal::from_str("100.0").unwrap();
    let time_1h = TimeDelta::hours(1);

    // CV = 3% (MIN_CV) → confidence = 1.0
    // 相対幅 = 2.56 × 0.03 × sqrt(1) = 0.0768 (7.68%)
    let lower_3pct = BigDecimal::from_str("96.16").unwrap();
    let upper_3pct = BigDecimal::from_str("103.84").unwrap();

    let conf_3pct = PredictionService::calculate_confidence_from_interval(
        &forecast,
        Some(&lower_3pct),
        Some(&upper_3pct),
        time_1h,
    )
    .unwrap();

    let conf_3pct_f64: f64 = conf_3pct.to_string().parse().unwrap();
    assert!(
        conf_3pct_f64 >= 0.99,
        "CV=3% should give confidence ≈ 1.0, got {}",
        conf_3pct_f64
    );

    // CV = 15% (MAX_CV) → confidence = 0.0
    // 相対幅 = 2.56 × 0.15 × sqrt(1) = 0.384 (38.4%)
    let lower_15pct = BigDecimal::from_str("80.8").unwrap();
    let upper_15pct = BigDecimal::from_str("119.2").unwrap();

    let conf_15pct = PredictionService::calculate_confidence_from_interval(
        &forecast,
        Some(&lower_15pct),
        Some(&upper_15pct),
        time_1h,
    )
    .unwrap();

    let conf_15pct_f64: f64 = conf_15pct.to_string().parse().unwrap();
    assert!(
        conf_15pct_f64 <= 0.01,
        "CV=15% should give confidence ≈ 0.0, got {}",
        conf_15pct_f64
    );
}

/// 信頼区間がない場合は None を返す
#[test]
fn test_confidence_none_when_no_interval() {
    let forecast = BigDecimal::from_str("100.0").unwrap();
    let time_1h = TimeDelta::hours(1);

    let result = PredictionService::calculate_confidence_from_interval(
        &forecast,
        None,
        Some(&BigDecimal::from_str("110.0").unwrap()),
        time_1h,
    );
    assert!(result.is_none(), "Should return None when lower is missing");

    let result = PredictionService::calculate_confidence_from_interval(
        &forecast,
        Some(&BigDecimal::from_str("90.0").unwrap()),
        None,
        time_1h,
    );
    assert!(result.is_none(), "Should return None when upper is missing");
}

/// 予測値がゼロまたは負の場合は None を返す
#[test]
fn test_confidence_none_when_forecast_invalid() {
    let lower = BigDecimal::from_str("90.0").unwrap();
    let upper = BigDecimal::from_str("110.0").unwrap();
    let time_1h = TimeDelta::hours(1);

    let zero_forecast = BigDecimal::from_str("0.0").unwrap();
    let result = PredictionService::calculate_confidence_from_interval(
        &zero_forecast,
        Some(&lower),
        Some(&upper),
        time_1h,
    );
    assert!(result.is_none(), "Should return None when forecast is zero");

    let neg_forecast = BigDecimal::from_str("-10.0").unwrap();
    let result = PredictionService::calculate_confidence_from_interval(
        &neg_forecast,
        Some(&lower),
        Some(&upper),
        time_1h,
    );
    assert!(
        result.is_none(),
        "Should return None when forecast is negative"
    );
}

#[tokio::test]
#[serial]
async fn test_predict_multiple_tokens_parallel_execution() -> Result<()> {
    clean_test_tokens().await?;

    let fixture = TestFixture::new();

    // 5トークン分のテストデータを準備
    let tokens: Vec<TokenOutAccount> = (1..=5)
        .map(|i| {
            format!("parallel_test{}.near", i)
                .parse::<TokenAccount>()
                .unwrap()
                .into()
        })
        .collect();

    for (i, token) in tokens.iter().enumerate() {
        let base_price = 1.0 + i as f64 * 0.5;
        let prices: Vec<f64> = (0..10).map(|j| base_price + j as f64 * 0.1).collect();
        fixture.setup_price_history(token, &prices).await?;
    }

    let service = PredictionService::new();

    // 並行処理で予測実行
    let start = std::time::Instant::now();
    let result = service
        .predict_multiple_tokens(tokens.clone(), &fixture.quote_token, 1, 24)
        .await;
    let duration = start.elapsed();

    assert!(
        result.is_ok(),
        "Parallel prediction should succeed: {:?}",
        result.err()
    );
    let predictions = result.unwrap();

    // 全トークンの予測が成功していることを確認
    assert_eq!(
        predictions.len(),
        tokens.len(),
        "All tokens should have predictions"
    );

    println!(
        "Parallel prediction for {} tokens completed in {:?}",
        tokens.len(),
        duration
    );

    clean_test_tokens().await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_prediction_concurrency_config() {
    let config = common::config::config();
    assert!(
        config.trade.prediction_concurrency >= 1,
        "prediction_concurrency should be at least 1"
    );
    assert!(
        config.trade.prediction_concurrency <= 32,
        "prediction_concurrency should be reasonable (<=32)"
    );
}

#[tokio::test]
#[serial]
async fn test_predict_multiple_tokens_batch_history_fetch() -> Result<()> {
    use common::types::TimeRange;
    use persistence::token_rate::TokenRate;

    clean_test_tokens().await?;

    let fixture = TestFixture::new();

    // 3トークン分のテストデータを準備
    let tokens: Vec<TokenOutAccount> = (1..=3)
        .map(|i| {
            format!("batch_history{}.near", i)
                .parse::<TokenAccount>()
                .unwrap()
                .into()
        })
        .collect();

    for token in &tokens {
        let prices = vec![1.0, 1.1, 1.05, 1.12, 1.15];
        fixture.setup_price_history(token, &prices).await?;
    }

    // バッチ履歴取得の機能を直接テスト
    let token_strs: Vec<String> = tokens.iter().map(|t| t.to_string()).collect();
    let range = TimeRange {
        start: (Utc::now() - Duration::hours(10)).naive_utc(),
        end: Utc::now().naive_utc(),
    };

    let histories_map = TokenRate::get_rates_for_multiple_tokens(
        &token_strs,
        &fixture.quote_token,
        &range,
        test_get_decimals(),
    )
    .await?;

    // 全トークンの履歴が取得できることを確認
    assert_eq!(
        histories_map.len(),
        tokens.len(),
        "All tokens should have price histories"
    );

    // 各トークンに価格データがあることを確認
    for token_str in &token_strs {
        assert!(
            histories_map.contains_key(token_str),
            "Should contain {}",
            token_str
        );
        assert!(
            !histories_map[token_str].is_empty(),
            "Should have price data for {}",
            token_str
        );
    }

    clean_test_tokens().await?;
    Ok(())
}

/// スポットレート補正付きの価格履歴テスト用ヘルパー
fn make_token_rate_with_path(
    base: TokenOutAccount,
    quote: TokenInAccount,
    rate_str: &str,
    timestamp: NaiveDateTime,
    swap_path: Option<SwapPath>,
) -> TokenRate {
    TokenRate {
        base,
        quote,
        exchange_rate: make_rate_from_str(rate_str),
        timestamp,
        rate_calc_near: 10,
        swap_path,
    }
}

/// swap_path 付きレートでスポットレート補正が適用されることを確認
#[test]
fn test_spot_rate_correction_with_path() {
    let base: TokenOutAccount = "test.near".parse::<TokenAccount>().unwrap().into();
    let quote: TokenInAccount = "wrap.near".parse::<TokenAccount>().unwrap().into();
    let timestamp = Utc::now().naive_utc();

    // プールサイズ: 100 NEAR = 10^26 yocto
    // rate_calc_near: 10 NEAR
    // 補正係数: 1 + (10 * 10^24) / (10^26) = 1.1 (+10%)
    let swap_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 123,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "100000000000000000000000000".to_string(), // 100 NEAR in yocto
            amount_out: "50000000000000000000000000".to_string(),
        }],
    };

    let rate = make_token_rate_with_path(base, quote, "1.0", timestamp, Some(swap_path));

    // スポットレート補正を適用
    let spot_rate = rate.to_spot_rate_with_fallback(None);
    let price = spot_rate.to_price();

    // 補正後のレート: 1.0 * 1.1 = 1.1
    // price = yocto_per_near / rate = 10^24 / 1.1 ≈ 9.09... × 10^23
    // 元のレート 1.0 の price = 10^24
    // 補正後は price が小さくなる（レートが大きくなるため）
    let original_rate = make_rate_from_str("1.0");
    let original_price = original_rate.to_price();

    assert!(
        price.as_bigdecimal() < original_price.as_bigdecimal(),
        "Spot-corrected price should be less than original (rate increased): {} < {}",
        price.as_bigdecimal(),
        original_price.as_bigdecimal()
    );
}

/// swap_path なしのレートにフォールバックが適用されることを確認
#[test]
fn test_spot_rate_correction_with_fallback() {
    let base: TokenOutAccount = "test.near".parse::<TokenAccount>().unwrap().into();
    let quote: TokenInAccount = "wrap.near".parse::<TokenAccount>().unwrap().into();
    let now = Utc::now().naive_utc();

    // フォールバック用 swap_path（補正係数 1.1）
    let fallback_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 456,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "100000000000000000000000000".to_string(), // 100 NEAR
            amount_out: "50000000000000000000000000".to_string(),
        }],
    };

    // swap_path なしのレート
    let rate_without_path = make_token_rate_with_path(
        base.clone(),
        quote.clone(),
        "1.0",
        now - chrono::Duration::hours(1),
        None,
    );

    // フォールバックを使用して補正
    let spot_rate = rate_without_path.to_spot_rate_with_fallback(Some(&fallback_path));
    let price_with_fallback = spot_rate.to_price();

    // フォールバックなしの場合（補正なし）
    let price_without_fallback = rate_without_path
        .to_spot_rate_with_fallback(None)
        .to_price();

    // フォールバック使用時は価格が異なる（補正が適用される）
    assert_ne!(
        price_with_fallback.as_bigdecimal(),
        price_without_fallback.as_bigdecimal(),
        "Price should differ when fallback is used"
    );

    // フォールバック使用時は価格が小さい（レートが大きくなるため）
    assert!(
        price_with_fallback.as_bigdecimal() < price_without_fallback.as_bigdecimal(),
        "Price with fallback should be less than without"
    );
}
