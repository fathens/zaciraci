use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// フロントエンドのモデルを再利用
pub use common::types::TokenAccount;

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenFileData {
    pub metadata: FileMetadata,
    pub token_data: TokenVolatilityData,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileMetadata {
    pub generated_at: DateTime<Utc>,
    pub start_date: String,
    pub end_date: String,
    pub token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote_token: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenVolatilityData {
    pub token: String,
    pub volatility_score: f64,
    pub price_data: PriceData,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PriceData {
    pub current_price: f64,
    pub price_change_24h: f64,
    pub volume_24h: f64,
}
