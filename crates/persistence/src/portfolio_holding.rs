use crate::connection_pool;
use crate::schema::portfolio_holdings;
use anyhow::Result;
use chrono::NaiveDateTime;
use common::types::TokenSmallestUnits;
use common::types::token_account::TokenAccount;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

/// JSONB 用の個別トークン保有量
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenHolding {
    pub token: TokenAccount,
    pub balance: TokenSmallestUnits,
    pub decimals: u8,
}

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = portfolio_holdings)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[allow(dead_code)] // Diesel Queryable でDBスキーマと一致させるため必要
pub struct DbPortfolioHolding {
    pub id: i32,
    pub evaluation_period_id: String,
    pub timestamp: NaiveDateTime,
    pub token_holdings: serde_json::Value,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = portfolio_holdings)]
pub struct NewPortfolioHolding {
    pub evaluation_period_id: String,
    pub timestamp: NaiveDateTime,
    pub token_holdings: serde_json::Value,
}

impl DbPortfolioHolding {
    /// token_holdings JSONB を TokenHolding の Vec にパース
    pub fn parse_holdings(&self) -> Result<Vec<TokenHolding>> {
        serde_json::from_value(self.token_holdings.clone())
            .map_err(|e| anyhow::anyhow!("Failed to parse token_holdings: {}", e))
    }
}

pub struct PortfolioHolding;

impl PortfolioHolding {
    /// 1件挿入
    pub async fn insert_async(record: NewPortfolioHolding) -> Result<()> {
        let conn = connection_pool::get().await?;

        conn.interact(move |conn| {
            diesel::insert_into(portfolio_holdings::table)
                .values(&record)
                .execute(conn)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

        Ok(())
    }

    /// 期間の全レコード取得 (timestamp DESC)
    pub async fn get_by_period_async(period_id: String) -> Result<Vec<DbPortfolioHolding>> {
        let conn = connection_pool::get().await?;

        let results = conn
            .interact(move |conn| {
                portfolio_holdings::table
                    .filter(portfolio_holdings::evaluation_period_id.eq(&period_id))
                    .order_by(portfolio_holdings::timestamp.desc())
                    .load::<DbPortfolioHolding>(conn)
            })
            .await
            .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

        Ok(results)
    }

    /// 期間の最新1件を取得
    pub async fn get_latest_for_period_async(
        period_id: String,
    ) -> Result<Option<DbPortfolioHolding>> {
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(move |conn| {
                portfolio_holdings::table
                    .filter(portfolio_holdings::evaluation_period_id.eq(&period_id))
                    .order_by(portfolio_holdings::timestamp.desc())
                    .first::<DbPortfolioHolding>(conn)
                    .optional()
            })
            .await
            .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

        Ok(result)
    }

    /// 古いレコードを削除
    pub async fn cleanup_old_records(retention_days: u16) -> Result<usize> {
        let conn = connection_pool::get().await?;
        let cutoff =
            chrono::Utc::now().naive_utc() - chrono::TimeDelta::days(i64::from(retention_days));

        let deleted = conn
            .interact(move |conn| {
                diesel::delete(
                    portfolio_holdings::table.filter(portfolio_holdings::timestamp.lt(cutoff)),
                )
                .execute(conn)
            })
            .await
            .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

        Ok(deleted)
    }
}

#[cfg(test)]
mod tests;
