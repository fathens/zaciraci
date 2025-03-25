use crate::Result;
use crate::logging::*;
use crate::persistence::connection_pool;
use crate::persistence::schema::token_rates;
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use anyhow::anyhow;
use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use std::str::FromStr;

use super::TimeRange;

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
            .map(|(s, c)| match TokenAccount::from_str(&s) {
                Ok(token) => Some((token.into(), c)),
                Err(e) => {
                    error!(log, "Failed to parse token"; "token" => s, "error" => ?e);
                    None
                }
            })
            .filter_map(|v| v)
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
            .map(|(s, c)| match TokenAccount::from_str(&s) {
                Ok(token) => Some((token.into(), c)),
                Err(e) => {
                    error!(log, "Failed to parse token"; "token" => s, "error" => ?e);
                    None
                }
            })
            .filter_map(|v| v)
            .collect();

        Ok(bases)
    }

    pub async fn get_rates_in_time_range(range: &TimeRange, base: &TokenOutAccount, quote: &TokenInAccount) -> Result<Vec<TokenRate>> {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::SubsecRound;
    use diesel::RunQueryDsl;
    use serial_test::serial;

    // TokenRateインスタンス比較用マクロ
    macro_rules! assert_token_rate_eq {
        ($left:expr, $right:expr, $message:expr) => {{
            const PRECISION: u16 = 3; // ミリ秒精度

            // 各フィールドを個別に比較
            assert_eq!($left.base, $right.base, "{} - ベーストークンが一致しません", $message);
            assert_eq!($left.quote, $right.quote, "{} - クォートトークンが一致しません", $message);
            assert_eq!($left.rate, $right.rate, "{} - レートが一致しません", $message);

            // タイムスタンプだけ精度調整して比較
            let left_ts = $left.timestamp.trunc_subsecs(PRECISION);
            let right_ts = $right.timestamp.trunc_subsecs(PRECISION);
            assert_eq!(
                left_ts,
                right_ts,
                "{} - タイムスタンプが一致しません ({}ミリ秒精度) - 元の値: {} vs {}",
                $message,
                PRECISION,
                $left.timestamp,
                $right.timestamp
            );
        }};
    }

    // テーブルからすべてのレコードを削除する補助関数
    async fn clean_table() -> Result<()> {
        let conn = connection_pool::get().await?;
        conn.interact(|conn| diesel::delete(token_rates::table).execute(conn))
            .await
            .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

        // トランザクションがDBに反映されるのを少し待つ
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_token_rate_single_insert() -> Result<()> {
        // 1. テーブルの全レコード削除
        clean_table().await?;

        // テスト用のトークンアカウント作成
        let base: TokenOutAccount = TokenAccount::from_str("eth.token")?.into();
        let quote: TokenInAccount = TokenAccount::from_str("usdt.token")?.into();

        // 2. get_latest で None が返ることを確認
        let result = TokenRate::get_latest(&base, &quote).await?;
        assert!(result.is_none(), "Empty table should return None");

        // 3. １つインサート
        let rate = BigDecimal::from(1000);
        let timestamp = chrono::Utc::now().naive_utc();
        let token_rate =
            TokenRate::new_with_timestamp(base.clone(), quote.clone(), rate.clone(), timestamp);
        token_rate.insert().await?;

        // 4. get_latest でインサートしたレコードが返ることを確認
        let result = TokenRate::get_latest(&base, &quote).await?;
        assert!(result.is_some(), "Should return inserted record");

        let retrieved_rate = result.unwrap();
        assert_token_rate_eq!(retrieved_rate, token_rate, "Token rate should match");

        // クリーンアップ
        clean_table().await?;

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_token_rate_batch_insert_history() -> Result<()> {
        // 1. テーブルの全レコード削除
        clean_table().await?;

        // テスト用のトークンアカウント作成
        let base: TokenOutAccount = TokenAccount::from_str("eth.token")?.into();
        let quote: TokenInAccount = TokenAccount::from_str("usdt.token")?.into();

        // 2. 複数レコードを作成（異なるレートで）
        let earliest = chrono::Utc::now().naive_utc() - chrono::Duration::hours(2);
        let middle = chrono::Utc::now().naive_utc() - chrono::Duration::hours(1);
        let latest = chrono::Utc::now().naive_utc();

        let rates = vec![
            TokenRate::new_with_timestamp(
                base.clone(),
                quote.clone(),
                BigDecimal::from(1000),
                earliest,
            ),
            TokenRate::new_with_timestamp(
                base.clone(),
                quote.clone(),
                BigDecimal::from(1050),
                middle,
            ),
            TokenRate::new_with_timestamp(
                base.clone(),
                quote.clone(),
                BigDecimal::from(1100),
                latest,
            ),
        ];

        // 3. バッチ挿入
        TokenRate::batch_insert(&rates).await?;

        // 4. get_historyで履歴を取得（リミット無制限）
        let history = TokenRate::get_history(&base, &quote, 10).await?;

        // 5. 結果の検証
        assert_eq!(history.len(), 3, "Should return 3 records");

        // レコードがレートの大きさと時刻の順序で正しく並んでいることを確認
        let expected_rates = [
            BigDecimal::from(1100),
            BigDecimal::from(1050),
            BigDecimal::from(1000),
        ];
        for (i, rate) in history.iter().enumerate() {
            assert_eq!(
                rate.rate, expected_rates[i],
                "Record {} should have rate {}",
                i, expected_rates[i]
            );
        }

        // タイムスタンプの順序を確認（マクロの代わりに明示的に比較）
        // この部分は全体の順序関係だけを確認しており、精密な値は比較していない
        assert!(
            history[0].timestamp > history[1].timestamp,
            "First record should have newer timestamp than second"
        );
        assert!(
            history[1].timestamp > history[2].timestamp,
            "Second record should have newer timestamp than third"
        );

        // 個別のTimestampを確認
        assert_token_rate_eq!(
            history[0],
            TokenRate::new_with_timestamp(
                base.clone(),
                quote.clone(),
                BigDecimal::from(1100),
                latest
            ),
            "Latest record should match"
        );
        assert_token_rate_eq!(
            history[1],
            TokenRate::new_with_timestamp(
                base.clone(),
                quote.clone(),
                BigDecimal::from(1050),
                middle
            ),
            "Middle record should match"
        );
        assert_token_rate_eq!(
            history[2],
            TokenRate::new_with_timestamp(
                base.clone(),
                quote.clone(),
                BigDecimal::from(1000),
                earliest
            ),
            "Earliest record should match"
        );

        // リミットが機能することを確認
        let limited_history = TokenRate::get_history(&base, &quote, 2).await?;
        assert_eq!(limited_history.len(), 2, "Should return only 2 records");
        assert_eq!(
            limited_history[0].rate,
            BigDecimal::from(1100),
            "Newest record should be first"
        );
        assert_eq!(
            limited_history[1].rate,
            BigDecimal::from(1050),
            "Second newest should be second"
        );

        // クリーンアップ
        clean_table().await?;

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_token_rate_different_pairs() -> Result<()> {
        // 1. テーブルの全レコード削除
        clean_table().await?;

        // テスト用のトークンアカウント作成 - 複数のペア
        let base1: TokenOutAccount = TokenAccount::from_str("eth.token")?.into();
        let base2: TokenOutAccount = TokenAccount::from_str("btc.token")?.into();
        let quote1: TokenInAccount = TokenAccount::from_str("usdt.token")?.into();
        let quote2: TokenInAccount = TokenAccount::from_str("near.token")?.into();

        // 2. 異なるトークンペアのレコードを挿入
        let now = chrono::Utc::now().naive_utc();
        let rate1 = TokenRate::new_with_timestamp(
            base1.clone(),
            quote1.clone(),
            BigDecimal::from(1000),
            now,
        );
        let rate2 = TokenRate::new_with_timestamp(
            base2.clone(),
            quote1.clone(),
            BigDecimal::from(2000),
            now,
        );
        let rate3 = TokenRate::new_with_timestamp(
            base1.clone(),
            quote2.clone(),
            BigDecimal::from(3000),
            now,
        );

        // 3. レコードを挿入
        TokenRate::batch_insert(&[rate1.clone(), rate2.clone(), rate3.clone()]).await?;

        // 4. 特定のペアのみが取得されることを確認
        let result1 = TokenRate::get_latest(&base1, &quote1).await?;
        assert!(result1.is_some(), "base1-quote1 pair should be found");
        let retrieved_rate1 = result1.unwrap();
        assert_token_rate_eq!(
            retrieved_rate1,
            rate1,
            "base1-quote1 TokenRate should match"
        );

        let result2 = TokenRate::get_latest(&base2, &quote1).await?;
        assert!(result2.is_some(), "base2-quote1 pair should be found");
        let retrieved_rate2 = result2.unwrap();
        assert_token_rate_eq!(
            retrieved_rate2,
            rate2,
            "base2-quote1 TokenRate should match"
        );

        let result3 = TokenRate::get_latest(&base1, &quote2).await?;
        assert!(result3.is_some(), "base1-quote2 pair should be found");
        let retrieved_rate3 = result3.unwrap();
        assert_token_rate_eq!(
            retrieved_rate3,
            rate3,
            "base1-quote2 TokenRate should match"
        );

        // 5. 存在しないペアが None を返すことを確認
        let result4 = TokenRate::get_latest(&base2, &quote2).await?;
        assert!(result4.is_none(), "base2-quote2 pair should not be found");

        // 6. get_history でも特定のペアだけが取得されることを確認
        let history1 = TokenRate::get_history(&base1, &quote1, 10).await?;
        assert_eq!(history1.len(), 1, "Should find 1 record for base1-quote1");
        assert_token_rate_eq!(
            history1[0],
            rate1,
            "base1-quote1 history TokenRate should match"
        );

        let history2 = TokenRate::get_history(&base2, &quote1, 10).await?;
        assert_eq!(history2.len(), 1, "Should find 1 record for base2-quote1");
        assert_token_rate_eq!(
            history2[0],
            rate2,
            "base2-quote1 history TokenRate should match"
        );

        // 7. 存在しないペアは空の配列を返すことを確認
        let history3 = TokenRate::get_history(&base2, &quote2, 10).await?;
        assert_eq!(history3.len(), 0, "Should find 0 records for base2-quote2");

        // クリーンアップ
        clean_table().await?;

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_token_rate_get_latests_by_quote() -> Result<()> {
        // 1. テーブルの全レコード削除
        clean_table().await?;

        // テスト用のトークンアカウント作成
        let base1: TokenOutAccount = TokenAccount::from_str("eth.token")?.into();
        let base2: TokenOutAccount = TokenAccount::from_str("btc.token")?.into();
        let base3: TokenOutAccount = TokenAccount::from_str("near.token")?.into();
        let quote1: TokenInAccount = TokenAccount::from_str("usdt.token")?.into();
        let quote2: TokenInAccount = TokenAccount::from_str("usdc.token")?.into();

        // 2. タイムスタンプを設定
        let now = chrono::Utc::now().naive_utc();
        let one_hour_ago = now - chrono::Duration::hours(1);
        let two_hours_ago = now - chrono::Duration::hours(2);

        // 3. 複数のレコードを挿入（同じクォートトークンで異なるベーストークン）
        let rates = vec![
            // quote1用のレコード
            TokenRate::new_with_timestamp(
                base1.clone(),
                quote1.clone(),
                BigDecimal::from(1000),
                two_hours_ago, // 古いレコード
            ),
            TokenRate::new_with_timestamp(
                base1.clone(),
                quote1.clone(),
                BigDecimal::from(1100),
                one_hour_ago, // 新しいレコード（base1用）
            ),
            TokenRate::new_with_timestamp(
                base2.clone(),
                quote1.clone(),
                BigDecimal::from(20000),
                now, // 最新レコード（base2用）
            ),
            // 異なるクォートトークン（quote2）用のレコード - 結果に含まれないはず
            TokenRate::new_with_timestamp(base3.clone(), quote2.clone(), BigDecimal::from(5), now),
        ];

        // 4. バッチ挿入
        TokenRate::batch_insert(&rates).await?;

        // 5. get_latests_by_quoteでquote1のレコードを取得
        let results = TokenRate::get_latests_by_quote(&quote1).await?;

        // 6. 結果の検証
        // 2つのベーストークン（base1, base2）が取得されるはず
        assert_eq!(results.len(), 2, "Should find 2 base tokens for quote1");

        // 結果を検証するために、トークン名でソート
        let mut sorted_results = results.clone();
        sorted_results.sort_by(|a, b| a.0.to_string().cmp(&b.0.to_string()));

        // 各ベーストークンとタイムスタンプのペアを検証
        let (result_base1, result_time1) = &sorted_results[0]; // btc
        let (result_base2, result_time2) = &sorted_results[1]; // eth

        // ベーストークンを検証
        assert_eq!(
            result_base1.to_string(),
            "btc.token",
            "First base token should be btc.token"
        );
        assert_eq!(
            result_base2.to_string(),
            "eth.token",
            "Second base token should be eth.token"
        );

        // タイムスタンプを精度を考慮して比較
        {
            // base2 (btc) のタイムスタンプがnowに近いことを確認
            let expected_btc = TokenRate::new_with_timestamp(
                base2.clone(),
                quote1.clone(),
                BigDecimal::from(20000),
                now,
            );
            let actual_btc = TokenRate::new_with_timestamp(
                result_base1.clone(),
                quote1.clone(),
                BigDecimal::from(20000),
                *result_time1,
            );
            assert_token_rate_eq!(
                actual_btc,
                expected_btc,
                "BTCのタイムスタンプが正しくありません"
            );
        }

        {
            // base1 (eth) のタイムスタンプがone_hour_agoに近いことを確認
            let expected_eth = TokenRate::new_with_timestamp(
                base1.clone(),
                quote1.clone(),
                BigDecimal::from(1100),
                one_hour_ago,
            );
            let actual_eth = TokenRate::new_with_timestamp(
                result_base2.clone(),
                quote1.clone(),
                BigDecimal::from(1100),
                *result_time2,
            );
            assert_token_rate_eq!(
                actual_eth,
                expected_eth,
                "ETHのタイムスタンプが正しくありません"
            );
        }

        // quote2のレコードも確認（base3のみ存在するはず）
        let results2 = TokenRate::get_latests_by_quote(&quote2).await?;
        assert_eq!(results2.len(), 1, "Should find 1 base token for quote2");
        assert_eq!(
            results2[0].0.to_string(),
            "near.token",
            "Base token for quote2 should be near.token"
        );

        // クリーンアップ
        clean_table().await?;

        Ok(())
    }
}
