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
