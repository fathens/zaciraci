mod connection_pool;
pub mod pool_info;
pub mod schema;
pub mod token_rate;

use chrono::NaiveDateTime;

pub struct TimeRange {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
}
