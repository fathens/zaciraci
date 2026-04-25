use super::*;
use bigdecimal::Zero;

/// テスト用ヘルパー: prediction_records テーブルの全レコードを削除
pub async fn clean_table() -> Result<()> {
    let conn = connection_pool::get().await?;
    conn.interact(|conn| diesel::delete(prediction_records::table).execute(conn))
        .await
        .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    Ok(())
}

/// テスト用ヘルパー: 評価済みレコードを挿入し返す
///
/// evaluated_at は target_time + 1h に設定される（実運用の時間関係を反映）
pub async fn insert_evaluated_record(
    token: &str,
    quote_token: &str,
    predicted_price: i64,
    actual_price: i64,
    data_cutoff_time: NaiveDateTime,
    target_time: NaiveDateTime,
) -> Result<DbPredictionRecord> {
    let new_record = NewPredictionRecord {
        token: token.to_string(),
        quote_token: quote_token.to_string(),
        predicted_price: BigDecimal::from(predicted_price),
        data_cutoff_time,
        target_time,
    };

    let actual = BigDecimal::from(actual_price);
    let predicted = BigDecimal::from(predicted_price);
    let mape = if !actual.is_zero() {
        let diff = (&predicted - &actual).abs();
        use bigdecimal::ToPrimitive;
        (diff / &actual).to_f64().unwrap_or(0.0) * 100.0
    } else {
        0.0
    };
    let absolute_error = (&predicted - &actual).abs();
    let evaluated_at = target_time + chrono::TimeDelta::hours(1);

    let conn = connection_pool::get().await?;
    let result = conn
        .interact(move |conn| {
            // 挿入
            diesel::insert_into(prediction_records::table)
                .values(&new_record)
                .execute(conn)?;

            // 挿入したレコードを取得
            let record: DbPredictionRecord = prediction_records::table
                .order_by(prediction_records::id.desc())
                .first(conn)?;

            // 評価済みに更新
            diesel::update(prediction_records::table.filter(prediction_records::id.eq(record.id)))
                .set((
                    prediction_records::actual_price.eq(&actual),
                    prediction_records::mape.eq(mape),
                    prediction_records::absolute_error.eq(&absolute_error),
                    prediction_records::evaluated_at.eq(evaluated_at),
                ))
                .execute(conn)?;

            // 更新後のレコードを取得
            prediction_records::table
                .filter(prediction_records::id.eq(record.id))
                .first::<DbPredictionRecord>(conn)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

    Ok(result)
}

/// テスト用ヘルパー: 未評価の NewPredictionRecord を挿入
///
/// `created_at` はデフォルトで `data_cutoff_time` に揃える。production では
/// `data_cutoff_time <= created_at <= target_time` の時間関係が常に成り立つため、
/// この単純化はテスト内での因果性を壊さない。`as_of`(= sim 時刻) を境にした
/// `created_at <= as_of` フィルタの単位テストにも、この既定値で十分。
pub async fn insert_unevaluated_record(
    token: &str,
    quote_token: &str,
    predicted_price: i64,
    data_cutoff_time: NaiveDateTime,
    target_time: NaiveDateTime,
) -> Result<()> {
    insert_unevaluated_record_at(
        token,
        quote_token,
        predicted_price,
        data_cutoff_time,
        target_time,
        data_cutoff_time,
    )
    .await
}

/// テスト用ヘルパー: `created_at` を明示的に指定して未評価レコードを挿入
///
/// `prediction_records.created_at` カラムは DB 既定 (`CURRENT_TIMESTAMP`) で
/// 自動設定されるため、過去の `as_of` を使うテストでは挿入直後に UPDATE して
/// 時点を揃える。data leakage シナリオ
/// (`created_at` が `as_of` より新しい予測を引かないこと) を直接検証する場合は
/// このヘルパーを使う。
pub async fn insert_unevaluated_record_at(
    token: &str,
    quote_token: &str,
    predicted_price: i64,
    data_cutoff_time: NaiveDateTime,
    target_time: NaiveDateTime,
    created_at: NaiveDateTime,
) -> Result<()> {
    let new_record = NewPredictionRecord {
        token: token.to_string(),
        quote_token: quote_token.to_string(),
        predicted_price: BigDecimal::from(predicted_price),
        data_cutoff_time,
        target_time,
    };

    let conn = connection_pool::get().await?;
    conn.interact(move |conn| {
        diesel::insert_into(prediction_records::table)
            .values(&new_record)
            .execute(conn)?;

        let id: i32 = prediction_records::table
            .order_by(prediction_records::id.desc())
            .select(prediction_records::id)
            .first(conn)?;

        diesel::update(prediction_records::table.filter(prediction_records::id.eq(id)))
            .set(prediction_records::created_at.eq(created_at))
            .execute(conn)
    })
    .await
    .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

    Ok(())
}
