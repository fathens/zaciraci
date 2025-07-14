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

// データベース用モデル
#[allow(dead_code)]
#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = token_rates)]
struct DbTokenRate {
    pub id: i32,
    pub base_token: String,
    pub quote_token: String,
    pub rate: BigDecimal,
    pub timestamp: NaiveDateTime,
}

// データベース挿入用モデル
#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = token_rates)]
struct NewDbTokenRate {
    pub base_token: String,
    pub quote_token: String,
    pub rate: BigDecimal,
    pub timestamp: NaiveDateTime,
}

// ボラティリティ計算結果用の一時的な構造体
#[allow(dead_code)]
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TokenRate {
    pub base: TokenOutAccount,
    pub quote: TokenInAccount,
    pub rate: BigDecimal,
    pub timestamp: NaiveDateTime,
}

// 相互変換の実装
#[allow(dead_code)]
impl TokenRate {
    // 新しいTokenRateインスタンスを現在時刻で作成
    pub fn new(base: TokenOutAccount, quote: TokenInAccount, rate: BigDecimal) -> Self {
        Self {
            base,
            quote,
            rate,
            timestamp: chrono::Utc::now().naive_utc(),
        }
    }

    // 特定の時刻でTokenRateインスタンスを作成
    pub fn new_with_timestamp(
        base: TokenOutAccount,
        quote: TokenInAccount,
        rate: BigDecimal,
        timestamp: NaiveDateTime,
    ) -> Self {
        Self {
            base,
            quote,
            rate,
            timestamp,
        }
    }

    // DBオブジェクトから変換
    fn from_db(db_rate: DbTokenRate) -> Result<Self> {
        let base = TokenAccount::from_str(&db_rate.base_token)?.into();
        let quote = TokenAccount::from_str(&db_rate.quote_token)?.into();

        Ok(Self {
            base,
            quote,
            rate: db_rate.rate,
            timestamp: db_rate.timestamp,
        })
    }

    // NewDbTokenRateに変換
    fn to_new_db(&self) -> NewDbTokenRate {
        NewDbTokenRate {
            base_token: self.base.to_string(),
            quote_token: self.quote.to_string(),
            rate: self.rate.clone(),
            timestamp: self.timestamp,
        }
    }

    // データベースに挿入
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

        Ok(())
    }

    // 最新のレートを取得
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
                HAVING MIN(rate) > 0
                ORDER BY variance DESC
                LIMIT 100
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
