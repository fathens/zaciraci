use anyhow::anyhow;
use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use std::str::FromStr;

use crate::persistence::connection_pool;
use crate::persistence::schema::token_rates;
use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount, TokenAccount};
use crate::Result;

// データベース用モデル
#[allow(dead_code)]
#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = token_rates)]
struct DbTokenRate {
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
struct NewDbTokenRate {
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

    // DBオブジェクトから変換
    fn from_db(db_rate: DbTokenRate) -> Result<Self> {
        let base = TokenAccount::from_str(&db_rate.base_token)?.into();
        let quote = TokenAccount::from_str(&db_rate.quote_token)?.into();

        Ok(Self {
            base,
            quote,
            rate: db_rate.rate,
            timestamp: db_rate.timestamp,
        })
    }

    // NewDbTokenRateに変換
    fn to_new_db(&self) -> NewDbTokenRate {
        NewDbTokenRate {
            base_token: self.base.to_string(),
            quote_token: self.quote.to_string(),
            rate: self.rate.clone(),
            timestamp: self.timestamp,
        }
    }

    // データベースに挿入
    pub async fn insert(&self) -> Result<()> {
        use diesel::RunQueryDsl;

        let new_rate = self.to_new_db();
        let conn = connection_pool::get().await?;
        
        conn.interact(move |conn| {
            diesel::insert_into(token_rates::table)
                .values(&new_rate)
                .execute(conn)
        }).await.map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
        
        Ok(())
    }

    // 複数レコードを一括挿入
    pub async fn batch_insert(token_rates: &[TokenRate]) -> Result<()> {
        use diesel::RunQueryDsl;

        if token_rates.is_empty() {
            return Ok(());
        }

        let new_rates: Vec<NewDbTokenRate> = token_rates
            .iter()
            .map(|rate| rate.to_new_db())
            .collect();

        let conn = connection_pool::get().await?;

        conn.interact(move |conn| {
            diesel::insert_into(token_rates::table)
                .values(&new_rates)
                .execute(conn)
        }).await.map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

        Ok(())
    }

    // 最新のレートを取得
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
            
            Ok(Some(TokenRate::from_db(result)?))
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
            
        results
            .into_iter()
            .map(TokenRate::from_db)
            .collect()
    }
}