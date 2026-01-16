use crate::persistence::connection_pool;
use crate::persistence::schema::trade_transactions;
use anyhow::{Context, Result};
use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = trade_transactions)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct TradeTransaction {
    pub tx_id: String,
    pub trade_batch_id: String,
    pub from_token: String,
    pub from_amount: BigDecimal,
    pub to_token: String,
    pub to_amount: BigDecimal,
    pub timestamp: NaiveDateTime,
    pub evaluation_period_id: Option<String>,
}

impl TradeTransaction {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tx_id: String,
        trade_batch_id: String,
        from_token: String,
        from_amount: BigDecimal,
        to_token: String,
        to_amount: BigDecimal,
        evaluation_period_id: Option<String>,
    ) -> Self {
        Self {
            tx_id,
            trade_batch_id,
            from_token,
            from_amount,
            to_token,
            to_amount,
            timestamp: chrono::Utc::now().naive_utc(),
            evaluation_period_id,
        }
    }

    pub fn insert(&self, conn: &mut PgConnection) -> QueryResult<TradeTransaction> {
        diesel::insert_into(trade_transactions::table)
            .values(self)
            .get_result(conn)
    }

    pub async fn insert_async(&self) -> Result<TradeTransaction> {
        let transaction = self.clone();
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(move |conn| transaction.insert(conn))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?;

        result.context("Failed to insert trade transaction")
    }

    pub fn insert_batch(
        transactions: Vec<Self>,
        conn: &mut PgConnection,
    ) -> QueryResult<Vec<TradeTransaction>> {
        diesel::insert_into(trade_transactions::table)
            .values(&transactions)
            .get_results(conn)
    }

    pub async fn insert_batch_async(transactions: Vec<Self>) -> Result<Vec<TradeTransaction>> {
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(move |conn| Self::insert_batch(transactions, conn))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?;

        result.context("Failed to insert batch of trade transactions")
    }

    pub fn find_by_batch_id(
        batch_id: &str,
        conn: &mut PgConnection,
    ) -> QueryResult<Vec<TradeTransaction>> {
        trade_transactions::table
            .filter(trade_transactions::trade_batch_id.eq(batch_id))
            .order(trade_transactions::timestamp.asc())
            .get_results(conn)
    }

    pub async fn find_by_batch_id_async(batch_id: String) -> Result<Vec<TradeTransaction>> {
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(move |conn| Self::find_by_batch_id(&batch_id, conn))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?;

        result.context("Failed to find transactions by batch ID")
    }

    pub fn find_by_tx_id(
        tx_id: &str,
        conn: &mut PgConnection,
    ) -> QueryResult<Option<TradeTransaction>> {
        trade_transactions::table
            .filter(trade_transactions::tx_id.eq(tx_id))
            .first(conn)
            .optional()
    }

    pub async fn find_by_tx_id_async(tx_id: String) -> Result<Option<TradeTransaction>> {
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(move |conn| Self::find_by_tx_id(&tx_id, conn))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?;

        result.context("Failed to find transaction by tx ID")
    }

    pub fn get_latest_batch_id(conn: &mut PgConnection) -> QueryResult<Option<String>> {
        trade_transactions::table
            .select(trade_transactions::trade_batch_id)
            .order(trade_transactions::timestamp.desc())
            .first::<String>(conn)
            .optional()
    }

    pub async fn get_latest_batch_id_async() -> Result<Option<String>> {
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(Self::get_latest_batch_id)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?;

        result.context("Failed to get latest batch ID")
    }

    pub async fn delete_by_tx_id_async(tx_id: String) -> Result<()> {
        let conn = connection_pool::get().await?;

        conn.interact(move |conn| {
            diesel::delete(trade_transactions::table.filter(trade_transactions::tx_id.eq(&tx_id)))
                .execute(conn)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?
        .map_err(|e| anyhow::anyhow!("Failed to delete transaction: {}", e))?;

        Ok(())
    }

    /// 指定した評価期間のトランザクション数を取得
    pub fn count_by_evaluation_period(
        period_id: &str,
        conn: &mut PgConnection,
    ) -> QueryResult<i64> {
        use diesel::dsl::count;

        trade_transactions::table
            .filter(trade_transactions::evaluation_period_id.eq(period_id))
            .select(count(trade_transactions::tx_id))
            .first(conn)
    }

    pub async fn count_by_evaluation_period_async(period_id: String) -> Result<i64> {
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(move |conn| Self::count_by_evaluation_period(&period_id, conn))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?;

        result.context("Failed to count transactions by evaluation period")
    }
}

#[cfg(test)]
mod tests;
