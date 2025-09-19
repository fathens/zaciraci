use crate::persistence::connection_pool;
use crate::persistence::schema::trade_transactions;
use anyhow::{Context, Result};
use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

pub type BatchSummary = (String, Option<BigDecimal>, i64, NaiveDateTime);
pub type TimelineSummary = (String, Option<BigDecimal>, NaiveDateTime);

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
    pub price_yocto_near: BigDecimal,
    pub timestamp: NaiveDateTime,
}

impl TradeTransaction {
    pub fn new(
        tx_id: String,
        trade_batch_id: String,
        from_token: String,
        from_amount: BigDecimal,
        to_token: String,
        to_amount: BigDecimal,
        price_yocto_near: BigDecimal,
    ) -> Self {
        Self {
            tx_id,
            trade_batch_id,
            from_token,
            from_amount,
            to_token,
            to_amount,
            price_yocto_near,
            timestamp: chrono::Utc::now().naive_utc(),
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

    pub fn get_portfolio_value_by_batch(
        batch_id: &str,
        conn: &mut PgConnection,
    ) -> QueryResult<BigDecimal> {
        use diesel::dsl::sum;

        trade_transactions::table
            .filter(trade_transactions::trade_batch_id.eq(batch_id))
            .select(sum(trade_transactions::price_yocto_near))
            .first::<Option<BigDecimal>>(conn)
            .map(|opt| opt.unwrap_or_else(|| BigDecimal::from(0)))
    }

    pub async fn get_portfolio_value_by_batch_async(batch_id: String) -> Result<BigDecimal> {
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(move |conn| Self::get_portfolio_value_by_batch(&batch_id, conn))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?;

        result.context("Failed to get portfolio value")
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

    pub fn get_batch_summary(
        conn: &mut PgConnection,
        limit: i64,
    ) -> QueryResult<Vec<BatchSummary>> {
        use diesel::dsl::{count, min, sum};

        trade_transactions::table
            .group_by(trade_transactions::trade_batch_id)
            .select((
                trade_transactions::trade_batch_id,
                sum(trade_transactions::price_yocto_near),
                count(trade_transactions::tx_id),
                min(trade_transactions::timestamp).assume_not_null(),
            ))
            .order(min(trade_transactions::timestamp).desc())
            .limit(limit)
            .load(conn)
    }

    pub fn get_portfolio_timeline(conn: &mut PgConnection) -> QueryResult<Vec<TimelineSummary>> {
        use diesel::dsl::{min, sum};

        trade_transactions::table
            .group_by(trade_transactions::trade_batch_id)
            .select((
                trade_transactions::trade_batch_id,
                sum(trade_transactions::price_yocto_near),
                min(trade_transactions::timestamp).assume_not_null(),
            ))
            .order(min(trade_transactions::timestamp).asc())
            .load(conn)
    }

    pub async fn get_portfolio_timeline_async() -> Result<Vec<TimelineSummary>> {
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(Self::get_portfolio_timeline)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?;

        result.context("Failed to get portfolio timeline")
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_trade_transaction_crud() {
        let batch_id = uuid::Uuid::new_v4().to_string();
        let tx_id = format!("test_tx_{}", uuid::Uuid::new_v4());

        let transaction = TradeTransaction::new(
            tx_id.clone(),
            batch_id.clone(),
            "wrap.near".to_string(),
            BigDecimal::from(1000000000000000000000000i128), // 1 NEAR
            "akaia.tkn.near".to_string(),
            BigDecimal::from(50000000000000000000000i128),
            BigDecimal::from(20000000000000000000i128),
        );

        let result = transaction.insert_async().await.unwrap();
        assert_eq!(result.tx_id, tx_id);
        assert_eq!(result.trade_batch_id, batch_id);

        let found = TradeTransaction::find_by_tx_id_async(tx_id.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.tx_id, tx_id);

        let batch_transactions = TradeTransaction::find_by_batch_id_async(batch_id)
            .await
            .unwrap();
        assert_eq!(batch_transactions.len(), 1);

        TradeTransaction::delete_by_tx_id_async(tx_id)
            .await
            .unwrap();
    }
}
