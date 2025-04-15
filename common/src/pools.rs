use crate::types::{TokenAccount, YoctoNearToken};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct TradeRequest {
    pub timestamp: NaiveDateTime,
    pub token_in: TokenAccount,
    pub token_out: TokenAccount,
    pub amount_in: YoctoNearToken,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct TradeResponse {
    pub amount_out: YoctoNearToken,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PoolRecordsRequest {
    pub timestamp: NaiveDateTime,
    pub pool_ids: Vec<PoolId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PoolRecordsResponse {
    pub pools: Vec<PoolRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PoolRecord {
    pub id: PoolId,
    pub timestamp: NaiveDateTime,
    pub bare: PoolBared,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PoolBared {
    pub pool_kind: String,
    pub token_account_ids: Vec<TokenAccount>,
    pub amounts: Vec<YoctoNearToken>,
    pub total_fee: u32,
    pub shares_total_supply: YoctoNearToken,
    pub amp: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub struct PoolId(pub u32);

impl From<u32> for PoolId {
    fn from(id: u32) -> Self {
        PoolId(id)
    }
}

impl From<PoolId> for u32 {
    fn from(id: PoolId) -> Self {
        id.0
    }
}
