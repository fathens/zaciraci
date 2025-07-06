use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// フロントエンドの予測モデルを再利用
#[derive(Debug, Serialize, Deserialize)]
pub struct ZeroShotPredictionRequest {
    pub timestamp: Vec<DateTime<Utc>>,
    pub values: Vec<f64>,
    pub forecast_until: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_params: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PredictionResponse {
    pub id: String,
    pub status: String,
    pub forecast: Option<Vec<PredictionPoint>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AsyncPredictionResponse {
    pub task_id: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PredictionResult {
    pub task_id: String,
    pub status: String,
    pub progress: Option<f64>,
    pub message: Option<String>,
    pub result: Option<ChronosPredictionResponse>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChronosPredictionResponse {
    pub forecast_timestamp: Vec<DateTime<Utc>>,
    pub forecast_values: Vec<f64>,
    pub model_name: String,
    pub confidence_intervals: Option<HashMap<String, Vec<f64>>>,
    pub metrics: Option<HashMap<String, f64>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PredictionPoint {
    pub timestamp: DateTime<Utc>,
    pub value: f64,
    pub confidence_interval: Option<ConfidenceInterval>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfidenceInterval {
    pub lower: f64,
    pub upper: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenPredictionResult {
    pub token: String,
    pub prediction_id: String,
    pub predicted_values: Vec<PredictionPoint>,
    pub accuracy_metrics: Option<AccuracyMetrics>,
    pub chart_svg: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AccuracyMetrics {
    pub mae: f64,
    pub rmse: f64,
    pub mape: f64,
}
