use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// フロントエンドのモデルを再利用
pub use common::types::TokenAccount;

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenFileData {
    pub metadata: FileMetadata,
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileMetadata {
    pub generated_at: DateTime<Utc>,
    pub start_date: String,
    pub end_date: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote_token: Option<String>,
}
