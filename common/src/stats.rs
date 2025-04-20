use crate::types::TokenAccount;
use chrono::{Duration, NaiveDateTime};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DescribesRequest {
    pub quote_token: String,
    pub base_token: String,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub period: Duration,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetValuesRequest {
    pub quote_token: TokenAccount,
    pub base_token: TokenAccount,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GetValuesResponse {
    pub values: Vec<ValueAtTime>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ValueAtTime {
    pub value: f64,
    pub time: NaiveDateTime,
}
