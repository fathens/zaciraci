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
        let last_price_point = history.prices.last();
        let last_price = last_price_point
            .map(|p| p.price.to_string().parse::<f64>().unwrap_or(100.0))
            .unwrap_or(100.0);
        // 実装に合わせて最終データのタイムスタンプを使用
        let data_cutoff_time = last_price_point
            .map(|p| p.timestamp)
            .unwrap_or_else(Utc::now);
        let mut predictions = Vec::new();

        for i in 1..=prediction_horizon {
            let timestamp = data_cutoff_time + Duration::hours(i as i64);
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
            data_cutoff_time,
            predictions,
        })
    }

    async fn predict_multiple_tokens(
        &self,
        tokens: Vec<TokenOutAccount>,
        quote_token: &TokenInAccount,
        history_days: i64,
        prediction_horizon: usize,
        _end_date: DateTime<Utc>,
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
        let data_cutoff_time = create_test_timestamp();
        let predicted_timestamp = data_cutoff_time + Duration::hours(24);

        // 予測価格を price 形式（NEAR/token）で作成
        let predicted_price_value = BigDecimal::from_f64(110.0).unwrap();
        let token: TokenOutAccount = "test_token".parse().unwrap();
        let quote_token: TokenInAccount = "wrap.near".parse().unwrap();
        let prediction_result = TokenPredictionResult {
            token: token.clone(),
            quote_token,
            data_cutoff_time,
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
            .predict_multiple_tokens(tokens, &quote_token, 7, 24, Utc::now())
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
        let data_cutoff_time = create_test_timestamp();
        let predicted_timestamp = data_cutoff_time + Duration::hours(1); // 24時間後ではない

        let token: TokenOutAccount = "test_token".parse().unwrap();
        let quote_token: TokenInAccount = "wrap.near".parse().unwrap();
        let prediction_result = TokenPredictionResult {
            token,
            quote_token,
            data_cutoff_time,
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

    /// data_cutoff_time が現在時刻より大幅に過去でも、24hフィルタが data_cutoff_time 基準で動作すること
    #[tokio::test]
    async fn test_prediction_data_conversion_with_past_data_cutoff_time() {
        // 3日前のデータカットオフ時刻
        let data_cutoff_time = Utc::now() - Duration::days(3);
        let predicted_timestamp = data_cutoff_time + Duration::hours(24);

        let token: TokenOutAccount = "test_token".parse().unwrap();
        let quote_token: TokenInAccount = "wrap.near".parse().unwrap();
        let prediction_result = TokenPredictionResult {
            token: token.clone(),
            quote_token,
            data_cutoff_time,
            predictions: vec![
                PredictedPrice {
                    timestamp: data_cutoff_time + Duration::hours(1),
                    price: TokenPrice::from_near_per_token(BigDecimal::from_f64(101.0).unwrap()),
                    confidence: Some("0.9".parse::<BigDecimal>().unwrap()),
                },
                PredictedPrice {
                    timestamp: predicted_timestamp,
                    price: TokenPrice::from_near_per_token(BigDecimal::from_f64(110.0).unwrap()),
                    confidence: Some("0.85".parse::<BigDecimal>().unwrap()),
                },
            ],
        };

        let current_price = TokenPrice::from_near_per_token(BigDecimal::from(100));
        let prediction_data =
            PredictionData::from_token_prediction(&prediction_result, current_price);

        assert!(
            prediction_data.is_some(),
            "Should find 24h prediction even when data_cutoff_time is in the past"
        );
        let data = prediction_data.unwrap();
        // 1h後の予測ではなく、24h後の予測が選択されること
        assert_eq!(
            data.predicted_price_24h,
            TokenPrice::from_near_per_token(BigDecimal::from_f64(110.0).unwrap()),
            "Should select the 24h prediction, not the 1h prediction"
        );
        assert_eq!(data.timestamp, data_cutoff_time);
    }

    /// MockPredictionProvider が data_cutoff_time に最終データのタイムスタンプを使用すること
    #[tokio::test]
    async fn test_mock_provider_uses_last_data_timestamp_for_cutoff() {
        let past_timestamp = Utc::now() - Duration::hours(6);
        let provider = MockPredictionProvider::new()
            .with_price_history("token1", vec![(past_timestamp, 100.0)]);

        let token: TokenOutAccount = "token1".parse().unwrap();
        let quote_token: TokenInAccount = "wrap.near".parse().unwrap();
        let history = provider
            .get_price_history(&token, &quote_token, Utc::now(), Utc::now())
            .await
            .unwrap();
        let result = provider.predict_price(&history, 24).await.unwrap();

        assert_eq!(
            result.data_cutoff_time, past_timestamp,
            "data_cutoff_time should be the last data timestamp, not Utc::now()"
        );
        // predictions のタイムスタンプが data_cutoff_time 基準であること
        let first_prediction = result.predictions.first().unwrap();
        let expected_first_ts = past_timestamp + Duration::hours(1);
        assert_eq!(
            first_prediction.timestamp, expected_first_ts,
            "First prediction timestamp should be data_cutoff_time + 1h"
        );
    }

    // --- prediction_at_horizon ---

    fn make_prediction_result(
        data_cutoff_time: DateTime<Utc>,
        hour_offsets: &[i64],
    ) -> TokenPredictionResult {
        let token: TokenOutAccount = "test_token".parse().unwrap();
        let quote_token: TokenInAccount = "wrap.near".parse().unwrap();
        let predictions = hour_offsets
            .iter()
            .map(|&h| PredictedPrice {
                timestamp: data_cutoff_time + Duration::hours(h),
                price: TokenPrice::from_near_per_token(
                    BigDecimal::from_f64(100.0 + h as f64).unwrap(),
                ),
                confidence: Some("0.8".parse::<BigDecimal>().unwrap()),
            })
            .collect();
        TokenPredictionResult {
            token,
            quote_token,
            data_cutoff_time,
            predictions,
        }
    }

    #[test]
    fn test_prediction_at_horizon_exact_match() {
        let base = create_test_timestamp();
        let result = make_prediction_result(base, &[1, 2, 12, 24]);
        let p = result.prediction_at_horizon(24).unwrap();
        assert_eq!(p.timestamp, base + Duration::hours(24));
    }

    #[test]
    fn test_prediction_at_horizon_no_match() {
        let base = create_test_timestamp();
        // 1h〜12h しかない → 24h はマッチしない
        let result = make_prediction_result(base, &[1, 2, 12]);
        assert!(result.prediction_at_horizon(24).is_none());
    }

    #[test]
    fn test_prediction_at_horizon_boundary_23h() {
        let base = create_test_timestamp();
        // 23h のポイントのみ → 24h ±1h の範囲内なのでマッチする
        let result = make_prediction_result(base, &[23]);
        let p = result.prediction_at_horizon(24).unwrap();
        assert_eq!(p.timestamp, base + Duration::hours(23));
    }

    #[test]
    fn test_prediction_at_horizon_boundary_25h() {
        let base = create_test_timestamp();
        // 25h のポイントのみ → 24h ±1h の範囲内なのでマッチする
        let result = make_prediction_result(base, &[25]);
        let p = result.prediction_at_horizon(24).unwrap();
        assert_eq!(p.timestamp, base + Duration::hours(25));
    }

    #[test]
    fn test_prediction_at_horizon_outside_tolerance() {
        let base = create_test_timestamp();
        // 22h と 26h → どちらも 24h ±1h の範囲外
        let result = make_prediction_result(base, &[22, 26]);
        assert!(result.prediction_at_horizon(24).is_none());
    }

    #[test]
    fn test_prediction_at_horizon_zero() {
        let base = create_test_timestamp();
        let result = make_prediction_result(base, &[1, 24]);
        assert!(
            result.prediction_at_horizon(0).is_none(),
            "horizon_hours = 0 should return None"
        );
    }

    #[test]
    fn test_prediction_at_horizon_selects_closest() {
        let base = create_test_timestamp();
        // 23h と 24h の両方がある → 24h（目標に最も近い）が選ばれるべき
        let result = make_prediction_result(base, &[23, 24]);
        let p = result.prediction_at_horizon(24).unwrap();
        assert_eq!(
            p.timestamp,
            base + Duration::hours(24),
            "Should select the closest point to target (24h), not the first match (23h)"
        );
    }

    #[test]
    fn test_prediction_at_horizon_selects_closest_from_both_sides() {
        let base = create_test_timestamp();
        // 23h と 25h → どちらも距離1hで同じ。min_by_key は最初のものを返す = 23h
        let result = make_prediction_result(base, &[23, 25]);
        let p = result.prediction_at_horizon(24).unwrap();
        // 両方同距離なので最初のマッチ（23h）を返す
        assert!(
            p.timestamp == base + Duration::hours(23) || p.timestamp == base + Duration::hours(25),
            "Should return one of the equidistant points"
        );
    }

    #[test]
    fn test_prediction_at_horizon_empty_predictions() {
        let base = create_test_timestamp();
        let result = make_prediction_result(base, &[]);
        assert!(
            result.prediction_at_horizon(24).is_none(),
            "Empty predictions should return None"
        );
    }

    #[test]
    fn test_prediction_data_conversion_empty_predictions() {
        let base = create_test_timestamp();
        let result = make_prediction_result(base, &[]);
        let current_price = TokenPrice::from_near_per_token(BigDecimal::from(100));
        let prediction_data = PredictionData::from_token_prediction(&result, current_price);
        assert!(
            prediction_data.is_none(),
            "Empty predictions should make from_token_prediction return None"
        );
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
