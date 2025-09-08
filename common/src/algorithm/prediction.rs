use crate::Result;
use async_trait::async_trait;
use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// トークンの価格履歴
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceHistory {
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
pub struct TokenPredictionResult {
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
pub struct TopTokenInfo {
    pub token: String,
    pub volatility: f64,
    pub volume_24h: f64,
    pub current_price: f64,
}

/// 予測サービスのトレイト
#[async_trait]
pub trait PredictionProvider: Send + Sync {
    /// 指定期間のトップトークンを取得
    async fn get_top_tokens(
        &self,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
        limit: usize,
        quote_token: &str,
    ) -> Result<Vec<TopTokenInfo>>;

    /// 指定トークンの価格履歴を取得
    async fn get_price_history(
        &self,
        token: &str,
        quote_token: &str,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<PriceHistory>;

    /// 価格予測を実行
    async fn predict_price(
        &self,
        history: &PriceHistory,
        prediction_horizon: usize,
    ) -> Result<TokenPredictionResult>;

    /// 複数トークンの価格予測を実行
    async fn predict_multiple_tokens(
        &self,
        tokens: Vec<String>,
        quote_token: &str,
        history_days: i64,
        prediction_horizon: usize,
    ) -> Result<HashMap<String, TokenPredictionResult>>;
}

/// PredictionDataへの変換（momentum.rsから移動）
impl crate::algorithm::PredictionData {
    /// TokenPredictionResultから変換
    pub fn from_token_prediction(
        prediction: &TokenPredictionResult,
        current_price: BigDecimal,
    ) -> Option<Self> {
        use chrono::Duration;

        // 24時間後の予測価格を取得
        let predicted_24h = prediction.predictions.iter().find(|p| {
            let diff = p.timestamp - prediction.prediction_time;
            diff >= Duration::hours(23) && diff <= Duration::hours(25)
        })?;

        Some(Self {
            token: prediction.token.clone(),
            current_price,
            predicted_price_24h: BigDecimal::from(predicted_24h.price as i64),
            timestamp: prediction.prediction_time,
            confidence: predicted_24h.confidence,
        })
    }
}

#[cfg(test)]
mod tests;
