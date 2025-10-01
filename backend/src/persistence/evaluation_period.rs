use crate::persistence::connection_pool;
use crate::persistence::schema::evaluation_periods;
use anyhow::{Context, Result};
use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
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
    pub initial_value: BigDecimal,
    pub selected_tokens: Option<Vec<Option<String>>>,
    pub token_count: i32,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, Insertable)]
#[diesel(table_name = evaluation_periods)]
pub struct NewEvaluationPeriod {
    pub period_id: String,
    pub start_time: NaiveDateTime,
    pub initial_value: BigDecimal,
    pub selected_tokens: Option<Vec<Option<String>>>,
    pub token_count: i32,
}

impl NewEvaluationPeriod {
    pub fn new(initial_value: BigDecimal, selected_tokens: Vec<String>) -> Self {
        let token_count = selected_tokens.len() as i32;
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
            token_count,
        }
    }

    pub fn insert(&self, conn: &mut PgConnection) -> QueryResult<EvaluationPeriod> {
        diesel::insert_into(evaluation_periods::table)
            .values(self)
            .get_result(conn)
    }

    pub async fn insert_async(&self) -> Result<EvaluationPeriod> {
        let self_clone = self.clone();
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(move |conn| self_clone.insert(conn))
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use bigdecimal::BigDecimal;
    use std::str::FromStr;

    #[tokio::test]
    async fn test_create_and_get_evaluation_period() {
        let initial_value = BigDecimal::from_str("100000000000000000000000000").unwrap();
        let tokens = vec!["token1.near".to_string(), "token2.near".to_string()];

        let new_period = NewEvaluationPeriod::new(initial_value.clone(), tokens);

        assert_eq!(new_period.token_count, 2);
        assert!(new_period.period_id.starts_with("eval_"));

        // 実際のDBへの挿入テストは統合テストで行う
    }

    #[test]
    fn test_new_evaluation_period_with_empty_tokens() {
        let initial_value = BigDecimal::from_str("100000000000000000000000000").unwrap();
        let tokens = vec![];

        let new_period = NewEvaluationPeriod::new(initial_value, tokens);

        assert_eq!(new_period.token_count, 0);
        assert_eq!(new_period.selected_tokens, None);
    }
}
