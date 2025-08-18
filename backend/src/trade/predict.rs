use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zaciraci_common::api::chronos::ChronosApiClient;
use zaciraci_common::api::traits::PredictionClient;
use zaciraci_common::prediction::{PredictionResult, ZeroShotPredictionRequest};

/// トークンの価格履歴
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPriceHistory {
    pub token: String,
    pub quote_token: String,
    pub prices: Vec<PricePoint>,
}

/// 価格ポイント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricePoint {
    pub timestamp: DateTime<Utc>,
    pub price: f64,
}

/// 予測結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPrediction {
    pub token: String,
    pub quote_token: String,
    pub prediction_time: DateTime<Utc>,
    pub predictions: Vec<PredictedPrice>,
}

/// 予測価格
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictedPrice {
    pub timestamp: DateTime<Utc>,
    pub price: f64,
    pub confidence: Option<f64>,
}

/// トップトークン情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopToken {
    pub token: String,
    pub volatility: f64,
    pub volume_24h: f64,
    pub current_price: f64,
}

/// 価格予測サービス
pub struct PredictionService {
    chronos_client: ChronosApiClient,
    backend_url: String,
}

impl PredictionService {
    #[allow(dead_code)]
    pub fn new(chronos_url: String, backend_url: String) -> Self {
        Self {
            chronos_client: ChronosApiClient::new(chronos_url),
            backend_url,
        }
    }

    /// 指定期間のトップトークンを取得
    #[allow(dead_code)]
    pub async fn get_top_tokens(
        &self,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
        limit: usize,
        quote_token: &str,
    ) -> Result<Vec<TopToken>> {
        // Backend APIを使用してトップトークンを取得
        let client = reqwest::Client::new();
        let url = format!("{}/api/volatility_tokens", self.backend_url);

        let params = [
            ("start_date", start_date.format("%Y-%m-%d").to_string()),
            ("end_date", end_date.format("%Y-%m-%d").to_string()),
            ("limit", limit.to_string()),
            ("quote_token", quote_token.to_string()),
        ];

        let response = client
            .get(&url)
            .query(&params)
            .send()
            .await
            .context("Failed to fetch top tokens")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to fetch top tokens: HTTP {}",
                response.status()
            ));
        }

        // APIレスポンスをパース
        let tokens: Vec<TopToken> = response
            .json()
            .await
            .context("Failed to parse top tokens response")?;

        Ok(tokens)
    }

    /// 指定トークンの価格履歴を取得
    #[allow(dead_code)]
    pub async fn get_price_history(
        &self,
        token: &str,
        quote_token: &str,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<TokenPriceHistory> {
        let client = reqwest::Client::new();
        let url = format!(
            "{}/api/price_history/{}/{}",
            self.backend_url, quote_token, token
        );

        let params = [
            ("start", start_date.timestamp().to_string()),
            ("end", end_date.timestamp().to_string()),
        ];

        let response = client
            .get(&url)
            .query(&params)
            .send()
            .await
            .context("Failed to fetch price history")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to fetch price history: HTTP {}",
                response.status()
            ));
        }

        // APIレスポンスをパース
        let prices: Vec<(i64, f64)> = response
            .json()
            .await
            .context("Failed to parse price history response")?;

        // タイムスタンプを DateTime に変換
        let price_points: Vec<PricePoint> = prices
            .into_iter()
            .map(|(timestamp, price)| PricePoint {
                timestamp: DateTime::from_timestamp(timestamp, 0).unwrap_or_else(Utc::now),
                price,
            })
            .collect();

        Ok(TokenPriceHistory {
            token: token.to_string(),
            quote_token: quote_token.to_string(),
            prices: price_points,
        })
    }

    /// 価格予測を実行
    #[allow(dead_code)]
    pub async fn predict_price(
        &self,
        history: &TokenPriceHistory,
        prediction_horizon: usize,
    ) -> Result<TokenPrediction> {
        // 履歴データを予測用フォーマットに変換
        let values: Vec<f64> = history.prices.iter().map(|p| p.price).collect();
        let timestamps: Vec<DateTime<Utc>> = history.prices.iter().map(|p| p.timestamp).collect();

        if values.is_empty() {
            return Err(anyhow::anyhow!("No price history available for prediction"));
        }

        let last_timestamp = timestamps.last().unwrap();
        let forecast_until = *last_timestamp + Duration::hours(prediction_horizon as i64);

        // 予測リクエストを作成
        let request = ZeroShotPredictionRequest {
            timestamp: timestamps,
            values,
            forecast_until,
            model_name: Some("chronos-t5-large".to_string()),
            model_params: None,
        };

        // 非同期予測を開始
        let async_response = self
            .chronos_client
            .predict(request)
            .await
            .context("Failed to start prediction")?;

        println!(
            "Prediction started with task ID: {}",
            async_response.task_id
        );

        // 予測完了まで待機
        let result = self
            .chronos_client
            .poll_prediction_until_complete(&async_response.task_id)
            .await
            .context("Failed to get prediction result")?;

        // 予測結果を変換
        let predictions = self.convert_prediction_result(
            &result,
            &history.prices.last().unwrap().timestamp,
            prediction_horizon,
        )?;

        Ok(TokenPrediction {
            token: history.token.clone(),
            quote_token: history.quote_token.clone(),
            prediction_time: Utc::now(),
            predictions,
        })
    }

    /// 複数トークンの価格予測を並列実行
    #[allow(dead_code)]
    pub async fn predict_multiple_tokens(
        &self,
        tokens: Vec<String>,
        quote_token: &str,
        history_days: i64,
        prediction_horizon: usize,
    ) -> Result<HashMap<String, TokenPrediction>> {
        let end_date = Utc::now();
        let start_date = end_date - Duration::days(history_days);

        let mut predictions = HashMap::new();

        // 各トークンの履歴を取得して予測
        for token in tokens {
            println!("Processing token: {}", token);

            // 価格履歴を取得
            let history = self
                .get_price_history(&token, quote_token, start_date, end_date)
                .await?;

            // 価格予測を実行
            let prediction = self.predict_price(&history, prediction_horizon).await?;

            predictions.insert(token, prediction);
        }

        Ok(predictions)
    }

    /// 予測結果を変換
    fn convert_prediction_result(
        &self,
        result: &PredictionResult,
        last_timestamp: &DateTime<Utc>,
        horizon: usize,
    ) -> Result<Vec<PredictedPrice>> {
        let chronos_response = result.result.as_ref().context("No prediction result")?;

        let predicted_prices: Vec<PredictedPrice> = chronos_response
            .forecast_values
            .iter()
            .take(horizon)
            .enumerate()
            .map(|(i, &price)| {
                let timestamp = *last_timestamp + Duration::hours((i + 1) as i64);
                PredictedPrice {
                    timestamp,
                    price,
                    confidence: None, // 信頼度は将来実装
                }
            })
            .collect();

        Ok(predicted_prices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{self, Matcher};
    use zaciraci_common::prediction::ChronosPredictionResponse;

    // モックレスポンスのヘルパー関数
    fn create_mock_top_tokens() -> Vec<TopToken> {
        vec![
            TopToken {
                token: "token1.near".to_string(),
                volatility: 0.25,
                volume_24h: 1000000.0,
                current_price: 1.5,
            },
            TopToken {
                token: "token2.near".to_string(),
                volatility: 0.20,
                volume_24h: 500000.0,
                current_price: 2.0,
            },
        ]
    }

    fn create_mock_price_history() -> Vec<(i64, f64)> {
        let now = Utc::now().timestamp();
        vec![
            (now - 3600, 1.0),
            (now - 2400, 1.1),
            (now - 1200, 1.05),
            (now, 1.15),
        ]
    }

    #[tokio::test]
    async fn test_get_top_tokens() {
        let mut server = mockito::Server::new_async().await;
        let url = server.url();

        let expected_tokens = create_mock_top_tokens();
        let _m = server
            .mock("GET", "/api/volatility_tokens")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("limit".into(), "10".into()),
                Matcher::UrlEncoded("quote_token".into(), "wrap.near".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&expected_tokens).unwrap())
            .create_async()
            .await;

        let service = PredictionService::new("http://localhost:8000".to_string(), url.clone());

        let start_date = Utc::now() - Duration::days(7);
        let end_date = Utc::now();

        let result = service
            .get_top_tokens(start_date, end_date, 10, "wrap.near")
            .await;

        assert!(result.is_ok());
        let tokens = result.unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].token, "token1.near");
        assert_eq!(tokens[1].token, "token2.near");
    }

    #[tokio::test]
    async fn test_get_price_history() {
        let mut server = mockito::Server::new_async().await;
        let url = server.url();

        let mock_prices = create_mock_price_history();
        let _m = server
            .mock("GET", "/api/price_history/wrap.near/test.near")
            .match_query(Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&mock_prices).unwrap())
            .create_async()
            .await;

        let service = PredictionService::new("http://localhost:8000".to_string(), url.clone());

        let start_date = Utc::now() - Duration::hours(2);
        let end_date = Utc::now();

        let result = service
            .get_price_history("test.near", "wrap.near", start_date, end_date)
            .await;

        // デバッグ用にエラーを出力
        if let Err(ref e) = result {
            eprintln!("Error in test_get_price_history: {:?}", e);
        }

        assert!(result.is_ok());
        let history = result.unwrap();
        assert_eq!(history.token, "test.near");
        assert_eq!(history.quote_token, "wrap.near");
        assert_eq!(history.prices.len(), 4);
        assert_eq!(history.prices[0].price, 1.0);
        assert_eq!(history.prices[3].price, 1.15);
    }

    #[tokio::test]
    async fn test_convert_prediction_result() {
        let service = PredictionService::new(
            "http://localhost:8000".to_string(),
            "http://localhost:3000".to_string(),
        );

        let result = PredictionResult {
            task_id: "test-id".to_string(),
            status: "completed".to_string(),
            progress: Some(1.0),
            message: None,
            result: Some(ChronosPredictionResponse {
                forecast_timestamp: vec![],
                forecast_values: vec![1.2, 1.3, 1.4, 1.5],
                model_name: "chronos-t5-large".to_string(),
                confidence_intervals: None,
                metrics: None,
            }),
            error: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let last_timestamp = Utc::now();
        let predictions = service.convert_prediction_result(&result, &last_timestamp, 3);

        assert!(predictions.is_ok());
        let preds = predictions.unwrap();
        assert_eq!(preds.len(), 3);
        assert_eq!(preds[0].price, 1.2);
        assert_eq!(preds[1].price, 1.3);
        assert_eq!(preds[2].price, 1.4);

        // タイムスタンプが1時間ずつ増加していることを確認
        assert_eq!(preds[1].timestamp - preds[0].timestamp, Duration::hours(1));
    }

    #[tokio::test]
    async fn test_get_top_tokens_error_handling() {
        let mut server = mockito::Server::new_async().await;
        let url = server.url();

        let _m = server
            .mock("GET", "/api/volatility_tokens")
            .with_status(500)
            .with_body("Internal Server Error")
            .create_async()
            .await;

        let service = PredictionService::new("http://localhost:8000".to_string(), url.clone());

        let start_date = Utc::now() - Duration::days(7);
        let end_date = Utc::now();

        let result = service
            .get_top_tokens(start_date, end_date, 10, "wrap.near")
            .await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("Failed to fetch top tokens"));
    }

    #[tokio::test]
    async fn test_empty_price_history() {
        let service = PredictionService::new(
            "http://localhost:8000".to_string(),
            "http://localhost:3000".to_string(),
        );

        let history = TokenPriceHistory {
            token: "test.near".to_string(),
            quote_token: "wrap.near".to_string(),
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

    // データ構造のシリアライゼーションテスト
    #[test]
    fn test_token_prediction_serialization() {
        let prediction = TokenPrediction {
            token: "test.near".to_string(),
            quote_token: "wrap.near".to_string(),
            prediction_time: Utc::now(),
            predictions: vec![PredictedPrice {
                timestamp: Utc::now(),
                price: 1.5,
                confidence: Some(0.85),
            }],
        };

        let json = serde_json::to_string(&prediction);
        assert!(json.is_ok());

        let deserialized: Result<TokenPrediction, _> = serde_json::from_str(&json.unwrap());
        assert!(deserialized.is_ok());
        assert_eq!(deserialized.unwrap().token, "test.near");
    }
}
