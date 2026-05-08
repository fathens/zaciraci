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

/// テスト用ヘルパー: Layer 3 (DB CHECK) を一時的に剥がして data leakage 違反行を
/// 強制的に持ち込む。
///
/// `created_at < data_cutoff_time` 系の違反は通常 Layer 3 の
/// `created_at_geq_data_cutoff` CHECK 制約で reject されるため、
/// `new_unchecked` でも INSERT できない。本ヘルパは Layer 4 (read-time SQL filter)
/// が以下の運用シナリオで違反行を読み飛ばすことを検証するために用意する:
///
/// - `down.sql` で CHECK が drop された rollback 期間
/// - 将来テーブル成長で `NOT VALID` + `VALIDATE` 二段移行を採用した場合の移行期間
/// - DBA 直接 INSERT / raw SQL bypass / migration 前レガシーデータ
///
/// 動作:
/// 1. `ALTER TABLE ... DROP CONSTRAINT created_at_geq_data_cutoff` で CHECK 剥がし
/// 2. `new_unchecked` で違反行を INSERT
/// 3. `ALTER TABLE ... ADD CONSTRAINT ... CHECK (...) NOT VALID` で再度 attach
///    (`NOT VALID` は既存違反行を許容しつつ以後の INSERT/UPDATE には CHECK を
///    効かせるため、以降のテストでも fail-loud な防御線が維持される)
///
/// 後始末は `clean_table()` で違反行を消したあとで残った CHECK が
/// `VALIDATE CONSTRAINT` 不要のまま自然に維持される (NOT VALID でも
/// PostgreSQL は新規 INSERT に対してチェックする)。
pub async fn insert_data_leakage_violator(
    token: &str,
    quote_token: &str,
    predicted_price: i64,
    data_cutoff_time: NaiveDateTime,
    target_time: NaiveDateTime,
    created_at: NaiveDateTime,
) -> Result<()> {
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
        diesel::sql_query(
            "ALTER TABLE prediction_records DROP CONSTRAINT IF EXISTS created_at_geq_data_cutoff",
        )
        .execute(conn)?;
        diesel::insert_into(prediction_records::table)
            .values(&new_record)
            .execute(conn)?;
        diesel::sql_query(
            "ALTER TABLE prediction_records \
             ADD CONSTRAINT created_at_geq_data_cutoff \
             CHECK (created_at >= data_cutoff_time) NOT VALID",
        )
        .execute(conn)?;
        Ok::<_, diesel::result::Error>(())
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
