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
        price_yocto_near: BigDecimal,
        evaluation_period_id: Option<String>,
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
            None, // evaluation_period_id
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

    #[tokio::test]
    async fn test_count_by_evaluation_period() {
        use crate::persistence::evaluation_period::NewEvaluationPeriod;

        // 評価期間を作成（外部キー制約のため）
        let new_period =
            NewEvaluationPeriod::new(BigDecimal::from(100000000000000000000000000i128), vec![]);
        let created_period = new_period.insert_async().await.unwrap();
        let period_id = created_period.period_id;
        let batch_id = uuid::Uuid::new_v4().to_string();

        // 同じevaluation_period_idで3つのトランザクションを作成
        let mut tx_ids = Vec::new();
        for i in 0..3 {
            let tx_id = format!("test_tx_count_{}_{}", i, uuid::Uuid::new_v4());
            tx_ids.push(tx_id.clone());

            let transaction = TradeTransaction::new(
                tx_id,
                batch_id.clone(),
                "wrap.near".to_string(),
                BigDecimal::from(1000000000000000000000000i128),
                "akaia.tkn.near".to_string(),
                BigDecimal::from(50000000000000000000000i128),
                BigDecimal::from(20000000000000000000i128),
                Some(period_id.clone()),
            );

            transaction.insert_async().await.unwrap();
        }

        // count_by_evaluation_period_asyncをテスト
        let count = TradeTransaction::count_by_evaluation_period_async(period_id.clone())
            .await
            .unwrap();
        assert_eq!(count, 3);

        // 存在しないperiod_idの場合は0を返す
        let count_non_existent =
            TradeTransaction::count_by_evaluation_period_async("non_existent_period".to_string())
                .await
                .unwrap();
        assert_eq!(count_non_existent, 0);

        // Cleanup
        for tx_id in tx_ids {
            TradeTransaction::delete_by_tx_id_async(tx_id)
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn test_transaction_with_evaluation_period_id() {
        use crate::persistence::evaluation_period::NewEvaluationPeriod;

        // 評価期間を作成（外部キー制約のため）
        let new_period =
            NewEvaluationPeriod::new(BigDecimal::from(100000000000000000000000000i128), vec![]);
        let created_period = new_period.insert_async().await.unwrap();
        let period_id = created_period.period_id;
        let batch_id = uuid::Uuid::new_v4().to_string();
        let tx_id = format!("test_tx_period_{}", uuid::Uuid::new_v4());

        // evaluation_period_id付きトランザクションを作成
        let transaction = TradeTransaction::new(
            tx_id.clone(),
            batch_id.clone(),
            "wrap.near".to_string(),
            BigDecimal::from(1000000000000000000000000i128),
            "akaia.tkn.near".to_string(),
            BigDecimal::from(50000000000000000000000i128),
            BigDecimal::from(20000000000000000000i128),
            Some(period_id.clone()),
        );

        let result = transaction.insert_async().await.unwrap();
        assert_eq!(result.tx_id, tx_id);
        assert_eq!(result.evaluation_period_id, Some(period_id.clone()));

        // 取得して確認
        let found = TradeTransaction::find_by_tx_id_async(tx_id.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.evaluation_period_id, Some(period_id));

        // Cleanup
        TradeTransaction::delete_by_tx_id_async(tx_id)
            .await
            .unwrap();
    }
}
