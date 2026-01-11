use crate::Result;
use crate::logging::*;
use crate::persistence::TimeRange;
use crate::persistence::connection_pool;
use crate::persistence::schema::token_rates;
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use anyhow::anyhow;
use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use std::str::FromStr;
use zaciraci_common::config;
use zaciraci_common::types::ExchangeRate;

// データベース用モデル（読み込み用）
#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = token_rates)]
struct DbTokenRate {
    #[allow(dead_code)]
    pub id: i32,
    pub base_token: String,
    pub quote_token: String,
    pub rate: BigDecimal,
    pub decimals: Option<i16>,
    pub timestamp: NaiveDateTime,
}

// データベース挿入用モデル（ExchangeRate から構築）
#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = token_rates)]
struct NewDbTokenRate {
    pub base_token: String,
    pub quote_token: String,
    pub rate: BigDecimal,
    pub decimals: Option<i16>,
    pub timestamp: NaiveDateTime,
}

impl NewDbTokenRate {
    /// ExchangeRate から挿入用モデルを作成
    fn from_exchange_rate(
        base: &TokenOutAccount,
        quote: &TokenInAccount,
        exchange_rate: &ExchangeRate,
        timestamp: NaiveDateTime,
    ) -> Self {
        Self {
            base_token: base.to_string(),
            quote_token: quote.to_string(),
            rate: exchange_rate.raw_rate().clone(),
            decimals: Some(exchange_rate.decimals() as i16),
            timestamp,
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
}

/// 後方互換性のために rate フィールドへのアクセサを提供
impl TokenRate {
    /// rate の BigDecimal 値を取得（後方互換性用）
    pub fn rate(&self) -> &BigDecimal {
        self.exchange_rate.raw_rate()
    }

    /// decimals を取得
    #[allow(dead_code)]
    pub fn decimals(&self) -> u8 {
        self.exchange_rate.decimals()
    }
}

// 相互変換の実装
impl TokenRate {
    /// 新しい TokenRate を作成（ExchangeRate 使用）
    pub fn new(base: TokenOutAccount, quote: TokenInAccount, exchange_rate: ExchangeRate) -> Self {
        Self {
            base,
            quote,
            exchange_rate,
            timestamp: chrono::Utc::now().naive_utc(),
        }
    }

    /// 特定の時刻で TokenRate を作成
    #[allow(dead_code)]
    pub fn new_with_timestamp(
        base: TokenOutAccount,
        quote: TokenInAccount,
        exchange_rate: ExchangeRate,
        timestamp: NaiveDateTime,
    ) -> Self {
        Self {
            base,
            quote,
            exchange_rate,
            timestamp,
        }
    }

    /// DB レコードから変換（decimals が NULL の場合はデフォルト値を使用）
    ///
    /// 注意: 本来は decimals が NULL の場合は RPC で取得して backfill すべき。
    /// バッチ処理など backfill が不要な場合はこの関数を使用する。
    fn from_db(db_rate: DbTokenRate) -> Result<Self> {
        let base = TokenAccount::from_str(&db_rate.base_token)?.into();
        let quote = TokenAccount::from_str(&db_rate.quote_token)?.into();

        // decimals が NULL の場合はデフォルト値 24 (NEAR) を使用
        let decimals = db_rate.decimals.map(|d| d as u8).unwrap_or(24);
        let exchange_rate = ExchangeRate::from_raw_rate(db_rate.rate, decimals);

        Ok(Self {
            base,
            quote,
            exchange_rate,
            timestamp: db_rate.timestamp,
        })
    }

    /// DB レコードから変換（decimals が NULL の場合は RPC で取得して backfill）
    ///
    /// decimals が NULL の場合:
    /// 1. RPC で ft_metadata を呼び出して decimals を取得
    /// 2. 同じトークンの全レコードを UPDATE
    /// 3. 取得した decimals で ExchangeRate を構築
    #[allow(dead_code)]
    async fn from_db_with_backfill<C>(db_rate: DbTokenRate, client: &C) -> Result<Self>
    where
        C: crate::jsonrpc::ViewContract,
    {
        let base = TokenAccount::from_str(&db_rate.base_token)?.into();
        let quote = TokenAccount::from_str(&db_rate.quote_token)?.into();

        let decimals = match db_rate.decimals {
            Some(d) => d as u8,
            None => {
                // decimals が NULL → RPC で取得して backfill
                let d = crate::trade::stats::get_token_decimals(client, &db_rate.base_token).await;
                Self::backfill_decimals(&db_rate.base_token, d).await?;
                d
            }
        };

        let exchange_rate = ExchangeRate::from_raw_rate(db_rate.rate, decimals);

        Ok(Self {
            base,
            quote,
            exchange_rate,
            timestamp: db_rate.timestamp,
        })
    }

    /// 指定トークンの全レコードに decimals を設定
    #[allow(dead_code)]
    pub async fn backfill_decimals(base_token: &str, decimals: u8) -> Result<usize> {
        use diesel::sql_types::{SmallInt, Text};

        let log = DEFAULT.new(o!(
            "function" => "backfill_decimals",
            "base_token" => base_token.to_string(),
            "decimals" => decimals,
        ));
        info!(log, "start");

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

        info!(log, "finish"; "updated_count" => updated_count);
        Ok(updated_count)
    }

    /// NewDbTokenRate に変換
    fn to_new_db(&self) -> NewDbTokenRate {
        NewDbTokenRate::from_exchange_rate(
            &self.base,
            &self.quote,
            &self.exchange_rate,
            self.timestamp,
        )
    }

    // データベースに挿入
    #[allow(dead_code)]
    pub async fn insert(&self) -> Result<()> {
        use diesel::RunQueryDsl;

        let new_rate = self.to_new_db();
        let conn = connection_pool::get().await?;

        conn.interact(move |conn| {
            diesel::insert_into(token_rates::table)
                .values(&new_rate)
                .execute(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

        Ok(())
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
            .unwrap_or(365);

        info!(log, "cleaning up old records"; "retention_days" => retention_days);
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
        info!(log, "start");

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

        info!(log, "finish"; "deleted_count" => deleted_count, "cutoff_date" => %cutoff_date);
        Ok(())
    }

    // 最新のレートを取得
    #[allow(dead_code)]
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

    // 履歴レコードを取得（新しい順）
    #[allow(dead_code)]
    pub async fn get_history(
        base: &TokenOutAccount,
        quote: &TokenInAccount,
        limit: i64,
    ) -> Result<Vec<TokenRate>> {
        use diesel::QueryDsl;

        let base_str = base.to_string();
        let quote_str = quote.to_string();
        let conn = connection_pool::get().await?;

        let results = conn
            .interact(move |conn| {
                token_rates::table
                    .filter(token_rates::base_token.eq(&base_str))
                    .filter(token_rates::quote_token.eq(&quote_str))
                    .order(token_rates::timestamp.desc())
                    .limit(limit)
                    .load::<DbTokenRate>(conn)
            })
            .await
            .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

        results.into_iter().map(TokenRate::from_db).collect()
    }

    // quoteトークンを指定して対応するすべてのbaseトークンとその最新時刻を取得
    #[allow(dead_code)]
    pub async fn get_latests_by_quote(
        quote: &TokenInAccount,
    ) -> Result<Vec<(TokenOutAccount, NaiveDateTime)>> {
        use diesel::QueryDsl;
        use diesel::dsl::max;

        let quote_str = quote.to_string();
        let conn = connection_pool::get().await?;

        // 各base_tokenごとに最新のタイムスタンプを取得
        let latest_timestamps = conn
            .interact(move |conn| {
                token_rates::table
                    .filter(token_rates::quote_token.eq(&quote_str))
                    .group_by(token_rates::base_token)
                    .select((token_rates::base_token, max(token_rates::timestamp)))
                    .load::<(String, Option<NaiveDateTime>)>(conn)
            })
            .await
            .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

        // 結果をトークンとタイムスタンプのペアに変換
        let mut results = Vec::new();
        for (base_token, timestamp_opt) in latest_timestamps {
            if let Some(timestamp) = timestamp_opt {
                match TokenAccount::from_str(&base_token) {
                    Ok(token) => results.push((token.into(), timestamp)),
                    Err(e) => return Err(anyhow!("Failed to parse token: {:?}", e)),
                }
            }
        }

        Ok(results)
    }

    // quote トークンとその個数を時間帯で区切って取り出す
    pub async fn get_quotes_in_time_range(range: &TimeRange) -> Result<Vec<(TokenInAccount, i64)>> {
        use diesel::QueryDsl;
        use diesel::dsl::count;

        let log = DEFAULT.new(o!("function" => "get_quotes_in_time_range"));
        let conn = connection_pool::get().await?;

        let start = range.start;
        let end = range.end;

        let results = conn
            .interact(move |conn| {
                token_rates::table
                    .filter(token_rates::timestamp.gt(start))
                    .filter(token_rates::timestamp.le(end))
                    .group_by(token_rates::quote_token)
                    .select((token_rates::quote_token, count(token_rates::quote_token)))
                    .load::<(String, i64)>(conn)
            })
            .await
            .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

        let quotes = results
            .into_iter()
            .filter_map(|(s, c)| match TokenAccount::from_str(&s) {
                Ok(token) => Some((token.into(), c)),
                Err(e) => {
                    error!(log, "Failed to parse token"; "token" => s, "error" => ?e);
                    None
                }
            })
            .collect();

        Ok(quotes)
    }

    pub async fn get_bases_in_time_range(
        range: &TimeRange,
        quote: &TokenInAccount,
    ) -> Result<Vec<(TokenOutAccount, i64)>> {
        use diesel::QueryDsl;
        use diesel::dsl::count;

        let log = DEFAULT.new(o!("function" => "get_bases_in_time_range"));
        let conn = connection_pool::get().await?;

        let start = range.start;
        let end = range.end;
        let quote_str = quote.to_string();

        let results = conn
            .interact(move |conn| {
                token_rates::table
                    .filter(token_rates::timestamp.gt(start))
                    .filter(token_rates::timestamp.le(end))
                    .filter(token_rates::quote_token.eq(&quote_str))
                    .group_by(token_rates::base_token)
                    .select((token_rates::base_token, count(token_rates::base_token)))
                    .load::<(String, i64)>(conn)
            })
            .await
            .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

        let bases = results
            .into_iter()
            .filter_map(|(s, c)| match TokenAccount::from_str(&s) {
                Ok(token) => Some((token.into(), c)),
                Err(e) => {
                    error!(log, "Failed to parse token"; "token" => s, "error" => ?e);
                    None
                }
            })
            .collect();

        Ok(bases)
    }

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
                    .order_by(token_rates::timestamp)
                    .load::<DbTokenRate>(conn)
            })
            .await
            .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

        results.into_iter().map(TokenRate::from_db).collect()
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
        info!(log, "start");

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
}

#[cfg(test)]
mod tests;
