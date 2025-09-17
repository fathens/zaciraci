use anyhow::{Context, Result};
use async_trait::async_trait;
use bigdecimal::BigDecimal;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zaciraci_common::algorithm::prediction::{
    PredictedPrice as CommonPredictedPrice, PredictionProvider, PriceHistory as CommonPriceHistory,
    PricePoint as CommonPricePoint, TokenPredictionResult, TopTokenInfo,
};
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
    pub price: BigDecimal,
    pub volume: Option<BigDecimal>,
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
    pub price: BigDecimal,
    pub confidence: Option<BigDecimal>,
}

/// トップトークン情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopToken {
    pub token: String,
    pub volatility: BigDecimal,
    pub volume_24h: BigDecimal,
    pub current_price: BigDecimal,
}

/// 価格予測サービス
pub struct PredictionService {
    chronos_client: ChronosApiClient,
    backend_url: String,
}

impl PredictionService {
    pub fn new(chronos_url: String, backend_url: String) -> Self {
        Self {
            chronos_client: ChronosApiClient::new(chronos_url),
            backend_url,
        }
    }

    /// 指定期間のトップトークンを取得
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
                price: BigDecimal::from(price as i64),
                volume: None,
            })
            .collect();

        Ok(TokenPriceHistory {
            token: token.to_string(),
            quote_token: quote_token.to_string(),
            prices: price_points,
        })
    }

    /// 価格予測を実行
    pub async fn predict_price(
        &self,
        history: &TokenPriceHistory,
        prediction_horizon: usize,
    ) -> Result<TokenPrediction> {
        // 履歴データを予測用フォーマットに変換
        // Chronos API は f64 を期待するため、ここでは BigDecimal から f64 に変換
        let values: Vec<f64> = history
            .prices
            .iter()
            .map(|p| p.price.to_string().parse::<f64>().unwrap_or(0.0))
            .collect();
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
            model_name: Some("chronos_default".to_string()),
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

    /// 複数トークンの価格予測を実行（10個ずつのバッチで処理）
    pub async fn predict_multiple_tokens(
        &self,
        tokens: Vec<String>,
        quote_token: &str,
        history_days: i64,
        prediction_horizon: usize,
    ) -> Result<HashMap<String, TokenPrediction>> {
        let end_date = Utc::now();
        let start_date = end_date - Duration::days(history_days);
        let batch_size = 10;

        let mut all_predictions = HashMap::new();

        // トークンを10個ずつのバッチに分割して処理
        for batch in tokens.chunks(batch_size) {
            println!("Processing batch of {} tokens", batch.len());

            // バッチ内の各トークンを順次処理
            // 注: バッチ間では並列化せず、バッチ内のトークンも順次処理する
            // これによりChronosサービスへの同時リクエスト数を制限
            for token in batch {
                println!("Processing token: {}", token);

                // 価格履歴を取得
                let history = self
                    .get_price_history(token, quote_token, start_date, end_date)
                    .await?;

                // 価格予測を実行
                let prediction = self.predict_price(&history, prediction_horizon).await?;

                all_predictions.insert(token.clone(), prediction);
            }
        }

        Ok(all_predictions)
    }

    /// 予測結果を変換
    #[allow(dead_code)]
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
                    price: BigDecimal::from(price as i64), // f64 → BigDecimal 変換
                    confidence: None,                      // 信頼度は将来実装
                }
            })
            .collect();

        Ok(predicted_prices)
    }
}

// PredictionProviderトレイトの実装
#[async_trait]
impl PredictionProvider for PredictionService {
    async fn get_top_tokens(
        &self,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
        limit: usize,
        quote_token: &str,
    ) -> Result<Vec<TopTokenInfo>> {
        let tokens = self
            .get_top_tokens(start_date, end_date, limit, quote_token)
            .await?;
        Ok(tokens
            .into_iter()
            .map(|t| TopTokenInfo {
                token: t.token,
                volatility: t.volatility.to_string().parse::<f64>().unwrap_or(0.0),
                volume_24h: t.volume_24h.to_string().parse::<f64>().unwrap_or(0.0),
                current_price: t.current_price.to_string().parse::<f64>().unwrap_or(0.0),
            })
            .collect())
    }

    async fn get_price_history(
        &self,
        token: &str,
        quote_token: &str,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<CommonPriceHistory> {
        let history = self
            .get_price_history(token, quote_token, start_date, end_date)
            .await?;
        Ok(CommonPriceHistory {
            token: history.token,
            quote_token: history.quote_token,
            prices: history
                .prices
                .into_iter()
                .map(|p| CommonPricePoint {
                    timestamp: p.timestamp,
                    price: p.price.clone(),
                    volume: p.volume.clone(),
                })
                .collect(),
        })
    }

    async fn predict_price(
        &self,
        history: &CommonPriceHistory,
        prediction_horizon: usize,
    ) -> Result<TokenPredictionResult> {
        // CommonPriceHistoryをTokenPriceHistoryに変換
        let backend_history = TokenPriceHistory {
            token: history.token.clone(),
            quote_token: history.quote_token.clone(),
            prices: history
                .prices
                .iter()
                .map(|p| PricePoint {
                    timestamp: p.timestamp,
                    price: p.price.clone(),
                    volume: p.volume.clone(),
                })
                .collect(),
        };

        let prediction = self
            .predict_price(&backend_history, prediction_horizon)
            .await?;

        Ok(TokenPredictionResult {
            token: prediction.token,
            quote_token: prediction.quote_token,
            prediction_time: prediction.prediction_time,
            predictions: prediction
                .predictions
                .into_iter()
                .map(|p| CommonPredictedPrice {
                    timestamp: p.timestamp,
                    price: p.price,
                    confidence: p
                        .confidence
                        .map(|c| c.to_string().parse::<f64>().unwrap_or(0.0)),
                })
                .collect(),
        })
    }

    async fn predict_multiple_tokens(
        &self,
        tokens: Vec<String>,
        quote_token: &str,
        history_days: i64,
        prediction_horizon: usize,
    ) -> Result<HashMap<String, TokenPredictionResult>> {
        let predictions = self
            .predict_multiple_tokens(tokens, quote_token, history_days, prediction_horizon)
            .await?;

        let mut result = HashMap::new();
        for (token, prediction) in predictions {
            result.insert(
                token,
                TokenPredictionResult {
                    token: prediction.token,
                    quote_token: prediction.quote_token,
                    prediction_time: prediction.prediction_time,
                    predictions: prediction
                        .predictions
                        .into_iter()
                        .map(|p| CommonPredictedPrice {
                            timestamp: p.timestamp,
                            price: p.price,
                            confidence: p
                                .confidence
                                .map(|c| c.to_string().parse::<f64>().unwrap_or(0.0)),
                        })
                        .collect(),
                },
            );
        }

        Ok(result)
    }
}

#[cfg(test)]
#[path = "predict/tests.rs"]
mod tests;
