use crate::Result;
use crate::types::{TokenInAccount, TokenOutAccount, TokenPrice};
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
        quote_token: &TokenInAccount,
    ) -> Result<Vec<TopTokenInfo>>;

    /// 指定トークンの価格履歴を取得
    async fn get_price_history(
        &self,
        token: &TokenOutAccount,
        quote_token: &TokenInAccount,
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
        tokens: Vec<TokenOutAccount>,
        quote_token: &TokenInAccount,
        history_days: i64,
        prediction_horizon: usize,
        end_date: DateTime<Utc>,
    ) -> Result<HashMap<TokenOutAccount, TokenPredictionResult>>;
}

/// 予測の時間軸（何時間先の価格を予測するか）
pub const PREDICTION_HORIZON_HOURS: usize = 24;

impl TokenPredictionResult {
    /// 指定した時間軸に最も近い予測ポイントを取得（±1h の許容範囲）
    pub fn prediction_at_horizon(&self, horizon_hours: usize) -> Option<&PredictedPrice> {
        if horizon_hours == 0 {
            return None;
        }
        let target = self.data_cutoff_time + chrono::TimeDelta::hours(horizon_hours as i64);
        let tolerance = chrono::TimeDelta::hours(1);
        self.predictions
            .iter()
            .filter(|p| (p.timestamp - target).abs() <= tolerance)
            .min_by_key(|p| (p.timestamp - target).abs())
    }
}

/// PredictionDataへの変換（momentum.rsから移動）
impl crate::algorithm::types::PredictionData {
    /// TokenPredictionResultから変換
    pub fn from_token_prediction(
        prediction: &TokenPredictionResult,
        current_price: TokenPrice,
    ) -> Option<Self> {
        // PredictionData は 24h 後の予測を格納する型のため、ホライゾン定数を使用
        let predicted_24h = prediction.prediction_at_horizon(PREDICTION_HORIZON_HOURS)?;

        Some(Self {
            token: prediction.token.clone(),
            current_price,
            predicted_price_24h: predicted_24h.price.clone(),
            timestamp: prediction.data_cutoff_time,
            confidence: predicted_24h.confidence.clone(),
        })
    }
}

#[cfg(test)]
mod tests;
