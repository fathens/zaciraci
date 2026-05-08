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
    let new_record = NewPredictionRecord::try_new(
        token.to_string(),
        quote_token.to_string(),
        BigDecimal::from(predicted_price),
        data_cutoff_time,
        target_time,
        data_cutoff_time,
    )?;

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

/// テスト用ヘルパー: `NewPredictionRecord::try_new` の caller-side assertion を
/// バイパスして「壊れた」レコードを直接 DB に書き込む (テスト専用、persistence
/// crate 内のみで利用可能)。
///
/// SQL レイヤの fresh-prediction filter を直接検証するため、本来 caller-side で
/// 弾かれるはずのレコードをあえて DB に投入する必要があるテスト専用。新規
/// production caller は必ず [`NewPredictionRecord::try_new`] 経由で構築すること
/// (フィールドは完全 private、bypass は `#[cfg(test)] new_unchecked` のみ)。
///
/// # 制約 (Layer 3 DB CHECK 制約との関係)
///
/// `prediction_records` テーブルには `created_at >= data_cutoff_time` の
/// validated CHECK 制約が migration で追加されている。本ヘルパーは
/// `new_unchecked` で caller-side 検証を bypass できるが、**`created_at <
/// data_cutoff_time` 系違反は DB レイヤ (Layer 3) で弾かれて INSERT が失敗する**。
/// 本ヘルパーで挿入できる違反パターンは `target_time <= created_at`
/// (= horizon 系違反) のみ。
pub async fn insert_invariant_violating_record(
    token: &str,
    quote_token: &str,
    predicted_price: i64,
    data_cutoff_time: NaiveDateTime,
    target_time: NaiveDateTime,
    created_at: NaiveDateTime,
) -> Result<()> {
    // `target_time <= created_at` 系の違反をあえて作るため、`new_unchecked` で
    // `try_new` の Layer 1 検証をバイパスする (`#[cfg(test)]` なので release では
    // 消滅し persistence crate のテストに閉じる)。`created_at < data_cutoff_time`
    // 系は Layer 3 (DB CHECK) で別途弾かれる。
    let new_record = NewPredictionRecord::new_unchecked(
        token.to_string(),
        quote_token.to_string(),
        BigDecimal::from(predicted_price),
        data_cutoff_time,
        target_time,
        created_at,
    );

    let conn = connection_pool::get().await?;
    conn.interact(move |conn| {
        diesel::insert_into(prediction_records::table)
            .values(&new_record)
            .execute(conn)
    })
    .await
    .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

    Ok(())
}

/// テスト用ヘルパー: `created_at` を明示的に指定して未評価レコードを挿入
///
/// data leakage シナリオ (`created_at` が `as_of` より新しい予測を引かないこと)
/// を直接検証する場合に使う。
pub async fn insert_unevaluated_record_at(
    token: &str,
    quote_token: &str,
    predicted_price: i64,
    data_cutoff_time: NaiveDateTime,
    target_time: NaiveDateTime,
    created_at: NaiveDateTime,
) -> Result<()> {
    let new_record = NewPredictionRecord::try_new(
        token.to_string(),
        quote_token.to_string(),
        BigDecimal::from(predicted_price),
        data_cutoff_time,
        target_time,
        created_at,
    )?;

    let conn = connection_pool::get().await?;
    conn.interact(move |conn| {
        diesel::insert_into(prediction_records::table)
            .values(&new_record)
            .execute(conn)
    })
    .await
    .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

    Ok(())
}
