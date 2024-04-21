use crate::Result;
use chrono::{DateTime, Utc};
use near_sdk::json_types::U128;

#[derive(Debug, Clone)]
pub struct PoolInfo {
    pub index: i32,
    pub kind: String,
    pub token_a: String,
    pub token_b: String,
    pub amount_a: U128,
    pub amount_b: U128,
    pub total_fee: u32,
    pub shares_total_supply: U128,
    pub amp: u64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct PoolInfoList(pub Vec<PoolInfo>);

pub async fn get_all() -> Result<PoolInfoList> {
    todo!("get_all")
}

pub async fn delete_all() -> Result<()> {
    todo!("delete_all")
}

pub async fn insert_all(_: PoolInfoList) -> Result<()> {
    todo!("insert")
}
