pub mod momentum;
pub mod portfolio;
pub mod prediction;
pub mod trend_following;
pub mod types;
pub mod indicators;

// Re-export common types and indicators for convenience
pub use types::*;
pub use indicators::*;

use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ==================== 共通型定義 ====================

/// 取引の種類
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TradeType {
    Buy,
    Sell,
    Swap,
}

/// 価格履歴データ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceHistory {
    pub timestamp: DateTime<Utc>,
    pub price: BigDecimal,
    pub volume: Option<BigDecimal>,
}

/// トークン情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub symbol: String,
    pub address: String,
    pub decimals: u8,
    pub current_price: BigDecimal,
}

/// 取引実行結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeExecution {
    pub trade_type: TradeType,
    pub token_in: String,
    pub token_out: String,
    pub amount_in: BigDecimal,
    pub amount_out: BigDecimal,
    pub timestamp: DateTime<Utc>,
    pub cost: BigDecimal,
    pub success: bool,
}

// ==================== 共通関数は indicators.rs に移動 ====================

// ==================== 全ての共通関数とテストは indicators.rs と types.rs に移動 ====================
