use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use std::str::FromStr;
use anyhow::anyhow;

use crate::persistence::connection_pool;
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
    pub timestamp: NaiveDateTime,
}

// アプリケーションロジック用モデル
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TokenRate {
    pub base: TokenOutAccount,
    pub quote: TokenInAccount,
    pub rate: BigDecimal,
    pub timestamp: NaiveDateTime,
}

// 相互変換の実装
#[allow(dead_code)]
impl TokenRate {
    // 新しいTokenRateインスタンスを現在時刻で作成
    pub fn new(base: TokenOutAccount, quote: TokenInAccount, rate: BigDecimal) -> Self {
        Self {
            base,
            quote,
            rate,
            timestamp: chrono::Utc::now().naive_utc(),
        }
    }

    // 特定の時刻でTokenRateインスタンスを作成
    pub fn with_timestamp(base: TokenOutAccount, quote: TokenInAccount, rate: BigDecimal, timestamp: NaiveDateTime) -> Self {
        Self {
            base,
            quote,
            rate,
            timestamp,
        }
    }

    // DbTokenRate からの変換
    pub fn from_db(db_rate: DbTokenRate) -> Result<Self> {
        Ok(Self {
            base: TokenAccount::from_str(&db_rate.base_token)?.into(),
            quote: TokenAccount::from_str(&db_rate.quote_token)?.into(),
            rate: db_rate.rate,
            timestamp: db_rate.timestamp,
        })
    }
    
    // DbTokenRateへの変換
    pub fn to_new_db(&self) -> NewDbTokenRate {
        NewDbTokenRate {
            base_token: self.base.to_string(),
            quote_token: self.quote.to_string(),
            rate: self.rate.clone(),
            timestamp: self.timestamp,
        }
    }
}

// データベース操作
#[allow(dead_code)]
impl DbTokenRate {
    // 単一レコードの挿入
    pub async fn insert(token_rate: &TokenRate) -> Result<DbTokenRate> {
        use diesel::RunQueryDsl;

        let new_rate = token_rate.to_new_db();
        let conn = connection_pool::get().await?;
        
        let result = conn.interact(move |conn| {
            diesel::insert_into(token_rates::table)
                .values(&new_rate)
                .get_result::<DbTokenRate>(conn)
        }).await.map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
        
        Ok(result)
    }

    // 複数レコードの一括挿入
    pub async fn batch_insert(token_rates: &[TokenRate]) -> Result<Vec<DbTokenRate>> {
        use diesel::RunQueryDsl;

        if token_rates.is_empty() {
            return Ok(Vec::new());
        }

        let new_rates: Vec<NewDbTokenRate> = token_rates
            .iter()
            .map(|rate| rate.to_new_db())
            .collect();

        let conn = connection_pool::get().await?;

        let results = conn.interact(move |conn| {
            diesel::insert_into(token_rates::table)
                .values(&new_rates)
                .get_results::<DbTokenRate>(conn)
        }).await.map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

        Ok(results)
    }

    // 最新のレコードを取得
    pub async fn get_latest(base: &TokenOutAccount, quote: &TokenInAccount) -> Result<Option<TokenRate>> {
        use diesel::dsl::max;
        use diesel::QueryDsl;

        let base_str = base.to_string();
        let quote_str = quote.to_string();
        let conn = connection_pool::get().await?;

        // まず最新のタイムスタンプを検索
        let latest_timestamp = conn.interact(move |conn| {
            token_rates::table
                .filter(token_rates::base_token.eq(&base_str))
                .filter(token_rates::quote_token.eq(&quote_str))
                .select(max(token_rates::timestamp))
                .first::<Option<NaiveDateTime>>(conn)
                .optional()
        }).await.map_err(|e| anyhow!("Database interaction error: {:?}", e))??
            .flatten();

        // タイムスタンプが存在する場合、そのレコードを取得
        if let Some(timestamp) = latest_timestamp {
            let base_str = base.to_string();
            let quote_str = quote.to_string();
            let conn = connection_pool::get().await?;

            let result = conn.interact(move |conn| {
                token_rates::table
                    .filter(token_rates::base_token.eq(&base_str))
                    .filter(token_rates::quote_token.eq(&quote_str))
                    .filter(token_rates::timestamp.eq(timestamp))
                    .first::<DbTokenRate>(conn)
            }).await.map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
            
            let token_rate = TokenRate::from_db(result)?;
            Ok(Some(token_rate))
        } else {
            Ok(None)
        }
    }

    // 履歴レコードを取得（新しい順）
    pub async fn get_history(base: &TokenOutAccount, quote: &TokenInAccount, limit: i64) -> Result<Vec<TokenRate>> {
        use diesel::QueryDsl;

        let base_str = base.to_string();
        let quote_str = quote.to_string();
        let conn = connection_pool::get().await?;

        let results = conn.interact(move |conn| {
            token_rates::table
                .filter(token_rates::base_token.eq(&base_str))
                .filter(token_rates::quote_token.eq(&quote_str))
                .order(token_rates::timestamp.desc())
                .limit(limit)
                .load::<DbTokenRate>(conn)
        }).await.map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
            
        let token_rates = results
            .into_iter()
            .map(TokenRate::from_db)
            .collect::<Result<Vec<_>>>()?;
            
        Ok(token_rates)
    }
}