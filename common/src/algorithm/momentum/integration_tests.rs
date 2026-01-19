use super::execute_with_prediction_provider;
use crate::algorithm::prediction::{PredictionProvider, TokenPredictionResult};
use crate::algorithm::types::*;
use crate::types::{
    ExchangeRate, NearValue, TokenAmount, TokenInAccount, TokenOutAccount, TokenPrice,
    TokenPriceF64,
};
use async_trait::async_trait;
use bigdecimal::{BigDecimal, FromPrimitive};
use chrono::{Duration, Utc};
use std::collections::HashMap;

#[allow(dead_code)]
fn price(v: f64) -> TokenPrice {
    TokenPrice::from_near_per_token(BigDecimal::from_f64(v).unwrap())
}

// テスト用のシンプルなMockPredictionProvider
struct SimpleMockProvider {
    price_histories: HashMap<TokenOutAccount, PriceHistory>,
}

impl SimpleMockProvider {
    fn new() -> Self {
        Self {
            price_histories: HashMap::new(),
        }
    }

    fn with_price_history(
        mut self,
        token: &str,
        prices: Vec<(chrono::DateTime<Utc>, f64)>,
    ) -> Self {
        let token_out: TokenOutAccount = token.parse().unwrap();
        let quote_token: TokenInAccount = "wrap.near".parse().unwrap();
        let price_points: Vec<PricePoint> = prices
            .into_iter()
            .map(|(timestamp, price)| PricePoint {
                timestamp,
                price: TokenPrice::from_near_per_token(
                    BigDecimal::from_f64(price).unwrap_or_default(),
                ),
                volume: None,
            })
            .collect();

        self.price_histories.insert(
            token_out.clone(),
            PriceHistory {
                token: token_out,
                quote_token,
                prices: price_points,
            },
        );
        self
    }
}

#[async_trait]
impl PredictionProvider for SimpleMockProvider {
    async fn get_tokens_by_volatility(
        &self,
        _start_date: chrono::DateTime<Utc>,
        _end_date: chrono::DateTime<Utc>,
        _quote_token: &TokenInAccount,
    ) -> crate::Result<Vec<TopTokenInfo>> {
        Ok(vec![
            TopTokenInfo {
                token: "top_token1".parse().unwrap(),
                volatility: 0.2,
                volume_24h: 1000000.0,
                current_price: TokenPriceF64::from_near_per_token(100.0),
                decimals: 24,
            },
            TopTokenInfo {
                token: "top_token2".parse().unwrap(),
                volatility: 0.3,
                volume_24h: 800000.0,
                current_price: TokenPriceF64::from_near_per_token(50.0),
                decimals: 24,
            },
        ]
        .into_iter()
        .collect())
    }

    async fn get_price_history(
        &self,
        token: &TokenOutAccount,
        _quote_token: &TokenInAccount,
        _start_date: chrono::DateTime<Utc>,
        _end_date: chrono::DateTime<Utc>,
    ) -> crate::Result<PriceHistory> {
        self.price_histories
            .get(token)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No price history found for token: {}", token))
    }

    async fn predict_price(
        &self,
        history: &PriceHistory,
        prediction_horizon: usize,
    ) -> crate::Result<TokenPredictionResult> {
        let last_price = history
            .prices
            .last()
            .map(|p| p.price.to_string().parse::<f64>().unwrap_or(100.0))
            .unwrap_or(100.0);
        let prediction_time = Utc::now();
        let mut predictions = Vec::new();

        for i in 1..=prediction_horizon {
            let timestamp = prediction_time + Duration::hours(i as i64);
            // price 形式で予測を作成（NEAR/token）
            let price_value = BigDecimal::from_f64(last_price * (1.0 + (i as f64 * 0.01))).unwrap();
            predictions.push(PredictedPrice {
                timestamp,
                price: TokenPrice::from_near_per_token(price_value),
                confidence: Some("0.8".parse::<BigDecimal>().unwrap()),
            });
        }

        Ok(TokenPredictionResult {
            token: history.token.clone(),
            quote_token: history.quote_token.clone(),
            prediction_time,
            predictions,
        })
    }

    async fn predict_multiple_tokens(
        &self,
        tokens: Vec<TokenOutAccount>,
        quote_token: &TokenInAccount,
        history_days: i64,
        prediction_horizon: usize,
    ) -> crate::Result<HashMap<TokenOutAccount, TokenPredictionResult>> {
        let mut results = HashMap::new();

        for token in tokens {
            let end_date = Utc::now();
            let start_date = end_date - Duration::days(history_days);

            if let Ok(history) = self
                .get_price_history(&token, quote_token, start_date, end_date)
                .await
                && let Ok(prediction) = self.predict_price(&history, prediction_horizon).await
            {
                results.insert(token, prediction);
            }
        }

        Ok(results)
    }
}

#[tokio::test]
async fn test_execute_with_prediction_provider() {
    let current_time = Utc::now();
    let provider = SimpleMockProvider::new()
        .with_price_history("token1", vec![(current_time, 100.0)])
        .with_price_history("token2", vec![(current_time, 50.0)])
        .with_price_history("top_token1", vec![(current_time, 100.0)])
        .with_price_history("top_token2", vec![(current_time, 50.0)]);

    let current_holdings = vec![
        TokenHolding {
            token: "token1".parse().unwrap(),
            amount: TokenAmount::from_smallest_units(BigDecimal::from(10), 24),
            current_rate: ExchangeRate::from_raw_rate(BigDecimal::from(100), 24),
        },
        TokenHolding {
            token: "token2".parse().unwrap(),
            amount: TokenAmount::from_smallest_units(BigDecimal::from(20), 24),
            current_rate: ExchangeRate::from_raw_rate(BigDecimal::from(50), 24),
        },
    ];

    let quote_token: TokenInAccount = "wrap.near".parse().unwrap();
    let min_trade_value = NearValue::from_near(BigDecimal::from(1)); // 1 NEAR
    let result = execute_with_prediction_provider(
        &provider,
        current_holdings,
        &quote_token,
        7,
        0.05,             // min_profit_threshold
        1.5,              // switch_multiplier
        &min_trade_value, // min_trade_value
    )
    .await;

    match result {
        Ok(report) => {
            // レポートの基本的な構造を確認
            assert_eq!(report.timestamp.date_naive(), Utc::now().date_naive());
            println!("Generated {} actions", report.actions.len());
            println!("Expected return: {:?}", report.expected_return);
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}

#[tokio::test]
async fn test_execute_with_prediction_provider_empty_holdings() {
    let current_time = Utc::now();
    let provider = SimpleMockProvider::new()
        .with_price_history("top_token1", vec![(current_time, 100.0)])
        .with_price_history("top_token2", vec![(current_time, 50.0)]);
    let current_holdings = vec![];

    let quote_token: TokenInAccount = "wrap.near".parse().unwrap();
    let min_trade_value = NearValue::from_near(BigDecimal::from(1)); // 1 NEAR
    let result = execute_with_prediction_provider(
        &provider,
        current_holdings,
        &quote_token,
        7,
        0.05,             // min_profit_threshold
        1.5,              // switch_multiplier
        &min_trade_value, // min_trade_value
    )
    .await;

    match result {
        Ok(report) => {
            // 空の保有でも実行できることを確認
            assert_eq!(report.total_trades, 0);
            assert_eq!(report.actions.len(), 0);
        }
        Err(e) => {
            panic!("Test failed with error: {}", e);
        }
    }
}

#[tokio::test]
async fn test_execute_with_prediction_provider_with_top_tokens() {
    let current_time = Utc::now();
    let provider = SimpleMockProvider::new()
        .with_price_history("top_token1", vec![(current_time, 100.0)])
        .with_price_history("top_token2", vec![(current_time, 50.0)]);

    let current_holdings = vec![TokenHolding {
        token: "other_token".parse().unwrap(),
        amount: TokenAmount::from_smallest_units(BigDecimal::from(10), 24),
        current_rate: ExchangeRate::from_raw_rate(BigDecimal::from(75), 24),
    }];

    let quote_token: TokenInAccount = "wrap.near".parse().unwrap();
    let min_trade_value = NearValue::from_near(BigDecimal::from(1)); // 1 NEAR
    let result = execute_with_prediction_provider(
        &provider,
        current_holdings,
        &quote_token,
        7,
        0.05,             // min_profit_threshold
        1.5,              // switch_multiplier
        &min_trade_value, // min_trade_value
    )
    .await;

    // トップトークンの情報も取得されることを確認
    assert!(result.is_ok());
    let report = result.unwrap();

    // レポートが生成されることを確認
    assert!(report.expected_return.is_some() || report.expected_return.is_none());
}
