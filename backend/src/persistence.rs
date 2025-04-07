mod connection_pool;
pub mod schema;
pub mod token_rate;
pub mod pool_info;

use chrono::NaiveDateTime;

pub struct TimeRange{
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
}