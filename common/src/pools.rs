use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use crate::types::{TokenAccount, YoctoNearToken};

#[derive(Deserialize, Serialize)]
pub struct TradeRequest {
    pub timestamp: NaiveDateTime,
    pub token_in: TokenAccount,
    pub token_out: TokenAccount,
    pub amount_in: YoctoNearToken,
}

#[derive(Deserialize, Serialize)]
pub struct TradeResponse {
    pub amount_out: YoctoNearToken,
}