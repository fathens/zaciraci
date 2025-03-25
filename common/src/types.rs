use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// フロントエンドとバックエンド間で共有するデータモデル
/// バックエンドの既存モデルからフロントエンド用に必要な属性だけを抽出することもあります

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    pub amount: String,
    pub timestamp: DateTime<Utc>,
    pub status: TransactionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionStatus {
    Pending,
    Completed,
    Failed,
}
