use crate::Result;
use crate::types::ExchangeRate;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

// Re-export shared types from types.rs
pub use super::types::{
    PredictedPrice, PriceHistory, PricePoint, TokenPredictionResult, TopTokenInfo,
};

/// 予測サービスのトレイト
#[async_trait]
pub trait PredictionProvider: Send + Sync {
    /// ボラティリティ順に全トークンを取得
    async fn get_tokens_by_volatility(
        &self,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
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
    ///
    /// `current_rate` の decimals を予測レートにも適用する。
    pub fn from_token_prediction(
        prediction: &TokenPredictionResult,
        current_rate: ExchangeRate,
    ) -> Option<Self> {
        use chrono::Duration;

        // 24時間後の予測価格を取得
        let predicted_24h = prediction.predictions.iter().find(|p| {
            let diff = p.timestamp - prediction.prediction_time;
            diff >= Duration::hours(23) && diff <= Duration::hours(25)
        })?;

        // 予測レートに同じ decimals を適用
        let predicted_rate_24h = ExchangeRate::new(
            predicted_24h.price.as_bigdecimal().clone(),
            current_rate.decimals(),
        );

        Some(Self {
            token: prediction.token.clone(),
            current_rate,
            predicted_rate_24h,
            timestamp: prediction.prediction_time,
            confidence: predicted_24h.confidence.clone(),
        })
    }
}

#[cfg(test)]
mod tests;
