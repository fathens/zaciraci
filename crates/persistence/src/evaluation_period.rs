use crate::connection_pool;
use crate::schema::evaluation_periods;
use anyhow::{Context, Result};
use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use common::types::YoctoAmount;
use diesel::prelude::*;
use logging::*;
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

    /// period_idで評価期間を削除（テスト専用）
    #[cfg(any(test, feature = "mock"))]
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

/// Minimum retention period to prevent accidental mass deletion
const MIN_RETENTION_DAYS: u32 = 30;

/// 指定日数より古いレコードを削除
///
/// ON DELETE CASCADE により、関連する trade_transactions と portfolio_holdings も連鎖削除される。
pub async fn cleanup_old_records(retention_days: u32) -> Result<()> {
    use diesel::sql_types::Timestamp;

    let log = DEFAULT.new(o!(
        "function" => "evaluation_period::cleanup_old_records",
        "retention_days" => retention_days,
    ));

    if retention_days == 0 {
        warn!(
            log,
            "retention_days is 0, skipping cleanup to prevent deleting all records"
        );
        return Ok(());
    }

    let effective_days = retention_days.max(MIN_RETENTION_DAYS);
    if effective_days != retention_days {
        warn!(log, "retention_days below minimum, using minimum";
            "requested" => retention_days, "effective" => effective_days);
    }

    trace!(log, "start");

    let cutoff_date =
        chrono::Utc::now().naive_utc() - chrono::TimeDelta::days(i64::from(effective_days));

    let conn = connection_pool::get().await?;

    let deleted_count = conn
        .interact(move |conn| {
            diesel::sql_query("DELETE FROM evaluation_periods WHERE created_at < $1")
                .bind::<Timestamp, _>(cutoff_date)
                .execute(conn)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

    info!(log, "finish"; "deleted_count" => deleted_count);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;
    use serial_test::serial;
    use std::panic::AssertUnwindSafe;

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
        let period_id = created.period_id.clone();

        let result = AssertUnwindSafe(async {
            assert_eq!(created.initial_value, initial_value);

            // DB から再取得しても一致
            let fetched = EvaluationPeriod::get_by_period_id_async(period_id.clone())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(fetched.initial_value, initial_value);
        })
        .catch_unwind()
        .await;

        // Cleanup（テスト本体がパニックしても常に実行）
        let _ = EvaluationPeriod::delete_by_period_id_async(period_id).await;

        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    /// 古い created_at を持つ evaluation_period を直接 INSERT するヘルパー
    async fn insert_with_created_at(period_id: &str, created_at: NaiveDateTime) -> Result<()> {
        use diesel::sql_types::{Numeric, Timestamp, Varchar};

        let conn = connection_pool::get().await?;
        let period_id = period_id.to_string();
        conn.interact(move |conn| {
            diesel::sql_query(
                "INSERT INTO evaluation_periods (period_id, start_time, initial_value, created_at) \
                 VALUES ($1, $2, $3, $4)",
            )
            .bind::<Varchar, _>(&period_id)
            .bind::<Timestamp, _>(created_at)
            .bind::<Numeric, _>(BigDecimal::from(0))
            .bind::<Timestamp, _>(created_at)
            .execute(conn)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;
        Ok(())
    }

    #[tokio::test]
    #[serial(evaluation_period)]
    async fn test_cleanup_old_records() {
        let now = chrono::Utc::now().naive_utc();
        let old_id = format!("eval_test_old_{}", uuid::Uuid::new_v4());
        let recent_id = format!("eval_test_recent_{}", uuid::Uuid::new_v4());

        // 400日前のレコードと10日前のレコードを作成
        insert_with_created_at(&old_id, now - chrono::TimeDelta::days(400))
            .await
            .unwrap();
        insert_with_created_at(&recent_id, now - chrono::TimeDelta::days(10))
            .await
            .unwrap();

        // 365日より古いレコードを削除
        cleanup_old_records(365).await.unwrap();

        // 古いレコードは削除、新しいレコードは残る
        let old = EvaluationPeriod::get_by_period_id_async(old_id)
            .await
            .unwrap();
        assert!(old.is_none(), "400-day-old record should be deleted");

        let recent = EvaluationPeriod::get_by_period_id_async(recent_id.clone())
            .await
            .unwrap();
        assert!(recent.is_some(), "10-day-old record should remain");

        let _ = EvaluationPeriod::delete_by_period_id_async(recent_id).await;
    }

    #[tokio::test]
    #[serial(evaluation_period)]
    async fn test_cleanup_old_records_zero_days_skips() {
        // retention_days=0 は何も削除しない
        let result = cleanup_old_records(0).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[serial(evaluation_period)]
    async fn test_cleanup_old_records_minimum_retention() {
        let now = chrono::Utc::now().naive_utc();
        let id = format!("eval_test_min_{}", uuid::Uuid::new_v4());

        // 40日前のレコード
        insert_with_created_at(&id, now - chrono::TimeDelta::days(40))
            .await
            .unwrap();

        // retention_days=1 → effective=30 (MIN_RETENTION_DAYS)
        // 40日前 > 30日前 なので削除される
        cleanup_old_records(1).await.unwrap();

        let record = EvaluationPeriod::get_by_period_id_async(id.clone())
            .await
            .unwrap();
        assert!(
            record.is_none(),
            "40-day-old record should be deleted with effective retention of 30 days"
        );
    }

    #[tokio::test]
    #[serial(evaluation_period)]
    async fn test_cleanup_cascades_to_child_tables() {
        use crate::portfolio_holding::{NewPortfolioHolding, PortfolioHolding};
        use crate::trade_transaction::TradeTransaction;

        let now = chrono::Utc::now().naive_utc();
        let period_id = format!("eval_test_cascade_{}", uuid::Uuid::new_v4());
        let old_time = now - chrono::TimeDelta::days(400);

        // 古い evaluation_period を作成
        insert_with_created_at(&period_id, old_time).await.unwrap();

        // 子テーブルにレコードを追加
        let holding = NewPortfolioHolding {
            evaluation_period_id: period_id.clone(),
            timestamp: old_time,
            token_holdings: serde_json::json!([]),
        };
        PortfolioHolding::insert_async(holding).await.unwrap();

        let tx_id = format!("test_tx_{}", uuid::Uuid::new_v4());
        let tx = TradeTransaction {
            tx_id: tx_id.clone(),
            trade_batch_id: "test_batch".to_string(),
            from_token: "wrap.near".to_string(),
            from_amount: "1000".parse().unwrap(),
            to_token: "usdt.tether-token.near".to_string(),
            to_amount: "100".parse().unwrap(),
            timestamp: old_time,
            evaluation_period_id: Some(period_id.clone()),
            actual_to_amount: None,
        };
        tx.insert_async().await.unwrap();

        // cleanup で親を削除 → CASCADE で子も消える
        cleanup_old_records(365).await.unwrap();

        // 親が消えている
        let parent = EvaluationPeriod::get_by_period_id_async(period_id.clone())
            .await
            .unwrap();
        assert!(parent.is_none(), "parent should be deleted");

        // portfolio_holdings も消えている
        let holdings = PortfolioHolding::get_by_period_async(period_id)
            .await
            .unwrap();
        assert!(
            holdings.is_empty(),
            "child portfolio_holdings should be cascade-deleted"
        );

        // trade_transaction も消えている
        let tx = TradeTransaction::find_by_tx_id_async(tx_id).await.unwrap();
        assert!(
            tx.is_none(),
            "child trade_transaction should be cascade-deleted"
        );
    }
}
