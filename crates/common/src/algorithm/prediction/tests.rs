use super::*;
use crate::types::{TokenInAccount, TokenOutAccount, TokenPrice};
use async_trait::async_trait;
use bigdecimal::{BigDecimal, FromPrimitive};
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;

/// テスト用のモックPredictionProvider実装
pub struct MockPredictionProvider {
    pub top_tokens: Vec<TopTokenInfo>,
    pub price_histories: HashMap<TokenOutAccount, PriceHistory>,
    pub predictions: HashMap<TokenOutAccount, TokenPredictionResult>,
}

impl MockPredictionProvider {
    pub fn new() -> Self {
        Self {
            top_tokens: vec![
                TopTokenInfo {
                    token: "token1".parse().unwrap(),
                    volatility: BigDecimal::from_f64(0.2).unwrap(),
                },
                TopTokenInfo {
                    token: "token2".parse().unwrap(),
                    volatility: BigDecimal::from_f64(0.3).unwrap(),
                },
            ],
            price_histories: HashMap::new(),
            predictions: HashMap::new(),
        }
    }

    pub fn with_price_history(mut self, token: &str, prices: Vec<(DateTime<Utc>, f64)>) -> Self {
        let token_out: TokenOutAccount = token.parse().unwrap();
        let quote_token: TokenInAccount = "wrap.near".parse().unwrap();
        let price_points: Vec<PricePoint> = prices
            .into_iter()
            .map(|(timestamp, price)| PricePoint {
                timestamp,
                price: TokenPrice::from_near_per_token(BigDecimal::from_f64(price).unwrap()),
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
impl PredictionProvider for MockPredictionProvider {
    async fn get_tokens_by_volatility(
        &self,
        _start_date: DateTime<Utc>,
        _end_date: DateTime<Utc>,
        _quote_token: &TokenInAccount,
    ) -> crate::Result<Vec<TopTokenInfo>> {
        Ok(self.top_tokens.clone())
    }

    async fn get_price_history(
        &self,
        token: &TokenOutAccount,
        _quote_token: &TokenInAccount,
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

#[cfg(test)]
mod prediction_tests {
    use super::*;
    use crate::algorithm::types::PredictionData;

    fn create_test_timestamp() -> DateTime<Utc> {
        DateTime::from_timestamp(1640995200, 0).unwrap() // 2022-01-01 00:00:00 UTC
    }

    #[tokio::test]
    async fn test_mock_prediction_provider_get_top_tokens() {
        let provider = MockPredictionProvider::new();
        let start_date = create_test_timestamp();
        let end_date = start_date + Duration::days(30);
        let quote_token: TokenInAccount = "wrap.near".parse().unwrap();

        let all_tokens = provider
            .get_tokens_by_volatility(start_date, end_date, &quote_token)
            .await
            .unwrap();
        let top_tokens: Vec<_> = all_tokens.into_iter().take(1).collect();

        let expected_token: TokenOutAccount = "token1".parse().unwrap();
        assert_eq!(top_tokens.len(), 1);
        assert_eq!(top_tokens[0].token, expected_token);
        assert_eq!(top_tokens[0].volatility, BigDecimal::from_f64(0.2).unwrap());
    }

    #[tokio::test]
    async fn test_mock_prediction_provider_with_price_history() {
        let timestamp1 = create_test_timestamp();
        let timestamp2 = timestamp1 + Duration::hours(1);

        let provider = MockPredictionProvider::new()
            .with_price_history("test_token", vec![(timestamp1, 100.0), (timestamp2, 105.0)]);

        let start_date = timestamp1;
        let end_date = timestamp2 + Duration::hours(1);
        let token: TokenOutAccount = "test_token".parse().unwrap();
        let quote_token: TokenInAccount = "wrap.near".parse().unwrap();

        let history = provider
            .get_price_history(&token, &quote_token, start_date, end_date)
            .await
            .unwrap();

        let expected_token: TokenOutAccount = "test_token".parse().unwrap();
        assert_eq!(history.token, expected_token);
        assert_eq!(history.prices.len(), 2);
        assert_eq!(
            history.prices[0].price,
            TokenPrice::from_near_per_token(BigDecimal::from_f64(100.0).unwrap())
        );
        assert_eq!(
            history.prices[1].price,
            TokenPrice::from_near_per_token(BigDecimal::from_f64(105.0).unwrap())
        );
    }

    #[tokio::test]
    async fn test_prediction_data_conversion() {
        let prediction_time = create_test_timestamp();
        let predicted_timestamp = prediction_time + Duration::hours(24);

        // 予測価格を price 形式（NEAR/token）で作成
        let predicted_price_value = BigDecimal::from_f64(110.0).unwrap();
        let token: TokenOutAccount = "test_token".parse().unwrap();
        let quote_token: TokenInAccount = "wrap.near".parse().unwrap();
        let prediction_result = TokenPredictionResult {
            token: token.clone(),
            quote_token,
            prediction_time,
            predictions: vec![PredictedPrice {
                timestamp: predicted_timestamp,
                price: TokenPrice::from_near_per_token(predicted_price_value.clone()),
                confidence: Some("0.85".parse::<BigDecimal>().unwrap()),
            }],
        };

        let current_price = TokenPrice::from_near_per_token(BigDecimal::from(100));
        let prediction_data =
            PredictionData::from_token_prediction(&prediction_result, current_price.clone());

        assert!(prediction_data.is_some());
        let data = prediction_data.unwrap();
        assert_eq!(data.token, token);
        assert_eq!(
            data.current_price.as_bigdecimal(),
            current_price.as_bigdecimal()
        );
        // predicted_price_24h は prediction_result の price をそのまま使用
        assert_eq!(
            data.predicted_price_24h.as_bigdecimal(),
            &predicted_price_value,
            "predicted price should match the input price"
        );
        assert_eq!(data.confidence, Some("0.85".parse::<BigDecimal>().unwrap()));
    }

    #[tokio::test]
    async fn test_predict_multiple_tokens() {
        let timestamp = create_test_timestamp();

        let provider = MockPredictionProvider::new()
            .with_price_history("token1", vec![(timestamp, 100.0)])
            .with_price_history("token2", vec![(timestamp, 50.0)]);

        let token1: TokenOutAccount = "token1".parse().unwrap();
        let token2: TokenOutAccount = "token2".parse().unwrap();
        let tokens = vec![token1.clone(), token2.clone()];
        let quote_token: TokenInAccount = "wrap.near".parse().unwrap();
        let predictions = provider
            .predict_multiple_tokens(tokens, &quote_token, 7, 24)
            .await
            .unwrap();

        assert_eq!(predictions.len(), 2);
        assert!(predictions.contains_key(&token1));
        assert!(predictions.contains_key(&token2));

        let token1_prediction = &predictions[&token1];
        assert_eq!(token1_prediction.token, token1);
        assert_eq!(token1_prediction.predictions.len(), 24);
    }

    #[tokio::test]
    async fn test_prediction_data_conversion_missing_24h_prediction() {
        let prediction_time = create_test_timestamp();
        let predicted_timestamp = prediction_time + Duration::hours(1); // 24時間後ではない

        let token: TokenOutAccount = "test_token".parse().unwrap();
        let quote_token: TokenInAccount = "wrap.near".parse().unwrap();
        let prediction_result = TokenPredictionResult {
            token,
            quote_token,
            prediction_time,
            predictions: vec![PredictedPrice {
                timestamp: predicted_timestamp,
                price: TokenPrice::from_near_per_token(BigDecimal::from_f64(110.0).unwrap()),
                confidence: Some("0.85".parse::<BigDecimal>().unwrap()),
            }],
        };

        let current_price = TokenPrice::from_near_per_token(BigDecimal::from(100));
        let prediction_data =
            PredictionData::from_token_prediction(&prediction_result, current_price);

        // 24時間後の予測が見つからないため、Noneが返される
        assert!(prediction_data.is_none());
    }

    #[tokio::test]
    async fn test_prediction_provider_error_handling() {
        let provider = MockPredictionProvider::new();

        // 存在しないトークンの価格履歴を取得しようとする
        let token: TokenOutAccount = "nonexistent".parse().unwrap();
        let quote_token: TokenInAccount = "wrap.near".parse().unwrap();
        let result = provider
            .get_price_history(&token, &quote_token, Utc::now(), Utc::now())
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
