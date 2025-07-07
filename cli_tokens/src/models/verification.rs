use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct VerificationReport {
    pub token: String,
    pub prediction_id: String,
    pub verification_date: DateTime<Utc>,
    pub period: VerificationPeriod,
    pub metrics: VerificationMetrics,
    pub data_points: Vec<ComparisonPoint>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VerificationPeriod {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub predicted_points_count: usize,
    pub actual_points_count: usize,
    pub matched_points_count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VerificationMetrics {
    pub mae: f64,                // Mean Absolute Error
    pub rmse: f64,               // Root Mean Square Error
    pub mape: f64,               // Mean Absolute Percentage Error
    pub direction_accuracy: f64, // 上昇/下降の予測精度 (0.0-1.0)
    pub correlation: f64,        // 相関係数 (-1.0 to 1.0)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ComparisonPoint {
    pub timestamp: DateTime<Utc>,
    pub predicted_value: f64,
    pub actual_value: f64,
    pub error: f64,
    pub percentage_error: f64,
}
