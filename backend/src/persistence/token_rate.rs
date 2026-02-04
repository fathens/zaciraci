use crate::Result;
use crate::logging::*;
use crate::persistence::TimeRange;
use crate::persistence::connection_pool;
use crate::persistence::schema::token_rates;
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use anyhow::anyhow;
use bigdecimal::{BigDecimal, Zero};
use chrono::NaiveDateTime;
use diesel::prelude::*;
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use zaciraci_common::config;
use zaciraci_common::types::ExchangeRate;

/// スワップパス内の個々のプール情報
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SwapPoolInfo {
    /// プールID
    pub pool_id: u32,
    /// 入力トークンのインデックス
    pub token_in_idx: u8,
    /// 出力トークンのインデックス
    pub token_out_idx: u8,
    /// 入力側プールサイズ（yocto 単位の文字列）
    pub amount_in: String,
    /// 出力側プールサイズ（yocto 単位の文字列）
    pub amount_out: String,
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
    pub decimals: Option<i16>,
    #[allow(dead_code)] // Diesel Queryable でDBスキーマと一致させるため必要
    pub rate_calc_near: i64,
    #[allow(dead_code)] // Diesel Queryable でDBスキーマと一致させるため必要
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
    pub decimals: Option<i16>,
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
            decimals: Some(exchange_rate.decimals() as i16),
            timestamp,
            rate_calc_near,
            swap_path: swap_path.map(|p| serde_json::to_value(p).unwrap()),
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
    /// 新しい TokenRate を作成（ExchangeRate 使用）
    #[allow(dead_code)] // テストで使用
    pub fn new(
        base: TokenOutAccount,
        quote: TokenInAccount,
        exchange_rate: ExchangeRate,
        rate_calc_near: i64,
    ) -> Self {
        Self {
            base,
            quote,
            exchange_rate,
            timestamp: chrono::Utc::now().naive_utc(),
            rate_calc_near,
            swap_path: None,
        }
    }

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

    /// DB レコードから変換（decimals を明示的に指定）
    fn from_db_with_decimals(db_rate: DbTokenRate, decimals: u8) -> Result<Self> {
        let base = TokenAccount::from_str(&db_rate.base_token)?.into();
        let quote = TokenAccount::from_str(&db_rate.quote_token)?.into();
        let exchange_rate = ExchangeRate::from_raw_rate(db_rate.rate, decimals);
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

    /// 単一の DB レコードを変換し、NULL decimals があれば RPC で取得して backfill
    async fn from_db_with_backfill(db_rate: DbTokenRate) -> Result<Self> {
        let decimals = match db_rate.decimals {
            Some(d) => d as u8,
            None => {
                let log = DEFAULT.new(o!(
                    "function" => "from_db_with_backfill",
                    "base_token" => db_rate.base_token.clone(),
                ));
                trace!(log, "backfilling decimals for token with NULL");

                let client = crate::jsonrpc::new_client();
                let decimals = crate::trade::token_cache::get_token_decimals_cached(
                    &client,
                    &db_rate.base_token,
                )
                .await?;
                Self::backfill_decimals(&db_rate.base_token, decimals).await?;
                decimals
            }
        };
        Self::from_db_with_decimals(db_rate, decimals)
    }

    /// 複数の DB レコードを変換し、NULL decimals があれば RPC で取得して backfill
    ///
    /// 1. NULL decimals を持つトークンを特定
    /// 2. RPC で decimals を取得して DB を backfill
    /// 3. 正しい decimals で全レコードを変換
    async fn from_db_results_with_backfill(results: Vec<DbTokenRate>) -> Result<Vec<Self>> {
        use std::collections::HashMap;

        let log = DEFAULT.new(o!(
            "function" => "from_db_results_with_backfill",
        ));

        // NULL decimals を持つトークンを特定
        let tokens_with_null: HashSet<String> = results
            .iter()
            .filter(|r| r.decimals.is_none())
            .map(|r| r.base_token.clone())
            .collect();

        // RPC で decimals を並行取得して backfill
        let mut decimals_map: HashMap<String, u8> = HashMap::new();
        if !tokens_with_null.is_empty() {
            trace!(log, "backfilling decimals for tokens with NULL"; "tokens_with_null_count" => tokens_with_null.len());

            let client = crate::jsonrpc::new_client();

            // 並行実行数（設定から取得、デフォルト8）
            let concurrency = zaciraci_common::config::config()
                .trade
                .prediction_concurrency as usize;

            // 全トークンの decimals を並行取得
            let results: Vec<_> = stream::iter(tokens_with_null.iter().cloned())
                .map(|token| {
                    let client = &client;
                    let log = log.clone();
                    async move {
                        match crate::trade::token_cache::get_token_decimals_cached(client, &token)
                            .await
                        {
                            Ok(decimals) => {
                                // backfill は並行で実行
                                if let Err(e) = Self::backfill_decimals(&token, decimals).await {
                                    warn!(log, "failed to backfill decimals"; "token" => &token, "error" => %e);
                                }
                                Some((token, decimals))
                            }
                            Err(e) => {
                                warn!(log, "failed to fetch decimals for backfill"; "token" => &token, "error" => %e);
                                None
                            }
                        }
                    }
                })
                .buffer_unordered(concurrency)
                .collect()
                .await;

            // 結果を decimals_map に収集
            for result in results.into_iter().flatten() {
                decimals_map.insert(result.0, result.1);
            }
        }

        // 正しい decimals で変換
        let mut rates = Vec::with_capacity(results.len());
        for db_rate in results {
            let decimals = match db_rate.decimals {
                Some(d) => d as u8,
                None => match decimals_map.get(&db_rate.base_token) {
                    Some(&d) => d,
                    None => {
                        warn!(log, "skipping rate: decimals unknown after backfill"; "token" => &db_rate.base_token);
                        continue;
                    }
                },
            };
            rates.push(Self::from_db_with_decimals(db_rate, decimals)?);
        }

        Ok(rates)
    }

    /// 指定トークンの全レコードに decimals を設定
    pub async fn backfill_decimals(base_token: &str, decimals: u8) -> Result<usize> {
        use diesel::sql_types::{SmallInt, Text};

        let log = DEFAULT.new(o!(
            "function" => "backfill_decimals",
            "base_token" => base_token.to_string(),
            "decimals" => decimals,
        ));
        trace!(log, "start");

        let conn = connection_pool::get().await?;
        let base_token = base_token.to_string();
        let decimals_i16 = decimals as i16;

        let updated_count = conn
            .interact(move |conn| {
                diesel::sql_query(
                    "UPDATE token_rates SET decimals = $1 WHERE base_token = $2 AND decimals IS NULL",
                )
                .bind::<SmallInt, _>(decimals_i16)
                .bind::<Text, _>(&base_token)
                .execute(conn)
            })
            .await
            .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

        trace!(log, "finish"; "updated_count" => updated_count);
        Ok(updated_count)
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
    pub async fn batch_insert(token_rates: &[TokenRate]) -> Result<()> {
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

        let conn = connection_pool::get().await?;

        conn.interact(move |conn| {
            diesel::insert_into(token_rates::table)
                .values(&new_rates)
                .execute(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

        // 古いレコードをクリーンアップ
        let retention_days = config::get("TOKEN_RATES_RETENTION_DAYS")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(90);

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
    ///
    /// NULL decimals があれば RPC で取得して DB を backfill する。
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

            Ok(Some(TokenRate::from_db_with_backfill(result).await?))
        } else {
            Ok(None)
        }
    }

    /// 時間範囲内のレートを取得
    ///
    /// NULL decimals があれば RPC で取得して DB を backfill する。
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

        Self::from_db_results_with_backfill(results).await
    }

    /// 複数トークンの価格履歴を一括取得
    ///
    /// N個のトークンに対して1回のDBクエリで履歴を取得し、トークンごとのHashMapとして返す。
    /// NULL decimals があれば RPC で取得して DB を backfill する。
    pub async fn get_rates_for_multiple_tokens(
        tokens: &[String],
        quote: &TokenInAccount,
        range: &TimeRange,
    ) -> Result<HashMap<String, Vec<TokenRate>>> {
        use diesel::sql_types::{Array, Text, Timestamp};

        if tokens.is_empty() {
            return Ok(HashMap::new());
        }

        let conn = connection_pool::get().await?;

        let tokens_vec = tokens.to_vec();
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

        // backfill 処理して TokenRate に変換
        let rates = Self::from_db_results_with_backfill(results).await?;

        // トークンごとに分割
        let mut map: HashMap<String, Vec<TokenRate>> = HashMap::new();
        for rate in rates {
            map.entry(rate.base.to_string()).or_default().push(rate);
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
    #[allow(dead_code)] // テストで使用
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

    /// スポットレートに補正（フォールバック付き）
    ///
    /// swap_path が None の場合、提供された fallback_path を使用して補正。
    /// swap_path も fallback_path もない場合は補正なしで元のレートを返す。
    pub fn to_spot_rate_with_fallback(&self, fallback_path: Option<&SwapPath>) -> ExchangeRate {
        let path = self.swap_path.as_ref().or(fallback_path);
        if let Some(path) = path
            && let Some(first_pool) = path.pools.first()
            && let Ok(pool_amount) = first_pool.amount_in.parse::<BigDecimal>()
            && !pool_amount.is_zero()
        {
            // rate_calc_near は NEAR 単位で記録されているため、yocto に変換
            // (1 NEAR = 10^24 yocto)
            let delta_x = BigDecimal::from(self.rate_calc_near) * BigDecimal::from(10_u128.pow(24));
            let correction = (&pool_amount + &delta_x) / &pool_amount;
            return self.exchange_rate.clone() * correction;
        }
        self.exchange_rate.clone()
    }
}

#[cfg(test)]
mod tests;
