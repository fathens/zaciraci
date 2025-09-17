use crate::Result;
use async_trait::async_trait;
use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

// Re-export shared types from types.rs
pub use super::types::{
    PredictedPrice, PriceHistory, PricePoint, TokenPredictionResult, TopTokenInfo,
};

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
            predicted_price_24h: predicted_24h.price.clone(),
            timestamp: prediction.prediction_time,
            confidence: predicted_24h.confidence.clone(),
        })
    }
}

#[cfg(test)]
mod tests;
