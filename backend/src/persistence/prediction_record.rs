use crate::persistence::connection_pool;
use crate::persistence::schema::prediction_records;
use anyhow::Result;
use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use diesel::prelude::*;

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = prediction_records)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[allow(dead_code)] // Diesel Queryable でDBスキーマと一致させるため必要
pub struct DbPredictionRecord {
    pub id: i32,
    pub evaluation_period_id: String,
    pub token: String,
    pub quote_token: String,
    pub predicted_price: BigDecimal,
    pub prediction_time: NaiveDateTime,
    pub target_time: NaiveDateTime,
    pub actual_price: Option<BigDecimal>,
    pub mape: Option<f64>,
    pub absolute_error: Option<BigDecimal>,
    pub evaluated_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = prediction_records)]
pub struct NewPredictionRecord {
    pub evaluation_period_id: String,
    pub token: String,
    pub quote_token: String,
    pub predicted_price: BigDecimal,
    pub prediction_time: NaiveDateTime,
    pub target_time: NaiveDateTime,
}

pub struct PredictionRecord;

impl PredictionRecord {
    /// 予測バッチ挿入
    pub async fn batch_insert(records: &[NewPredictionRecord]) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }

        let records = records.to_vec();
        let conn = connection_pool::get().await?;

        conn.interact(move |conn| {
            diesel::insert_into(prediction_records::table)
                .values(&records)
                .execute(conn)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

        Ok(())
    }

    /// 未評価 & target_time 経過済みのレコード取得
    pub async fn get_pending_evaluations() -> Result<Vec<DbPredictionRecord>> {
        let conn = connection_pool::get().await?;
        let now = chrono::Utc::now().naive_utc();

        let results = conn
            .interact(move |conn| {
                prediction_records::table
                    .filter(prediction_records::evaluated_at.is_null())
                    .filter(prediction_records::target_time.le(now))
                    .order_by(prediction_records::target_time.asc())
                    .load::<DbPredictionRecord>(conn)
            })
            .await
            .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

        Ok(results)
    }

    /// 評価結果で更新
    pub async fn update_evaluation(
        id: i32,
        actual_price: BigDecimal,
        mape: f64,
        absolute_error: BigDecimal,
    ) -> Result<()> {
        let conn = connection_pool::get().await?;
        let now = chrono::Utc::now().naive_utc();

        conn.interact(move |conn| {
            diesel::update(prediction_records::table.filter(prediction_records::id.eq(id)))
                .set((
                    prediction_records::actual_price.eq(actual_price),
                    prediction_records::mape.eq(mape),
                    prediction_records::absolute_error.eq(absolute_error),
                    prediction_records::evaluated_at.eq(now),
                ))
                .execute(conn)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

        Ok(())
    }

    /// 直近 N 件の評価済みレコード取得
    pub async fn get_recent_evaluated(limit: i64) -> Result<Vec<DbPredictionRecord>> {
        let conn = connection_pool::get().await?;

        let results = conn
            .interact(move |conn| {
                prediction_records::table
                    .filter(prediction_records::evaluated_at.is_not_null())
                    .order_by(prediction_records::evaluated_at.desc())
                    .limit(limit)
                    .load::<DbPredictionRecord>(conn)
            })
            .await
            .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

        Ok(results)
    }
}
