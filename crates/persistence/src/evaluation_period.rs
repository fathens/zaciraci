use crate::connection_pool;
use crate::schema::evaluation_periods;
use anyhow::{Context, Result};
use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use common::types::YoctoAmount;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = evaluation_periods)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct EvaluationPeriod {
    pub id: i32,
    pub period_id: String,
    pub start_time: NaiveDateTime,
    #[diesel(deserialize_as = BigDecimal)]
    #[diesel(serialize_as = BigDecimal)]
    pub initial_value: YoctoAmount,
    pub selected_tokens: Option<Vec<Option<String>>>,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, Insertable)]
#[diesel(table_name = evaluation_periods)]
pub struct NewEvaluationPeriod {
    pub period_id: String,
    pub start_time: NaiveDateTime,
    #[diesel(serialize_as = BigDecimal)]
    pub initial_value: YoctoAmount,
    pub selected_tokens: Option<Vec<Option<String>>>,
}

impl NewEvaluationPeriod {
    pub fn new(initial_value: YoctoAmount, selected_tokens: Vec<String>) -> Self {
        let selected_tokens_opt: Option<Vec<Option<String>>> = if selected_tokens.is_empty() {
            None
        } else {
            Some(selected_tokens.into_iter().map(Some).collect())
        };

        Self {
            period_id: format!("eval_{}", Uuid::new_v4()),
            start_time: chrono::Utc::now().naive_utc(),
            initial_value,
            selected_tokens: selected_tokens_opt,
        }
    }

    pub fn insert(self, conn: &mut PgConnection) -> QueryResult<EvaluationPeriod> {
        diesel::insert_into(evaluation_periods::table)
            .values(self)
            .get_result(conn)
    }

    pub async fn insert_async(self) -> Result<EvaluationPeriod> {
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(move |conn| self.insert(conn))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?;

        result.context("Failed to insert evaluation period")
    }
}

impl EvaluationPeriod {
    /// 最新の評価期間を取得
    pub fn get_latest(conn: &mut PgConnection) -> QueryResult<Option<EvaluationPeriod>> {
        evaluation_periods::table
            .order(evaluation_periods::start_time.desc())
            .first(conn)
            .optional()
    }

    /// 最新の評価期間を非同期で取得
    pub async fn get_latest_async() -> Result<Option<EvaluationPeriod>> {
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(Self::get_latest)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?;

        result.context("Failed to get latest evaluation period")
    }

    /// period_idで評価期間を取得
    pub fn get_by_period_id(
        conn: &mut PgConnection,
        period_id: &str,
    ) -> QueryResult<Option<EvaluationPeriod>> {
        evaluation_periods::table
            .filter(evaluation_periods::period_id.eq(period_id))
            .first(conn)
            .optional()
    }

    /// period_idで評価期間を非同期で取得
    pub async fn get_by_period_id_async(period_id: String) -> Result<Option<EvaluationPeriod>> {
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(move |conn| Self::get_by_period_id(conn, &period_id))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?;

        result.context("Failed to get evaluation period by period_id")
    }

    /// 全ての評価期間を取得（開始時刻降順）
    pub fn get_all(conn: &mut PgConnection) -> QueryResult<Vec<EvaluationPeriod>> {
        evaluation_periods::table
            .order(evaluation_periods::start_time.desc())
            .load(conn)
    }

    /// 全ての評価期間を非同期で取得
    pub async fn get_all_async() -> Result<Vec<EvaluationPeriod>> {
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(Self::get_all)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?;

        result.context("Failed to get all evaluation periods")
    }

    /// ページネーション付きで評価期間を取得（開始時刻降順）
    pub fn get_paginated(
        page: i64,
        page_size: i64,
        conn: &mut PgConnection,
    ) -> QueryResult<Vec<EvaluationPeriod>> {
        evaluation_periods::table
            .order(evaluation_periods::start_time.desc())
            .limit(page_size)
            .offset(page * page_size)
            .load(conn)
    }

    /// ページネーション付きで評価期間を非同期で取得
    pub async fn get_paginated_async(page: i64, page_size: i64) -> Result<Vec<EvaluationPeriod>> {
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(move |conn| Self::get_paginated(page, page_size, conn))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?;

        result.context("Failed to get paginated evaluation periods")
    }

    /// 評価期間の総数を取得
    pub fn count_all(conn: &mut PgConnection) -> QueryResult<i64> {
        evaluation_periods::table.count().first(conn)
    }

    /// 評価期間の総数を非同期で取得
    pub async fn count_all_async() -> Result<i64> {
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(Self::count_all)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?;

        result.context("Failed to count evaluation periods")
    }

    /// 選定トークンを更新
    pub fn update_selected_tokens(
        conn: &mut PgConnection,
        period_id: &str,
        tokens: Vec<String>,
    ) -> QueryResult<EvaluationPeriod> {
        let tokens_opt: Option<Vec<Option<String>>> = if tokens.is_empty() {
            None
        } else {
            Some(tokens.into_iter().map(Some).collect())
        };

        diesel::update(
            evaluation_periods::table.filter(evaluation_periods::period_id.eq(period_id)),
        )
        .set(evaluation_periods::selected_tokens.eq(tokens_opt))
        .get_result(conn)
    }

    /// 選定トークンを非同期で更新
    pub async fn update_selected_tokens_async(
        period_id: String,
        tokens: Vec<String>,
    ) -> Result<EvaluationPeriod> {
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(move |conn| Self::update_selected_tokens(conn, &period_id, tokens))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?;

        result.context("Failed to update selected tokens")
    }

    /// period_idで評価期間を削除
    pub async fn delete_by_period_id_async(period_id: String) -> Result<()> {
        let conn = connection_pool::get().await?;

        conn.interact(move |conn| {
            diesel::delete(
                evaluation_periods::table.filter(evaluation_periods::period_id.eq(&period_id)),
            )
            .execute(conn)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?
        .map_err(|e| anyhow::anyhow!("Failed to delete evaluation period: {}", e))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_get_evaluation_period() {
        let initial_value = YoctoAmount::from_u128(100_000_000_000_000_000_000_000_000);
        let tokens = vec!["token1.near".to_string(), "token2.near".to_string()];

        let new_period = NewEvaluationPeriod::new(initial_value, tokens);

        assert!(new_period.period_id.starts_with("eval_"));
        assert_eq!(new_period.selected_tokens.as_ref().unwrap().len(), 2);

        // 実際のDBへの挿入テストは統合テストで行う
    }

    #[test]
    fn test_new_evaluation_period_with_empty_tokens() {
        let initial_value = YoctoAmount::from_u128(100_000_000_000_000_000_000_000_000);
        let tokens = vec![];

        let new_period = NewEvaluationPeriod::new(initial_value, tokens);

        assert_eq!(new_period.selected_tokens, None);
    }

    #[tokio::test]
    async fn test_initial_value_db_roundtrip() {
        // 大きな値で DB ラウンドトリップが正しく YoctoAmount に戻ることを確認
        let initial_value = YoctoAmount::from_u128(100_000_000_000_000_000_000_000_000);
        let new_period = NewEvaluationPeriod::new(initial_value.clone(), vec![]);

        let created = new_period.insert_async().await.unwrap();
        assert_eq!(created.initial_value, initial_value);

        // DB から再取得しても一致
        let fetched = EvaluationPeriod::get_by_period_id_async(created.period_id.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched.initial_value, initial_value);

        // Cleanup
        EvaluationPeriod::delete_by_period_id_async(created.period_id)
            .await
            .unwrap();
    }
}
