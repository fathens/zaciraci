use crate::Result;
use crate::connection_pool;
use crate::schema::token_rates;
use anyhow::anyhow;
use bigdecimal::{BigDecimal, Zero};
use chrono::NaiveDateTime;
use common::config::ConfigAccess;
use common::types::TimeRange;
use common::types::{
    ExchangeRate, TokenAccount, TokenInAccount, TokenOutAccount, TokenSmallestUnits,
};
use diesel::prelude::*;
use logging::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

/// スワップパス内の個々のプール情報
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SwapPoolInfo {
    /// プールID
    pub pool_id: u32,
    /// 入力トークンのインデックス
    pub token_in_idx: u8,
    /// 出力トークンのインデックス
    pub token_out_idx: u8,
    /// 入力側プールサイズ（smallest_units）
    pub amount_in: TokenSmallestUnits,
    /// 出力側プールサイズ（smallest_units）
    pub amount_out: TokenSmallestUnits,
}

/// スワップパス全体の情報（マルチホップ対応）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SwapPath {
    /// パス内の全プール情報
    pub pools: Vec<SwapPoolInfo>,
}

// データベース用モデル（読み込み用）
#[derive(Debug, Clone, Queryable, Selectable, QueryableByName)]
#[diesel(table_name = token_rates)]
#[diesel(check_for_backend(diesel::pg::Pg))]
struct DbTokenRate {
    #[allow(dead_code)] // Diesel Queryable でDBスキーマと一致させるため必要
    pub id: i32,
    pub base_token: String,
    pub quote_token: String,
    pub rate: BigDecimal,
    pub timestamp: NaiveDateTime,
    pub decimals: i16,
    pub rate_calc_near: i64,
    pub swap_path: Option<serde_json::Value>,
}

// データベース挿入用モデル（ExchangeRate から構築）
#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = token_rates)]
struct NewDbTokenRate {
    pub base_token: String,
    pub quote_token: String,
    pub rate: BigDecimal,
    pub timestamp: NaiveDateTime,
    pub decimals: i16,
    pub rate_calc_near: i64,
    pub swap_path: Option<serde_json::Value>,
}

impl NewDbTokenRate {
    /// ExchangeRate から挿入用モデルを作成
    fn from_exchange_rate(
        base: &TokenOutAccount,
        quote: &TokenInAccount,
        exchange_rate: &ExchangeRate,
        timestamp: NaiveDateTime,
        rate_calc_near: i64,
        swap_path: Option<&SwapPath>,
    ) -> Self {
        Self {
            base_token: base.to_string(),
            quote_token: quote.to_string(),
            rate: exchange_rate.raw_rate().clone(),
            decimals: exchange_rate.decimals() as i16,
            timestamp,
            rate_calc_near,
            swap_path: swap_path.and_then(|p| serde_json::to_value(p).ok()),
        }
    }
}

// ボラティリティ計算結果用の一時的な構造体
#[derive(Debug, Clone, QueryableByName)]
#[diesel(check_for_backend(diesel::pg::Pg))]
struct VolatilityResult {
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub base_token: String,
    #[diesel(sql_type = diesel::sql_types::Numeric)]
    pub variance: BigDecimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TokenVolatility {
    pub base: TokenAccount,
    pub variance: BigDecimal,
}

// アプリケーションロジック用モデル
#[derive(Debug, Clone)]
pub struct TokenRate {
    pub base: TokenOutAccount,
    pub quote: TokenInAccount,
    pub exchange_rate: ExchangeRate,
    pub timestamp: NaiveDateTime,
    pub rate_calc_near: i64,
    /// スワップパス情報（プールサイズを含む）
    pub swap_path: Option<SwapPath>,
}

// 相互変換の実装
impl TokenRate {
    /// 新しい TokenRate を作成（スワップパス情報付き）
    pub fn new_with_path(
        base: TokenOutAccount,
        quote: TokenInAccount,
        exchange_rate: ExchangeRate,
        rate_calc_near: i64,
        swap_path: SwapPath,
    ) -> Self {
        Self {
            base,
            quote,
            exchange_rate,
            timestamp: chrono::Utc::now().naive_utc(),
            rate_calc_near,
            swap_path: Some(swap_path),
        }
    }

    /// DB レコードからドメインモデルに変換
    fn from_db(db_rate: DbTokenRate) -> Result<Self> {
        let base = TokenAccount::from_str(&db_rate.base_token)?.into();
        let quote = TokenAccount::from_str(&db_rate.quote_token)?.into();
        let exchange_rate = ExchangeRate::from_raw_rate(db_rate.rate, db_rate.decimals as u8);
        let swap_path = db_rate
            .swap_path
            .and_then(|v| serde_json::from_value(v).ok());

        Ok(Self {
            base,
            quote,
            exchange_rate,
            timestamp: db_rate.timestamp,
            rate_calc_near: db_rate.rate_calc_near,
            swap_path,
        })
    }

    /// NewDbTokenRate に変換
    fn to_new_db(&self) -> NewDbTokenRate {
        NewDbTokenRate::from_exchange_rate(
            &self.base,
            &self.quote,
            &self.exchange_rate,
            self.timestamp,
            self.rate_calc_near,
            self.swap_path.as_ref(),
        )
    }

    // 複数レコードを一括挿入
    pub async fn batch_insert(token_rates: &[TokenRate], cfg: &impl ConfigAccess) -> Result<()> {
        let log = DEFAULT.new(o!(
            "function" => "batch_insert",
            "token_rates" => token_rates.len(),
        ));
        info!(log, "start");
        use diesel::RunQueryDsl;

        if token_rates.is_empty() {
            return Ok(());
        }

        let new_rates: Vec<NewDbTokenRate> =
            token_rates.iter().map(|rate| rate.to_new_db()).collect();

        {
            let conn = connection_pool::get().await?;

            conn.interact(move |conn| {
                diesel::insert_into(token_rates::table)
                    .values(&new_rates)
                    .execute(conn)
            })
            .await
            .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
        }

        // 古いレコードをクリーンアップ
        let retention_days = cfg.token_rates_retention_days();

        trace!(log, "cleaning up old records"; "retention_days" => retention_days);
        TokenRate::cleanup_old_records(retention_days).await?;

        info!(log, "finish");
        Ok(())
    }

    // 指定日数より古いレコードを削除
    pub async fn cleanup_old_records(retention_days: u32) -> Result<()> {
        use diesel::prelude::*;
        use diesel::sql_types::Timestamp;

        let log = DEFAULT.new(o!(
            "function" => "cleanup_old_records",
            "retention_days" => retention_days,
        ));
        trace!(log, "start");

        let conn = connection_pool::get().await?;

        // 保持期間より古いレコードを削除
        let cutoff_date =
            chrono::Utc::now().naive_utc() - chrono::Duration::days(retention_days as i64);

        let deleted_count = conn
            .interact(move |conn| {
                diesel::sql_query("DELETE FROM token_rates WHERE timestamp < $1")
                    .bind::<Timestamp, _>(cutoff_date)
                    .execute(conn)
            })
            .await
            .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

        trace!(log, "finish"; "deleted_count" => deleted_count, "cutoff_date" => %cutoff_date);
        Ok(())
    }

    /// 最新のレートを取得
    pub async fn get_latest(
        base: &TokenOutAccount,
        quote: &TokenInAccount,
    ) -> Result<Option<TokenRate>> {
        use diesel::QueryDsl;
        use diesel::dsl::max;

        let base_str = base.to_string();
        let quote_str = quote.to_string();
        let conn = connection_pool::get().await?;

        // まず最新のタイムスタンプを検索
        let latest_timestamp = conn
            .interact(move |conn| {
                token_rates::table
                    .filter(token_rates::base_token.eq(&base_str))
                    .filter(token_rates::quote_token.eq(&quote_str))
                    .select(max(token_rates::timestamp))
                    .first::<Option<NaiveDateTime>>(conn)
                    .optional()
            })
            .await
            .map_err(|e| anyhow!("Database interaction error: {:?}", e))??
            .flatten();

        // タイムスタンプが存在する場合、そのレコードを取得
        if let Some(timestamp) = latest_timestamp {
            let base_str = base.to_string();
            let quote_str = quote.to_string();
            let conn = connection_pool::get().await?;

            let result = conn
                .interact(move |conn| {
                    token_rates::table
                        .filter(token_rates::base_token.eq(&base_str))
                        .filter(token_rates::quote_token.eq(&quote_str))
                        .filter(token_rates::timestamp.eq(timestamp))
                        .first::<DbTokenRate>(conn)
                })
                .await
                .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

            Ok(Some(TokenRate::from_db(result)?))
        } else {
            Ok(None)
        }
    }

    /// 時間範囲内のレートを取得
    pub async fn get_rates_in_time_range(
        range: &TimeRange,
        base: &TokenOutAccount,
        quote: &TokenInAccount,
    ) -> Result<Vec<TokenRate>> {
        use diesel::QueryDsl;

        let conn = connection_pool::get().await?;

        let start = range.start;
        let end = range.end;
        let base_str = base.to_string();
        let quote_str = quote.to_string();

        let results = conn
            .interact(move |conn| {
                token_rates::table
                    .filter(token_rates::timestamp.gt(start))
                    .filter(token_rates::timestamp.le(end))
                    .filter(token_rates::base_token.eq(&base_str))
                    .filter(token_rates::quote_token.eq(&quote_str))
                    .order_by(token_rates::timestamp.asc())
                    .load::<DbTokenRate>(conn)
            })
            .await
            .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

        results.into_iter().map(Self::from_db).collect()
    }

    /// 複数トークンの価格履歴を一括取得
    ///
    /// N個のトークンに対して1回のDBクエリで履歴を取得し、トークンごとのHashMapとして返す。
    pub async fn get_rates_for_multiple_tokens(
        tokens: &[TokenOutAccount],
        quote: &TokenInAccount,
        range: &TimeRange,
    ) -> Result<HashMap<TokenOutAccount, Vec<TokenRate>>> {
        use diesel::sql_types::{Array, Text, Timestamp};

        if tokens.is_empty() {
            return Ok(HashMap::new());
        }

        let conn = connection_pool::get().await?;

        let tokens_vec: Vec<String> = tokens.iter().map(|t| t.to_string()).collect();
        let quote_str = quote.to_string();
        let start = range.start;
        let end = range.end;

        let results: Vec<DbTokenRate> = conn
            .interact(move |conn| {
                diesel::sql_query(
                    "SELECT id, base_token, quote_token, rate, timestamp, decimals, rate_calc_near, swap_path
                     FROM token_rates
                     WHERE base_token = ANY($1)
                       AND quote_token = $2
                       AND timestamp > $3
                       AND timestamp <= $4
                     ORDER BY base_token, timestamp ASC",
                )
                .bind::<Array<Text>, _>(&tokens_vec)
                .bind::<Text, _>(&quote_str)
                .bind::<Timestamp, _>(start)
                .bind::<Timestamp, _>(end)
                .load::<DbTokenRate>(conn)
            })
            .await
            .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

        let rates: Vec<TokenRate> = results
            .into_iter()
            .map(Self::from_db)
            .collect::<Result<_>>()?;

        // トークンごとに分割
        let mut map: HashMap<TokenOutAccount, Vec<TokenRate>> = HashMap::new();
        for rate in rates {
            map.entry(rate.base.clone()).or_default().push(rate);
        }

        Ok(map)
    }

    // ボラティリティ（変動率）の高い順にトークンペアを取得
    pub async fn get_by_volatility_in_time_range(
        range: &TimeRange,
        quote: &TokenInAccount,
    ) -> Result<Vec<TokenVolatility>> {
        let quote_str = quote.to_string();
        let range_start = range.start;
        let range_end = range.end;
        let log = DEFAULT.new(o!(
            "function" => "get_by_volatility_in_time_range",
            "quote" => quote.to_string(),
            "range.start" => range_start.to_string(),
            "range.end" => range_end.to_string(),
        ));
        trace!(log, "start");

        let conn = connection_pool::get().await?;

        // SQLクエリを実装してボラティリティを計算
        // 全トークンを variance 降順で取得（フィルタリングはアプリケーション側）
        let volatility_results: Vec<VolatilityResult> = conn
            .interact(move |conn| {
                diesel::sql_query(
                    "
                SELECT
                    base_token,
                    var_pop(rate) as variance
                FROM token_rates
                WHERE
                    quote_token = $1 AND
                    timestamp >= $2 AND
                    timestamp <= $3
                GROUP BY base_token
                HAVING
                    MIN(rate) > 0
                ORDER BY variance DESC
                ",
                )
                .bind::<diesel::sql_types::Text, _>(&quote_str)
                .bind::<diesel::sql_types::Timestamp, _>(range_start)
                .bind::<diesel::sql_types::Timestamp, _>(range_end)
                .load::<VolatilityResult>(conn)
            })
            .await
            .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

        let volatility_results: Vec<TokenVolatility> = volatility_results
            .into_iter()
            .filter_map(|result| match TokenAccount::from_str(&result.base_token) {
                Ok(token) => Some(TokenVolatility {
                    base: token,
                    variance: result.variance,
                }),
                Err(e) => {
                    error!(log, "Failed to parse token: {}, {e}", result.base_token);
                    None
                }
            })
            .collect();

        Ok(volatility_results)
    }

    /// スポットレートに補正（最初のプールを使用）
    ///
    /// スリッページの影響を除去し、スポットレートを推定する。
    /// 補正式: r_spot = r_actual × (1 + Δx / x)
    /// - Δx = rate_calc_near（入力量）
    /// - x = 入力側プールサイズ
    ///
    /// swap_path が None の場合は補正なしで元のレートを返す。
    pub fn to_spot_rate(&self) -> ExchangeRate {
        self.to_spot_rate_with_fallback(None)
    }

    /// 指定インデックスのレートに対するフォールバック swap_path を検索
    ///
    /// 「自分より新しくもっとも古い」swap_path を返す。
    /// 自身が swap_path を持つ場合、または見つからない場合は None を返す。
    ///
    /// # Arguments
    /// * `rates` - 時系列昇順（古い → 新しい）のレート配列
    /// * `index` - フォールバックを探すレートのインデックス
    ///
    /// # Note
    /// この関数は O(n) の計算量を持ち、n 個のレートに対して呼び出すと O(n²) になる。
    /// 大量のレートを処理する場合は `precompute_fallback_indices()` を使用して事前計算を行うこと。
    #[cfg(test)] // テストでのみ使用（本番は precompute_fallback_indices を使用）
    pub fn find_fallback_path(rates: &[TokenRate], index: usize) -> Option<&SwapPath> {
        let rate = rates.get(index)?;

        // 自身が swap_path を持つ場合はフォールバック不要
        if rate.swap_path.is_some() {
            return None;
        }

        // index より後ろ（自分より新しい）のレートから、最初に見つかる swap_path を返す
        rates[index + 1..].iter().find_map(|r| r.swap_path.as_ref())
    }

    /// フォールバック swap_path のインデックスを事前計算（O(n)）
    ///
    /// 各レートに対して「自分より新しくもっとも古い」swap_path を持つインデックスを返す。
    /// 自身が swap_path を持つ場合は None。
    ///
    /// # Arguments
    /// * `rates` - 時系列昇順（古い → 新しい）のレート配列
    ///
    /// # Returns
    /// 各インデックスに対応するフォールバックインデックスの配列
    pub fn precompute_fallback_indices(rates: &[TokenRate]) -> Vec<Option<usize>> {
        let mut fallbacks = vec![None; rates.len()];
        let mut last_path_idx: Option<usize> = None;

        // 新しい方から古い方へスキャン
        for i in (0..rates.len()).rev() {
            if rates[i].swap_path.is_some() {
                last_path_idx = Some(i);
                fallbacks[i] = None; // 自身が持つ場合は不要
            } else {
                fallbacks[i] = last_path_idx;
            }
        }
        fallbacks
    }

    /// スポットレートに補正（フォールバック付き、マルチホップ対応）
    ///
    /// swap_path が None の場合、提供された fallback_path を使用して補正。
    /// swap_path も fallback_path もない場合は補正なしで元のレートを返す。
    ///
    /// # AMM プライスインパクト補正モデル
    ///
    /// AMM（自動マーケットメーカー）では、スワップ量が大きいほど実効レートが
    /// スポットレート（無限小取引量での理論レート）から乖離する（プライスインパクト）。
    /// この補正は、実測のスワップレートからプライスインパクトを除去して
    /// スポットレートを推定する。
    ///
    /// ## 補正式（マルチホップ）
    ///
    /// ```text
    /// spot_rate = exchange_rate × correction
    ///
    /// correction = Π_i (1 + Δx_i / x_i)
    /// ```
    ///
    /// - `Δx_i`: ホップ i でプールに投入されたトークン量
    /// - `x_i`: ホップ i のプール内の投入トークン側リザーブ（= `amount_in`）
    /// - `Δx_0 = rate_calc_near × 10^24`（NEAR → yocto 変換）
    /// - `Δx_{i+1} = y_i × Δx_i / (x_i + Δx_i)`:
    ///   AMM 定積公式 (x·y=k) による次ホップの投入量算出
    ///
    /// ## 直感的な解釈
    ///
    /// 定積 AMM (x·y=k) では、Δx を投入すると実効レートは `y/(x+Δx)` になるが、
    /// スポットレートは `y/x`。比率は `(x+Δx)/x = 1 + Δx/x` なので、
    /// 実効レートにこの補正係数を掛けるとスポットレートが復元される。
    /// マルチホップでは各プールの補正を積算する。
    pub fn to_spot_rate_with_fallback(&self, fallback_path: Option<&SwapPath>) -> ExchangeRate {
        let path = self.swap_path.as_ref().or(fallback_path);
        if let Some(path) = path
            && !path.pools.is_empty()
        {
            // rate_calc_near は NEAR 単位で記録されているため、yocto に変換
            // (1 NEAR = 10^24 yocto)
            let delta_x = BigDecimal::from(self.rate_calc_near) * BigDecimal::from(10_u128.pow(24));

            let mut correction = BigDecimal::from(1);
            let mut current_delta = delta_x;

            for pool in &path.pools {
                let pool_amount = pool.amount_in.as_bigdecimal();
                if !pool_amount.is_zero() {
                    // 各プールで補正を積算: correction *= (1 + Δx / x)
                    correction *= (pool_amount + &current_delta) / pool_amount;

                    // 次のホップの入力量を AMM の定積公式で算出: Δx_{i+1} = y × Δx / (x + Δx)
                    let amount_out = pool.amount_out.as_bigdecimal();
                    current_delta = amount_out * &current_delta / (pool_amount + &current_delta);
                }
            }

            return self.exchange_rate.clone() * correction;
        }
        self.exchange_rate.clone()
    }

    /// Vec<TokenRate> のスポットレート一括変換。
    ///
    /// swap_path が NULL のレコードには precompute_fallback_indices で
    /// 算出したフォールバックを適用する。
    /// ExchangeRate が実質ゼロのレコードは除外する。
    pub fn to_spot_rates(rates: &[TokenRate]) -> Vec<(NaiveDateTime, ExchangeRate)> {
        let fallback_indices = Self::precompute_fallback_indices(rates);
        rates
            .iter()
            .enumerate()
            .filter_map(|(i, rate)| {
                let fallback_path = fallback_indices[i]
                    .and_then(|idx| rates.get(idx))
                    .and_then(|r| r.swap_path.as_ref());
                let spot_rate = rate.to_spot_rate_with_fallback(fallback_path);
                if spot_rate.is_effectively_zero() {
                    return None;
                }
                Some((rate.timestamp, spot_rate))
            })
            .collect()
    }

    /// 指定タイムスタンプ以前の最新スポットレートをトークンごとに取得
    ///
    /// CTE で最新レートと最新 swap_path 付きレートを同時取得し、
    /// COALESCE で swap_path をフォールバック補完した結果を返す。
    /// 各レートは swap_path 補完後に `to_spot_rate()` でスポットレートに変換。
    pub async fn get_spot_rates_at_time(
        tokens: &[TokenOutAccount],
        quote: &TokenInAccount,
        at_or_before: NaiveDateTime,
    ) -> Result<HashMap<TokenOutAccount, ExchangeRate>> {
        use diesel::sql_types::{Array, Text, Timestamp};

        if tokens.is_empty() {
            return Ok(HashMap::new());
        }

        let conn = connection_pool::get().await?;

        let tokens_vec: Vec<String> = tokens.iter().map(|t| t.to_string()).collect();
        let quote_str = quote.to_string();
        let ts = at_or_before;

        let results: Vec<DbTokenRate> = conn
            .interact(move |conn| {
                // NOTE: $1, $2, $3 are intentionally reused in both CTEs.
                // PostgreSQL allows referencing the same bind parameters multiple times.
                diesel::sql_query(
                    "WITH latest AS ( \
                         SELECT DISTINCT ON (base_token) \
                             id, base_token, quote_token, rate, timestamp, \
                             decimals, rate_calc_near, swap_path \
                         FROM token_rates \
                         WHERE base_token = ANY($1) \
                           AND quote_token = $2 \
                           AND timestamp <= $3 \
                         ORDER BY base_token, timestamp DESC \
                     ), \
                     fallback AS ( \
                         SELECT DISTINCT ON (base_token) \
                             base_token, swap_path \
                         FROM token_rates \
                         WHERE base_token = ANY($1) \
                           AND quote_token = $2 \
                           AND timestamp <= $3 \
                           AND swap_path IS NOT NULL \
                         ORDER BY base_token, timestamp DESC \
                     ) \
                     SELECT l.id, l.base_token, l.quote_token, l.rate, l.timestamp, \
                            l.decimals, l.rate_calc_near, \
                            COALESCE(l.swap_path, f.swap_path) AS swap_path \
                     FROM latest l \
                     LEFT JOIN fallback f ON l.base_token = f.base_token",
                )
                .bind::<Array<Text>, _>(&tokens_vec)
                .bind::<Text, _>(&quote_str)
                .bind::<Timestamp, _>(ts)
                .load::<DbTokenRate>(conn)
            })
            .await
            .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

        let rates: Vec<TokenRate> = results
            .into_iter()
            .map(Self::from_db)
            .collect::<Result<_>>()?;

        let mut result = HashMap::with_capacity(rates.len());
        for rate in &rates {
            result.insert(rate.base.clone(), rate.to_spot_rate());
        }

        Ok(result)
    }
}

/// DB クエリ結果用の構造体（get_all_decimals 用）
#[derive(Debug, Clone, diesel::QueryableByName)]
#[diesel(check_for_backend(diesel::pg::Pg))]
struct TokenDecimalsRow {
    #[diesel(sql_type = diesel::sql_types::Text)]
    base_token: String,
    #[diesel(sql_type = diesel::sql_types::SmallInt)]
    decimals: i16,
}

/// 全トークンの最新スポットレートを一括取得（指定 quote_token 建て）
///
/// LATERAL サブクエリで base_token ごとに最新1行だけを取得し、
/// swap_path のフォールバック補完を行った結果にスポットレート補正を適用。
/// プール流動性の NEAR 換算に使用する。
///
/// インデックス `(quote_token, base_token, timestamp DESC)` を活用し、
/// base_token ごとに Index Scan + LIMIT 1 で O(N) で完了する。
pub async fn get_all_latest_rates(
    quote_token: &TokenAccount,
) -> Result<HashMap<TokenAccount, ExchangeRate>> {
    let conn = connection_pool::get().await?;
    let quote_str = quote_token.to_string();

    let rows: Vec<DbTokenRate> = conn
        .interact(move |conn| {
            use diesel::RunQueryDsl;
            use diesel::sql_types::Text;
            diesel::sql_query(
                "SELECT l.id, l.base_token, l.quote_token, l.rate, l.timestamp, \
                        l.decimals, l.rate_calc_near, \
                        COALESCE(l.swap_path, f.swap_path) AS swap_path \
                 FROM ( \
                     SELECT DISTINCT base_token \
                     FROM token_rates \
                     WHERE quote_token = $1 \
                 ) AS tokens \
                 CROSS JOIN LATERAL ( \
                     SELECT id, base_token, quote_token, rate, timestamp, \
                            decimals, rate_calc_near, swap_path \
                     FROM token_rates \
                     WHERE quote_token = $1 \
                       AND base_token = tokens.base_token \
                     ORDER BY timestamp DESC \
                     LIMIT 1 \
                 ) AS l \
                 LEFT JOIN LATERAL ( \
                     SELECT swap_path \
                     FROM token_rates \
                     WHERE quote_token = $1 \
                       AND base_token = tokens.base_token \
                       AND swap_path IS NOT NULL \
                     ORDER BY timestamp DESC \
                     LIMIT 1 \
                 ) AS f ON true",
            )
            .bind::<Text, _>(&quote_str)
            .load::<DbTokenRate>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    let log = DEFAULT.new(o!("function" => "get_all_latest_rates"));
    let mut result = HashMap::with_capacity(rows.len());
    for row in rows {
        match TokenRate::from_db(row) {
            Ok(token_rate) => {
                let spot_rate = token_rate.to_spot_rate();
                let token: TokenAccount = token_rate.base.into();
                result.insert(token, spot_rate);
            }
            Err(e) => {
                debug!(log, "skipping unparseable token rate"; "error" => %e);
            }
        }
    }
    Ok(result)
}

/// token_rates テーブルから全トークンの最新 decimals を一括取得
pub async fn get_all_decimals() -> Result<HashMap<TokenAccount, u8>> {
    let conn = connection_pool::get().await?;

    let rows: Vec<TokenDecimalsRow> = conn
        .interact(|conn| {
            use diesel::RunQueryDsl;
            diesel::sql_query(
                "SELECT DISTINCT ON (base_token) base_token, decimals \
                 FROM token_rates \
                 ORDER BY base_token, timestamp DESC",
            )
            .load::<TokenDecimalsRow>(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    let mut result = HashMap::with_capacity(rows.len());
    for row in rows {
        if let Ok(token) = TokenAccount::from_str(&row.base_token) {
            result.insert(token, row.decimals as u8);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
