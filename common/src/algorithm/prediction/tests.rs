use super::*;
use async_trait::async_trait;
use bigdecimal::BigDecimal;
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;

/// テスト用のモックPredictionProvider実装
pub struct MockPredictionProvider {
    pub top_tokens: Vec<TopTokenInfo>,
    pub price_histories: HashMap<String, PriceHistory>,
    pub predictions: HashMap<String, TokenPredictionResult>,
}

impl MockPredictionProvider {
    pub fn new() -> Self {
        Self {
            top_tokens: vec![
                TopTokenInfo {
                    token: "token1".to_string(),
                    volatility: 0.2,
                    volume_24h: 1000000.0,
                    current_price: 100.0,
                },
                TopTokenInfo {
                    token: "token2".to_string(),
                    volatility: 0.3,
                    volume_24h: 800000.0,
                    current_price: 50.0,
                },
            ],
            price_histories: HashMap::new(),
            predictions: HashMap::new(),
        }
    }

    pub fn with_price_history(mut self, token: &str, prices: Vec<(DateTime<Utc>, f64)>) -> Self {
        let price_points: Vec<PricePoint> = prices
            .into_iter()
            .map(|(timestamp, price)| PricePoint { timestamp, price })
            .collect();

        self.price_histories.insert(
            token.to_string(),
            PriceHistory {
                token: token.to_string(),
                quote_token: "wrap.near".to_string(),
                prices: price_points,
            },
        );
        self
    }
}

#[async_trait]
impl PredictionProvider for MockPredictionProvider {
    async fn get_top_tokens(
        &self,
        _start_date: DateTime<Utc>,
        _end_date: DateTime<Utc>,
        limit: usize,
        _quote_token: &str,
    ) -> crate::Result<Vec<TopTokenInfo>> {
        Ok(self.top_tokens.clone().into_iter().take(limit).collect())
    }

    async fn get_price_history(
        &self,
        token: &str,
        _quote_token: &str,
        _start_date: DateTime<Utc>,
        _end_date: DateTime<Utc>,
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
        if let Some(existing_prediction) = self.predictions.get(&history.token) {
            return Ok(existing_prediction.clone());
        }

        // デフォルトの予測を生成
        let last_price = history.prices.last().map(|p| p.price).unwrap_or(100.0);
        let prediction_time = Utc::now();
        let mut predictions = Vec::new();

        for i in 1..=prediction_horizon {
            let timestamp = prediction_time + Duration::hours(i as i64);
            let price = last_price * (1.0 + (i as f64 * 0.01)); // 1%ずつ増加する予測
            predictions.push(PredictedPrice {
                timestamp,
                price,
                confidence: Some(0.8),
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
        tokens: Vec<String>,
        quote_token: &str,
        history_days: i64,
        prediction_horizon: usize,
    ) -> crate::Result<HashMap<String, TokenPredictionResult>> {
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

#[cfg(test)]
mod prediction_tests {
    use super::*;
    use crate::algorithm::momentum::PredictionData;

    fn create_test_timestamp() -> DateTime<Utc> {
        DateTime::from_timestamp(1640995200, 0).unwrap() // 2022-01-01 00:00:00 UTC
    }

    #[tokio::test]
    async fn test_mock_prediction_provider_get_top_tokens() {
        let provider = MockPredictionProvider::new();
        let start_date = create_test_timestamp();
        let end_date = start_date + Duration::days(30);

        let top_tokens = provider
            .get_top_tokens(start_date, end_date, 1, "wrap.near")
            .await
            .unwrap();

        assert_eq!(top_tokens.len(), 1);
        assert_eq!(top_tokens[0].token, "token1");
        assert_eq!(top_tokens[0].current_price, 100.0);
    }

    #[tokio::test]
    async fn test_mock_prediction_provider_with_price_history() {
        let timestamp1 = create_test_timestamp();
        let timestamp2 = timestamp1 + Duration::hours(1);

        let provider = MockPredictionProvider::new()
            .with_price_history("test_token", vec![(timestamp1, 100.0), (timestamp2, 105.0)]);

        let start_date = timestamp1;
        let end_date = timestamp2 + Duration::hours(1);

        let history = provider
            .get_price_history("test_token", "wrap.near", start_date, end_date)
            .await
            .unwrap();

        assert_eq!(history.token, "test_token");
        assert_eq!(history.prices.len(), 2);
        assert_eq!(history.prices[0].price, 100.0);
        assert_eq!(history.prices[1].price, 105.0);
    }

    #[tokio::test]
    async fn test_prediction_data_conversion() {
        let prediction_time = create_test_timestamp();
        let predicted_timestamp = prediction_time + Duration::hours(24);

        let prediction_result = TokenPredictionResult {
            token: "test_token".to_string(),
            quote_token: "wrap.near".to_string(),
            prediction_time,
            predictions: vec![PredictedPrice {
                timestamp: predicted_timestamp,
                price: 110.0,
                confidence: Some(0.85),
            }],
        };

        let current_price = BigDecimal::from(100);
        let prediction_data =
            PredictionData::from_token_prediction(&prediction_result, current_price.clone());

        assert!(prediction_data.is_some());
        let data = prediction_data.unwrap();
        assert_eq!(data.token, "test_token");
        assert_eq!(data.current_price, current_price);
        assert_eq!(data.predicted_price_24h, BigDecimal::from(110));
        assert_eq!(data.confidence, Some(0.85));
    }

    #[tokio::test]
    async fn test_predict_multiple_tokens() {
        let timestamp = create_test_timestamp();

        let provider = MockPredictionProvider::new()
            .with_price_history("token1", vec![(timestamp, 100.0)])
            .with_price_history("token2", vec![(timestamp, 50.0)]);

        let tokens = vec!["token1".to_string(), "token2".to_string()];
        let predictions = provider
            .predict_multiple_tokens(tokens, "wrap.near", 7, 24)
            .await
            .unwrap();

        assert_eq!(predictions.len(), 2);
        assert!(predictions.contains_key("token1"));
        assert!(predictions.contains_key("token2"));

        let token1_prediction = &predictions["token1"];
        assert_eq!(token1_prediction.token, "token1");
        assert_eq!(token1_prediction.predictions.len(), 24);
    }

    #[tokio::test]
    async fn test_prediction_data_conversion_missing_24h_prediction() {
        let prediction_time = create_test_timestamp();
        let predicted_timestamp = prediction_time + Duration::hours(1); // 24時間後ではない

        let prediction_result = TokenPredictionResult {
            token: "test_token".to_string(),
            quote_token: "wrap.near".to_string(),
            prediction_time,
            predictions: vec![PredictedPrice {
                timestamp: predicted_timestamp,
                price: 110.0,
                confidence: Some(0.85),
            }],
        };

        let current_price = BigDecimal::from(100);
        let prediction_data =
            PredictionData::from_token_prediction(&prediction_result, current_price);

        // 24時間後の予測が見つからないため、Noneが返される
        assert!(prediction_data.is_none());
    }

    #[tokio::test]
    async fn test_prediction_provider_error_handling() {
        let provider = MockPredictionProvider::new();

        // 存在しないトークンの価格履歴を取得しようとする
        let result = provider
            .get_price_history("nonexistent", "wrap.near", Utc::now(), Utc::now())
            .await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No price history found")
        );
    }
}
