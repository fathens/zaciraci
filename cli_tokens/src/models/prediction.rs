use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use common::types::Price;
use serde::{Deserialize, Serialize};

/// Prediction file metadata (similar to HistoryMetadata)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionMetadata {
    pub generated_at: DateTime<Utc>,
    pub model_name: String,
    pub base_token: String,
    pub quote_token: String,
    pub history_start: String,    // YYYY-MM-DD format
    pub history_end: String,      // YYYY-MM-DD format
    pub prediction_start: String, // YYYY-MM-DD format
    pub prediction_end: String,   // YYYY-MM-DD format
}

/// Individual prediction point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionPoint {
    pub timestamp: DateTime<Utc>,
    /// 予測価格（無次元の価格比率）
    pub price: Price,
    pub confidence: Option<BigDecimal>,
}

/// Prediction results container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionResults {
    pub predictions: Vec<PredictionPoint>,
    pub model_metrics: Option<serde_json::Value>, // Flexible metrics from model
}

/// Complete prediction file data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionFileData {
    pub metadata: PredictionMetadata,
    pub prediction_results: PredictionResults,
}
