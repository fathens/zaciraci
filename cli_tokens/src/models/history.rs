use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use common::stats::ValueAtTime;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HistoryFileData {
    pub metadata: HistoryMetadata,
    pub price_history: PriceHistory,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HistoryMetadata {
    pub generated_at: DateTime<Utc>,
    pub start_date: String,
    pub end_date: String,
    pub base_token: String,
    pub quote_token: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PriceHistory {
    pub values: Vec<ValueAtTime>,
}
