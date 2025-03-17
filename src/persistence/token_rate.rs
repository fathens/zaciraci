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
    pub fn new_with_timestamp(base: TokenOutAccount, quote: TokenInAccount, rate: BigDecimal, timestamp: NaiveDateTime) -> Self {
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

    // quoteトークンを指定して対応するすべてのbaseトークンとその最新時刻を取得
    pub async fn get_latests_by_quote(quote: &TokenInAccount) -> Result<Vec<(TokenOutAccount, NaiveDateTime)>> {
        use diesel::dsl::max;
        use diesel::QueryDsl;

        let quote_str = quote.to_string();
        let conn = connection_pool::get().await?;

        // 各base_tokenごとに最新のタイムスタンプを取得
        let latest_timestamps = conn.interact(move |conn| {
            token_rates::table
                .filter(token_rates::quote_token.eq(&quote_str))
                .group_by(token_rates::base_token)
                .select((
                    token_rates::base_token,
                    max(token_rates::timestamp)
                ))
                .load::<(String, Option<NaiveDateTime>)>(conn)
        }).await.map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

        // 結果をトークンとタイムスタンプのペアに変換
        let mut results = Vec::new();
        for (base_token, timestamp_opt) in latest_timestamps {
            if let Some(timestamp) = timestamp_opt {
                match TokenAccount::from_str(&base_token) {
                    Ok(token) => results.push((token.into(), timestamp)),
                    Err(e) => return Err(anyhow!("Failed to parse token: {:?}", e)),
                }
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel::RunQueryDsl;

    // テーブルからすべてのレコードを削除する補助関数
    async fn clean_table() -> Result<()> {
        let conn = connection_pool::get().await?;
        conn.interact(|conn| {
            diesel::delete(token_rates::table)
                .execute(conn)
        }).await.map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
        Ok(())
    }

    #[tokio::test]
    async fn test_token_rate_single_insert() -> Result<()> {
        // 1. テーブルの全レコード削除
        clean_table().await?;

        // テスト用のトークンアカウント作成
        let base = TokenAccount::from_str("eth.token")?.into();
        let quote = TokenAccount::from_str("usdt.token")?.into();
        
        // 2. get_latest で None が返ることを確認
        let result = TokenRate::get_latest(&base, &quote).await?;
        assert!(result.is_none(), "Empty table should return None");

        // 3. １つインサート
        let rate = BigDecimal::from(1000);
        let timestamp = chrono::Utc::now().naive_utc();
        let token_rate = TokenRate::new_with_timestamp(
            base.clone(),
            quote.clone(),
            rate.clone(),
            timestamp,
        );
        token_rate.insert().await?;

        // 4. get_latest でインサートしたレコードが返ることを確認
        let result = TokenRate::get_latest(&base, &quote).await?;
        assert!(result.is_some(), "Should return inserted record");
        
        let retrieved_rate = result.unwrap();
        assert_eq!(retrieved_rate.base.to_string(), "eth.token", "Base token should match");
        assert_eq!(retrieved_rate.quote.to_string(), "usdt.token", "Quote token should match");
        assert_eq!(retrieved_rate.rate, rate, "Rate should match");
        assert_eq!(retrieved_rate.timestamp, timestamp, "Timestamp should match");

        // クリーンアップ
        clean_table().await?;
        
        Ok(())
    }
}