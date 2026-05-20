use super::*;
use bigdecimal::Zero;

/// テスト用ヘルパー: prediction_records テーブルの全レコードを削除
pub async fn clean_table() -> Result<()> {
    let conn = connection_pool::get_test_only().await?;
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

    let conn = connection_pool::get_test_only().await?;
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

    let conn = connection_pool::get_test_only().await?;
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
/// 動作 (`conn.transaction` 内で atomic に実行):
/// 1. `SET LOCAL lock_timeout = '5s'` で ACCESS EXCLUSIVE lock 取得失敗時の hang を防ぐ
/// 2. `ALTER TABLE ... DROP CONSTRAINT IF EXISTS ...` で CHECK 剥がし
/// 3. `new_unchecked` で違反行を INSERT
/// 4. `ALTER TABLE ... ADD CONSTRAINT ... CHECK (...) NOT VALID` で再度 attach
///
/// # 不変条件
///
/// - **テスト DB のみで動作**: [`connection_pool::get_test_only`] 経由で
///   `current_database()` が `postgres_test` であることを実行時に検証する。
///   本番 DB に対する `DATABASE_URL` 誤設定下での実行を構造的に阻止する
///   (CHECK 制約の永続的 NOT VALID 降格 = Layer 3 防御の永続消失を防ぐ)。
/// - **トランザクション必須**: 3 statement を `conn.transaction` で wrap し、
///   INSERT が型不一致 / FK / NOT NULL 違反等で失敗した場合に Layer 3 の CHECK が
///   永続消失して後続テストが Layer 4 pin を偽通過する経路を塞ぐ。
/// - **呼び出し元 `#[serial]` 必須**: 本ヘルパは ALTER TABLE で DB スキーマを
///   一時操作するため、並列テストで他テストの INSERT/UPDATE と race するのを
///   `serial_test::serial` で抑止する前提。
/// - **テスト末尾で `restore_layer3_check_validity()` 必須**: 本ヘルパ実行後の
///   CHECK は `NOT VALID` 状態 (既存違反行は許容、新規 INSERT/UPDATE には enforce)
///   になる。テスト DB を共有する後続テストで Layer 3 防御が validated 状態である
///   ことを期待するテストが偽通過しないよう、`clean_table()` で違反行を消去した
///   後に [`restore_layer3_check_validity`] で `VALIDATED` 状態に戻すこと。
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

    let conn = connection_pool::get_test_only().await?;
    let drop_sql = format!(
        "ALTER TABLE prediction_records DROP CONSTRAINT IF EXISTS {}",
        CREATED_AT_GEQ_DATA_CUTOFF_CONSTRAINT
    );
    let add_sql = format!(
        "ALTER TABLE prediction_records ADD CONSTRAINT {} \
         CHECK (created_at >= data_cutoff_time) NOT VALID",
        CREATED_AT_GEQ_DATA_CUTOFF_CONSTRAINT
    );
    conn.interact(move |conn| {
        conn.transaction(|conn| {
            diesel::sql_query("SET LOCAL lock_timeout = '5s'").execute(conn)?;
            diesel::sql_query(&drop_sql).execute(conn)?;
            diesel::insert_into(prediction_records::table)
                .values(&new_record)
                .execute(conn)?;
            diesel::sql_query(&add_sql).execute(conn)?;
            Ok::<_, diesel::result::Error>(())
        })
    })
    .await
    .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

    Ok(())
}

/// テスト用ヘルパー: Layer 3 CHECK 制約を `NOT VALID` から再 `VALIDATED` 状態に戻す。
///
/// [`insert_data_leakage_violator`] が走ると CHECK 制約は `NOT VALID` 状態になり
/// (既存違反行は許容、新規 INSERT/UPDATE には enforce)、テスト DB を共有する後続の
/// 全テストにわたって migration 直後の `VALIDATED` 状態が永続的に失われる。
///
/// 本ヘルパは:
/// 1. `clean_table()` 等で違反行を消去した後に
/// 2. `ALTER TABLE ... VALIDATE CONSTRAINT` を呼んで Layer 3 を再 validated に戻す
///
/// `insert_data_leakage_violator` を使った各テストは末尾で本ヘルパを呼ぶこと。
/// テスト DB の制約状態を migration 直後と同じ `VALIDATED` に戻すことで、後続の
/// Layer 3 直接検証テスト (`new_unchecked_with_data_leakage_is_rejected_by_check_constraint`
/// 等) を偽通過させない。
///
/// `VALIDATE CONSTRAINT` は既存行を full scan するが、`prediction_records` は
/// 小規模 (low thousands) なため lock 取得時間は無視できる。
pub async fn restore_layer3_check_validity() -> Result<()> {
    let conn = connection_pool::get_test_only().await?;
    let validate_sql = format!(
        "ALTER TABLE prediction_records VALIDATE CONSTRAINT {}",
        CREATED_AT_GEQ_DATA_CUTOFF_CONSTRAINT
    );
    conn.interact(move |conn| {
        diesel::sql_query("SET LOCAL lock_timeout = '5s'").execute(conn)?;
        diesel::sql_query(&validate_sql).execute(conn)?;
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

    let conn = connection_pool::get_test_only().await?;
    conn.interact(move |conn| {
        diesel::insert_into(prediction_records::table)
            .values(&new_record)
            .execute(conn)
    })
    .await
    .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

    Ok(())
}
