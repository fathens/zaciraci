use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use std::str::FromStr;

use crate::persistence::schema::token_rates;
use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount, TokenAccount};
use crate::Result;

// データベース用モデル
#[allow(dead_code)]
#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = token_rates)]
pub struct DbTokenRate {
    pub id: i32,
    pub base_token: String,
    pub quote_token: String,
    pub rate: BigDecimal,
    pub timestamp: NaiveDateTime,
}

// データベース挿入用モデル
#[allow(dead_code)]
#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = token_rates)]
pub struct NewDbTokenRate {
    pub base_token: String,
    pub quote_token: String,
    pub rate: BigDecimal,
}

// アプリケーションロジック用モデル
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TokenRate {
    pub base: TokenInAccount,
    pub quote: TokenOutAccount,
    pub rate: BigDecimal,
}

// 相互変換の実装
#[allow(dead_code)]
impl TokenRate {
    // DbTokenRate からの変換
    pub fn from_db(db_rate: DbTokenRate) -> Result<Self> {
        Ok(Self {
            base: TokenInAccount::from(TokenAccount::from_str(&db_rate.base_token)?),
            quote: TokenOutAccount::from(TokenAccount::from_str(&db_rate.quote_token)?),
            rate: db_rate.rate,
        })
    }
    
    // DbTokenRateへの変換
    pub fn to_new_db(&self) -> NewDbTokenRate {
        NewDbTokenRate {
            base_token: self.base.to_string(),
            quote_token: self.quote.to_string(),
            rate: self.rate.clone(),
        }
    }
}