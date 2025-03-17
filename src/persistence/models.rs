use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use diesel::prelude::*;

use crate::persistence::schema::token_rates;

#[allow(dead_code)]
#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = token_rates)]
pub struct TokenRate {
    pub id: i32,
    pub base_token: String,
    pub quote_token: String,
    pub rate: BigDecimal,
    pub timestamp: NaiveDateTime,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = token_rates)]
pub struct NewTokenRate {
    pub base_token: String,
    pub quote_token: String,
    pub rate: BigDecimal,
}
