use crate::connection_pool;
use crate::schema::prediction_records;
use anyhow::Result;
use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use common::types::{TokenAccount, TokenOutAccount};
use diesel::prelude::*;
use logging::*;

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = prediction_records)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[allow(dead_code)] // Diesel Queryable でDBスキーマと一致させるため必要
pub struct DbPredictionRecord {
    pub id: i32,
    pub token: String,
    pub quote_token: String,
    pub predicted_price: BigDecimal,
    pub data_cutoff_time: NaiveDateTime,
    pub target_time: NaiveDateTime,
    pub actual_price: Option<BigDecimal>,
    pub mape: Option<f64>,
    pub absolute_error: Option<BigDecimal>,
    pub evaluated_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
}

/// 予測レコード挿入用の値型。
///
/// # Caller responsibility
///
/// `created_at` は **monotonic な現在時刻** (= 予測を生成した瞬間のドメイン時刻)
/// でなければならない。`created_at` は engine の "fresh prediction" 判定
/// ([`PredictionRecord::earliest_fresh_visible_in`] / [`PredictionRecord::get_latest_fresh_predictions`])
/// で domain time として消費される。呼び出し側のコンテキスト別に:
///
/// - production: cron tick の `chrono::Utc::now().naive_utc()`
/// - シミュレーション: 当該シム時刻 (`sim_day`)
///
/// を渡すことで、両経路で「production 着信時刻 ≒ created_at」のセマンティクスが
/// 保たれる。
///
/// # Data leakage paths と防御階層
///
/// `created_at` の設定を誤ると以下の data leakage が発生し、金融的に致命的:
///
/// - `created_at` が **未来日付** (= `data_cutoff_time` より進みすぎている) →
///   バックテストで `as_of` 以後に作成された予測が "as_of 時点で既に visible" と
///   して選ばれ、optimizer が未来情報を学習してしまう経路。
/// - `created_at` が **過去日付** (= `data_cutoff_time` 以前) → 「データ取得時刻
///   より古い予測」として扱われ、本来 `as_of` 時点では存在しなかった予測を fresh
///   と誤認する経路。
///
/// 防御は以下の 4 階層で行う:
///
/// - **Layer 1** (Rust smart constructor): [`NewPredictionRecord::try_new`] が
///   `Result<Self, NewPredictionRecordError>` を返し、不変条件違反を runtime で
///   fail-soft に通知する (caller side で warn ログ + skip)。NTP step backward
///   等の環境起因 violation を crash loop 化させない。**正規 INSERT 経路の
///   開発時 + 運用時防御**。
/// - **Layer 2** (フィールド可視性): フィールドを完全に private にし、
///   `try_new` を唯一の構築経路に強制する。テスト専用の bypass は
///   `#[cfg(test)] pub(crate) fn new_unchecked` でのみ可能で、release ビルドでは
///   構築経路が完全消滅する。**コンパイル時防御**。
/// - **Layer 3** (DB CHECK 制約): PostgreSQL の `created_at_geq_data_cutoff`
///   CHECK 制約が全 INSERT/UPDATE 経路 (Diesel / raw SQL / psql 直接 / DBA 操作 /
///   migration backfill) を強制カバーする。**production の唯一の包括的防御線**。
///   現行 migration (`2026-05-07-000000_add_prediction_invariant_check`) は
///   validated CHECK として一括導入される (preflight.sql で違反行を triage 後、
///   up.sql で `ADD CONSTRAINT ... CHECK`)。テーブル成長時には NOT VALID + 別
///   migration で VALIDATE への再分割を follow-up で予定。
/// - **Layer 4** (SQL fresh-prediction filter): [`PredictionRecord::earliest_fresh_visible_in`] /
///   [`PredictionRecord::get_latest_fresh_predictions`] が `created_at >= data_cutoff_time`
///   と `target_time > created_at` を read 時に強制する **read-time defense-in-depth**。
///   Layer 3 が現行で validated でも、(a) `down.sql` で CHECK を drop した直後の
///   rollback 期間、(b) 将来テーブル成長で NOT VALID 経路を採用した場合の移行期間、
///   (c) DBA による直接操作や raw SQL での意図しない bypass、(d) migration 前
///   レガシーデータが残っている経路、で違反行が DB に混入する窓を本フィルタで補う。
///
/// 不変条件 `created_at >= data_cutoff_time` は production / simulation の両経路で
/// 常に成立する (predict は cutoff 以後に走るため)。
///
/// もう一つの不変条件 `target_time > data_cutoff_time` (= prediction horizon > 0)
/// は horizon 0 以下の壊れた予測を caller-side で弾くためのもの。SQL
/// fresh-prediction filter ([`PredictionRecord::earliest_fresh_visible_in`]) は
/// `target_time > created_at` というより厳格な条件を要求するが、production では
/// stale data (24h 以上古いデータ) からの予測で `target_time < created_at` が
/// 成立し得る (現行設計では SQL filter で除外する)。caller-side では horizon
/// 正値のみを必須条件とし、SQL filter 側の判定に委ねる。
#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = prediction_records)]
pub struct NewPredictionRecord {
    // Layer 2 (visibility): フィールドを完全 private にして、production / test
    // のいずれの経路からも struct literal による構築を不能にする。
    // production の唯一の構築経路は `try_new`、test の bypass 経路は
    // `#[cfg(test)] pub(crate) fn new_unchecked` のみ。release ビルドでは
    // unchecked factory が消えるため、`try_new` の Layer 1 検証を bypass する
    // 経路が**コンパイル時に存在しなくなる**。
    token: String,
    quote_token: String,
    predicted_price: BigDecimal,
    data_cutoff_time: NaiveDateTime,
    target_time: NaiveDateTime,
    created_at: NaiveDateTime,
}

/// [`NewPredictionRecord::try_new`] の構築失敗バリアント。
///
/// caller 側で `match` または `.ok()` でハンドルし、失敗 record は `error!` ログと
/// skip で fail-soft に処理する運用想定 (data leakage 経路を crash loop 化させない)。
///
/// `Display` 出力は構造化されたフィールド値のみで、攻撃者制御の任意文字列を含まない
/// ため log forwarding 経由の漏洩リスクなし。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NewPredictionRecordError {
    /// `created_at < data_cutoff_time` (= 「未来データを使った過去予測」) は data
    /// leakage 経路。バックテストで未来情報を学習する原因となる。
    #[error(
        "created_at ({created_at}) must be >= data_cutoff_time ({data_cutoff_time}); \
         data-leakage path (see NewPredictionRecord docstring)"
    )]
    CreatedAtBeforeCutoff {
        created_at: NaiveDateTime,
        data_cutoff_time: NaiveDateTime,
    },
    /// `target_time <= data_cutoff_time` (= prediction horizon ≤ 0) は壊れた
    /// 予測レコード。
    #[error(
        "target_time ({target_time}) must be > data_cutoff_time ({data_cutoff_time}); \
         prediction horizon must be positive"
    )]
    NonPositiveHorizon {
        target_time: NaiveDateTime,
        data_cutoff_time: NaiveDateTime,
    },
}

impl NewPredictionRecord {
    /// 予測レコード挿入用の値を構築する (唯一の構築経路)。
    ///
    /// `created_at >= data_cutoff_time` および `target_time > data_cutoff_time` を
    /// runtime で検証し、違反時は [`NewPredictionRecordError`] を返す。caller は
    /// `Err` を warn/error ログ + skip で処理し、process は継続させる (NTP step
    /// backward 等の環境起因 violation で crash loop 化させない)。
    ///
    /// 不変条件と防御階層の詳細は型レベルの docstring を参照。
    // Layer 1 (smart constructor, runtime fail-soft): 不変条件違反を
    // Result で通知し、caller は warn ログ + skip で fail-soft 処理する。
    // NTP step backward 等の環境起因 violation で crash loop 化させない。
    pub fn try_new(
        token: String,
        quote_token: String,
        predicted_price: BigDecimal,
        data_cutoff_time: NaiveDateTime,
        target_time: NaiveDateTime,
        created_at: NaiveDateTime,
    ) -> Result<Self, NewPredictionRecordError> {
        if created_at < data_cutoff_time {
            return Err(NewPredictionRecordError::CreatedAtBeforeCutoff {
                created_at,
                data_cutoff_time,
            });
        }
        if target_time <= data_cutoff_time {
            return Err(NewPredictionRecordError::NonPositiveHorizon {
                target_time,
                data_cutoff_time,
            });
        }
        Ok(Self {
            token,
            quote_token,
            predicted_price,
            data_cutoff_time,
            target_time,
            created_at,
        })
    }

    /// 検証/テスト用に data_cutoff_time を公開する read-only accessor。
    pub fn data_cutoff_time(&self) -> NaiveDateTime {
        self.data_cutoff_time
    }

    /// 検証/テスト用に target_time を公開する read-only accessor。
    pub fn target_time(&self) -> NaiveDateTime {
        self.target_time
    }

    /// 検証/テスト用に created_at を公開する read-only accessor。
    pub fn created_at(&self) -> NaiveDateTime {
        self.created_at
    }

    /// テスト専用の Layer 1 bypass 構築経路。
    ///
    /// SQL レイヤの fresh-prediction filter (Layer 4) を直接検証するため、本来
    /// `try_new` で弾かれるはずのレコードをあえて DB に投入するテストでのみ使う。
    /// `#[cfg(test)]` により release ビルドでは消滅し、production 経路は `try_new`
    /// に一本化される。
    ///
    /// 不変条件 `created_at >= data_cutoff_time` 系違反は DB レイヤ (Layer 3 の
    /// validated CHECK 制約) で弾かれて INSERT が失敗するため、本 factory で
    /// 挿入できる違反パターンは `target_time <= created_at` (= horizon 系違反)
    /// のみ。
    #[cfg(test)]
    pub(crate) fn new_unchecked(
        token: String,
        quote_token: String,
        predicted_price: BigDecimal,
        data_cutoff_time: NaiveDateTime,
        target_time: NaiveDateTime,
        created_at: NaiveDateTime,
    ) -> Self {
        Self {
            token,
            quote_token,
            predicted_price,
            data_cutoff_time,
            target_time,
            created_at,
        }
    }
}

pub struct PredictionRecord;

#[cfg(test)]
mod tests;

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
        Self::get_pending_evaluations_as_of(chrono::Utc::now().naive_utc()).await
    }

    /// 指定時刻以前に target_time が到来した未評価レコードを取得する。
    pub async fn get_pending_evaluations_as_of(
        as_of: NaiveDateTime,
    ) -> Result<Vec<DbPredictionRecord>> {
        let conn = connection_pool::get().await?;

        let results = conn
            .interact(move |conn| {
                prediction_records::table
                    .filter(prediction_records::evaluated_at.is_null())
                    .filter(prediction_records::target_time.le(as_of))
                    .order_by(prediction_records::target_time.asc())
                    .load::<DbPredictionRecord>(conn)
            })
            .await
            .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

        Ok(results)
    }

    /// 指定された target_time 範囲の予測レコードを削除する。
    /// シミュレーション用の予測再生成時に使用。
    pub async fn delete_by_target_time_range(
        start: NaiveDateTime,
        end: NaiveDateTime,
    ) -> Result<usize> {
        if start > end {
            anyhow::bail!("invalid range: start ({}) > end ({})", start, end);
        }

        let conn = connection_pool::get().await?;

        let deleted = conn
            .interact(move |conn| {
                diesel::delete(
                    prediction_records::table
                        .filter(prediction_records::target_time.ge(start))
                        .filter(prediction_records::target_time.le(end)),
                )
                .execute(conn)
            })
            .await
            .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

        Ok(deleted)
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

    /// 指定トークン群の直近評価済みレコードを一括取得
    pub async fn get_recent_evaluated_for_tokens(
        limit: i64,
        tokens: &[TokenOutAccount],
    ) -> Result<Vec<DbPredictionRecord>> {
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        let tokens: Vec<String> = tokens.iter().map(|t| t.to_string()).collect();
        let conn = connection_pool::get().await?;
        let results = conn
            .interact(move |conn| {
                prediction_records::table
                    .filter(prediction_records::evaluated_at.is_not_null())
                    .filter(prediction_records::token.eq_any(&tokens))
                    .order_by(prediction_records::target_time.desc())
                    .limit(limit)
                    .load::<DbPredictionRecord>(conn)
            })
            .await
            .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;
        Ok(results)
    }

    /// 同一トークンの直前の評価済みレコードを取得
    pub async fn get_previous_evaluated(
        token: &TokenAccount,
        before_target_time: NaiveDateTime,
    ) -> Result<Option<DbPredictionRecord>> {
        let conn = connection_pool::get().await?;
        let token = token.to_string();

        let result = conn
            .interact(move |conn| {
                prediction_records::table
                    .filter(prediction_records::token.eq(&token))
                    .filter(prediction_records::evaluated_at.is_not_null())
                    .filter(prediction_records::target_time.lt(before_target_time))
                    .order_by(prediction_records::target_time.desc())
                    .first::<DbPredictionRecord>(conn)
                    .optional()
            })
            .await
            .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

        Ok(result)
    }

    /// 指定トークンの最新予測を取得（target_time が未来のもののみ）
    ///
    /// 各トークンについて、以下の条件をすべて満たすレコードを 1 件返す:
    /// - `created_at <= as_of` (= `as_of` 時点で既に DB に存在していた)
    /// - `target_time > as_of` (= `as_of` から見て未来予測)
    /// - `created_at >= data_cutoff_time` (= 予測がカットオフ後に生成された)
    ///
    /// 最新性は `target_time` 降順、同一なら `data_cutoff_time` 降順で決まる。
    ///
    /// # フィルタの根拠
    ///
    /// `created_at <= as_of`: production では `as_of = NOW` であり、未来に作成される
    /// レコードは存在しないため、このフィルタは no-op として作用する。一方シミュレーション
    /// (`as_of` = 過去のシム日付) では、フィルタなしだと `as_of` 以後に生成された予測が
    /// 選択され、因果性違反 (data leakage) が発生する。`created_at` で時点を切ることで、
    /// production とバックテストで同一の意味論を保証する。
    ///
    /// `created_at >= data_cutoff_time`: Layer 4 の read-time defense-in-depth。Layer 3
    /// の DB CHECK 制約 (`created_at_geq_data_cutoff`) は現行 migration では validated
    /// で一括導入されるが、`down.sql` で CHECK が drop された rollback 期間、将来
    /// テーブル成長で NOT VALID + VALIDATE 再分割を採用した場合の移行期間、DBA 直接
    /// 操作や raw SQL での bypass、migration 前レガシーデータ等で違反行が DB に残った
    /// 場合に、read 時点で除外して optimizer が「データ取得時刻より古い予測」を fresh と
    /// 誤認する経路を塞ぐ。
    // Layer 4 (read-time defense-in-depth): Layers 1-3 を bypass された
    // 違反行 (down.sql rollback 期間 / 将来の NOT VALID 再採用期間 / DBA
    // 直接 INSERT / raw SQL bypass / migration 前レガシーデータ等) が DB に
    // 残った場合に、read 時点で除外して optimizer が誤った fresh 予測を
    // 学習しないようにする。production の唯一の包括的防御は Layer 3 だが、
    // 本フィルタは読み取り経路に追加された防御線。
    pub async fn get_latest_fresh_predictions(
        tokens: &[TokenOutAccount],
        as_of: NaiveDateTime,
    ) -> Result<Vec<DbPredictionRecord>> {
        if tokens.is_empty() {
            return Ok(Vec::new());
        }

        let tokens: Vec<String> = tokens.iter().map(|t| t.to_string()).collect();
        let conn = connection_pool::get().await?;

        let results = conn
            .interact(move |conn| {
                prediction_records::table
                    .filter(prediction_records::token.eq_any(&tokens))
                    .filter(prediction_records::created_at.le(as_of))
                    .filter(prediction_records::target_time.gt(as_of))
                    .filter(prediction_records::created_at.ge(prediction_records::data_cutoff_time))
                    .distinct_on(prediction_records::token)
                    .order_by((
                        prediction_records::token,
                        prediction_records::target_time.desc(),
                        prediction_records::data_cutoff_time.desc(),
                        prediction_records::id.desc(),
                    ))
                    .load::<DbPredictionRecord>(conn)
            })
            .await
            .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

        Ok(results)
    }

    /// 指定区間 [`since`, `until`) の中で「fresh prediction が初めて visible になった瞬間」を返す。
    ///
    /// "fresh" の定義は [`get_latest_fresh_predictions`] と同じ:
    /// `created_at <= t` かつ `target_time > t` を満たすレコードが少なくとも 1 件存在する瞬間 `t`。
    /// 加えて Layer 4 defense-in-depth として `created_at >= data_cutoff_time` も要求する
    /// (詳細は [`get_latest_fresh_predictions`] の docstring 参照)。
    ///
    /// 探索範囲を区間内のレコードの `created_at` に限定して `MIN(created_at)` を返す。
    /// 区間内に 1 件もそういうレコードが無ければ `None`。
    ///
    /// シミュレーションでは「production の cron が起動したものの予測が未着で失敗 →
    /// 予測が DB に着いた瞬間にトレード可能になる」というタイミングを再現するために使う。
    /// `since = date midnight`, `until = (date+1) midnight` を渡すと「その日の最初の
    /// fresh prediction 時刻」が得られる。
    // Layer 4 (read-time defense-in-depth): get_latest_fresh_predictions と
    // 同じく、Layers 1-3 を bypass された違反行を read 時に除外する。
    pub async fn earliest_fresh_visible_in(
        since: NaiveDateTime,
        until: NaiveDateTime,
    ) -> Result<Option<NaiveDateTime>> {
        if since >= until {
            return Ok(None);
        }

        let conn = connection_pool::get().await?;

        let result: Option<NaiveDateTime> = conn
            .interact(move |conn| {
                prediction_records::table
                    .filter(prediction_records::created_at.ge(since))
                    .filter(prediction_records::created_at.lt(until))
                    .filter(prediction_records::target_time.gt(prediction_records::created_at))
                    .filter(prediction_records::created_at.ge(prediction_records::data_cutoff_time))
                    .select(diesel::dsl::min(prediction_records::created_at))
                    .first::<Option<NaiveDateTime>>(conn)
            })
            .await
            .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

        Ok(result)
    }

    /// Minimum retention period to prevent accidental mass deletion
    const MIN_RETENTION_DAYS: u32 = 7;

    /// 古いレコードを削除
    ///
    /// - 評価済みレコード: evaluated_at から retention_days 日以上経過したもの
    /// - 未評価レコード: target_time から unevaluated_retention_days 日以上経過したもの
    ///
    /// 戻り値: (評価済み削除数, 未評価削除数)
    pub async fn delete_old_records(
        retention_days: u32,
        unevaluated_retention_days: u32,
    ) -> Result<(usize, usize)> {
        let log = DEFAULT.new(o!(
            "function" => "prediction_record::delete_old_records",
            "retention_days" => retention_days,
            "unevaluated_retention_days" => unevaluated_retention_days,
        ));

        if retention_days == 0 && unevaluated_retention_days == 0 {
            warn!(
                log,
                "both retention_days are 0, skipping cleanup to prevent deleting all records"
            );
            return Ok((0, 0));
        }

        let effective_retention = retention_days.max(Self::MIN_RETENTION_DAYS);
        if effective_retention != retention_days {
            warn!(log, "retention_days below minimum, using minimum";
                "requested" => retention_days, "effective" => effective_retention);
        }

        let effective_unevaluated = unevaluated_retention_days.max(Self::MIN_RETENTION_DAYS);
        if effective_unevaluated != unevaluated_retention_days {
            warn!(log, "unevaluated_retention_days below minimum, using minimum";
                "requested" => unevaluated_retention_days, "effective" => effective_unevaluated);
        }

        trace!(log, "start");

        let now = chrono::Utc::now().naive_utc();
        let evaluated_cutoff = now - chrono::TimeDelta::days(i64::from(effective_retention));
        let unevaluated_cutoff = now - chrono::TimeDelta::days(i64::from(effective_unevaluated));

        let conn = connection_pool::get().await?;

        let (evaluated_deleted, unevaluated_deleted) = conn
            .interact(move |conn| {
                // 評価済みの古いレコードを削除
                let evaluated_deleted = diesel::delete(
                    prediction_records::table
                        .filter(prediction_records::evaluated_at.is_not_null())
                        .filter(prediction_records::evaluated_at.lt(evaluated_cutoff)),
                )
                .execute(conn)?;

                // 未評価で target_time が古いレコードを削除（評価できなかったもの）
                let unevaluated_deleted = diesel::delete(
                    prediction_records::table
                        .filter(prediction_records::evaluated_at.is_null())
                        .filter(prediction_records::target_time.lt(unevaluated_cutoff)),
                )
                .execute(conn)?;

                Ok::<_, diesel::result::Error>((evaluated_deleted, unevaluated_deleted))
            })
            .await
            .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

        info!(log, "finish";
            "evaluated_deleted" => evaluated_deleted,
            "unevaluated_deleted" => unevaluated_deleted);
        Ok((evaluated_deleted, unevaluated_deleted))
    }
}
