// use super::*;
use crate::Result;
use crate::connection_pool;
use crate::schema::token_rates;
use crate::token_rate::{SwapPath, SwapPoolInfo, TokenRate};
use anyhow::anyhow;
use bigdecimal::BigDecimal;
use chrono::{NaiveDateTime, SubsecRound};
use common::types::ExchangeRate;
use common::types::{TokenAccount, TokenInAccount, TokenOutAccount};
use diesel::RunQueryDsl;
use serial_test::serial;
use std::str::FromStr;

/// テスト用ヘルパー: BigDecimal からデフォルト decimals (24) の ExchangeRate を作成
fn make_rate(value: i64) -> ExchangeRate {
    ExchangeRate::from_raw_rate(BigDecimal::from(value), 24)
}

/// テスト用ヘルパー: 文字列からデフォルト decimals (24) の ExchangeRate を作成
fn make_rate_str(value: &str) -> ExchangeRate {
    ExchangeRate::from_raw_rate(BigDecimal::from_str(value).unwrap(), 24)
}

/// テスト用ヘルパー: TokenRate を簡潔に作成
fn make_token_rate(
    base: TokenOutAccount,
    quote: TokenInAccount,
    rate: i64,
    timestamp: NaiveDateTime,
) -> TokenRate {
    TokenRate {
        base,
        quote,
        exchange_rate: make_rate(rate),
        timestamp,
        rate_calc_near: 10,
        swap_path: None,
    }
}

/// テスト用ヘルパー: TokenRate を文字列レートで作成
fn make_token_rate_str(
    base: TokenOutAccount,
    quote: TokenInAccount,
    rate: &str,
    timestamp: NaiveDateTime,
) -> TokenRate {
    TokenRate {
        base,
        quote,
        exchange_rate: make_rate_str(rate),
        timestamp,
        rate_calc_near: 10,
        swap_path: None,
    }
}

/// テスト用ヘルパー: decimals 取得コールバック（テストデータは常に decimals=24 で挿入するため backfill は発生しない）
fn test_get_decimals() -> &'static super::GetDecimalsFn {
    &|_token: &str| Box::pin(async move { Ok(24u8) })
}

// TokenRateインスタンス比較用マクロ
macro_rules! assert_token_rate_eq {
    ($left:expr, $right:expr, $message:expr) => {{
        const PRECISION: u16 = 3; // ミリ秒精度

        // 各フィールドを個別に比較
        assert_eq!(
            $left.base, $right.base,
            "{} - ベーストークンが一致しません",
            $message
        );
        assert_eq!(
            $left.quote, $right.quote,
            "{} - クォートトークンが一致しません",
            $message
        );
        assert_eq!(
            $left.exchange_rate.raw_rate(),
            $right.exchange_rate.raw_rate(),
            "{} - レートが一致しません",
            $message
        );

        // タイムスタンプだけ精度調整して比較
        let left_ts = $left.timestamp.trunc_subsecs(PRECISION);
        let right_ts = $right.timestamp.trunc_subsecs(PRECISION);
        assert_eq!(
            left_ts, right_ts,
            "{} - タイムスタンプが一致しません ({}ミリ秒精度) - 元: {} vs {}",
            $message, PRECISION, $left.timestamp, $right.timestamp
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
    let result = TokenRate::get_latest(&base, &quote, test_get_decimals()).await?;
    assert!(result.is_none(), "Empty table should return None");

    // 3. １つインサート
    let timestamp = chrono::Utc::now().naive_utc();
    let token_rate = make_token_rate(base.clone(), quote.clone(), 1000, timestamp);
    TokenRate::batch_insert(std::slice::from_ref(&token_rate)).await?;

    // 4. get_latest でインサートしたレコードが返ることを確認
    let result = TokenRate::get_latest(&base, &quote, test_get_decimals()).await?;
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
    use crate::TimeRange;

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
        make_token_rate(base.clone(), quote.clone(), 1000, earliest),
        make_token_rate(base.clone(), quote.clone(), 1050, middle),
        make_token_rate(base.clone(), quote.clone(), 1100, latest),
    ];

    // 3. バッチ挿入
    TokenRate::batch_insert(&rates).await?;

    // 4. get_rates_in_time_range で履歴を取得（時系列順で返る）
    let time_range = TimeRange {
        start: earliest - chrono::Duration::minutes(1),
        end: latest + chrono::Duration::minutes(1),
    };
    let mut history =
        TokenRate::get_rates_in_time_range(&time_range, &base, &quote, test_get_decimals()).await?;
    // 新しい順に並び替え（get_history は降順、get_rates_in_time_range は昇順）
    history.reverse();

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
            rate.exchange_rate.raw_rate(),
            &expected_rates[i],
            "Record {} should have rate {}",
            i,
            expected_rates[i]
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
        make_token_rate(base.clone(), quote.clone(), 1100, latest),
        "Latest record should match"
    );
    assert_token_rate_eq!(
        history[1],
        make_token_rate(base.clone(), quote.clone(), 1050, middle),
        "Middle record should match"
    );
    assert_token_rate_eq!(
        history[2],
        make_token_rate(base.clone(), quote.clone(), 1000, earliest),
        "Earliest record should match"
    );

    // クリーンアップ
    clean_table().await?;

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_token_rate_different_pairs() -> Result<()> {
    use crate::TimeRange;

    // 1. テーブルの全レコード削除
    clean_table().await?;

    // テスト用のトークンアカウント作成 - 複数のペア
    let base1: TokenOutAccount = TokenAccount::from_str("eth.token")?.into();
    let base2: TokenOutAccount = TokenAccount::from_str("btc.token")?.into();
    let quote1: TokenInAccount = TokenAccount::from_str("usdt.token")?.into();
    let quote2: TokenInAccount = TokenAccount::from_str("near.token")?.into();

    // 2. 異なるトークンペアのレコードを挿入
    let now = chrono::Utc::now().naive_utc();
    let rate1 = make_token_rate(base1.clone(), quote1.clone(), 1000, now);
    let rate2 = make_token_rate(base2.clone(), quote1.clone(), 2000, now);
    let rate3 = make_token_rate(base1.clone(), quote2.clone(), 3000, now);

    // 3. レコードを挿入
    TokenRate::batch_insert(&[rate1.clone(), rate2.clone(), rate3.clone()]).await?;

    // 4. 特定のペアのみが取得されることを確認
    let result1 = TokenRate::get_latest(&base1, &quote1, test_get_decimals()).await?;
    assert!(result1.is_some(), "base1-quote1 pair should be found");
    let retrieved_rate1 = result1.unwrap();
    assert_token_rate_eq!(
        retrieved_rate1,
        rate1,
        "base1-quote1 TokenRate should match"
    );

    let result2 = TokenRate::get_latest(&base2, &quote1, test_get_decimals()).await?;
    assert!(result2.is_some(), "base2-quote1 pair should be found");
    let retrieved_rate2 = result2.unwrap();
    assert_token_rate_eq!(
        retrieved_rate2,
        rate2,
        "base2-quote1 TokenRate should match"
    );

    let result3 = TokenRate::get_latest(&base1, &quote2, test_get_decimals()).await?;
    assert!(result3.is_some(), "base1-quote2 pair should be found");
    let retrieved_rate3 = result3.unwrap();
    assert_token_rate_eq!(
        retrieved_rate3,
        rate3,
        "base1-quote2 TokenRate should match"
    );

    // 5. 存在しないペアが None を返すことを確認
    let result4 = TokenRate::get_latest(&base2, &quote2, test_get_decimals()).await?;
    assert!(result4.is_none(), "base2-quote2 pair should not be found");

    // 6. get_rates_in_time_range でも特定のペアだけが取得されることを確認
    let time_range = TimeRange {
        start: now - chrono::Duration::minutes(1),
        end: now + chrono::Duration::minutes(1),
    };

    let history1 =
        TokenRate::get_rates_in_time_range(&time_range, &base1, &quote1, test_get_decimals())
            .await?;
    assert_eq!(history1.len(), 1, "Should find 1 record for base1-quote1");
    assert_token_rate_eq!(
        history1[0],
        rate1,
        "base1-quote1 history TokenRate should match"
    );

    let history2 =
        TokenRate::get_rates_in_time_range(&time_range, &base2, &quote1, test_get_decimals())
            .await?;
    assert_eq!(history2.len(), 1, "Should find 1 record for base2-quote1");
    assert_token_rate_eq!(
        history2[0],
        rate2,
        "base2-quote1 history TokenRate should match"
    );

    // 7. 存在しないペアは空の配列を返すことを確認
    let history3 =
        TokenRate::get_rates_in_time_range(&time_range, &base2, &quote2, test_get_decimals())
            .await?;
    assert_eq!(history3.len(), 0, "Should find 0 records for base2-quote2");

    // クリーンアップ
    clean_table().await?;

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_get_by_volatility_in_time_range() -> Result<()> {
    // 1. テーブルの全レコード削除
    clean_table().await?;

    // テスト用のトークンアカウント作成
    let base1: TokenOutAccount = TokenAccount::from_str("eth.token")?.into();
    let base2: TokenOutAccount = TokenAccount::from_str("btc.token")?.into();
    let base3: TokenOutAccount = TokenAccount::from_str("near.token")?.into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token")?.into();

    // 2. タイムスタンプを設定
    let now = chrono::Utc::now().naive_utc();
    let one_hour_ago = now - chrono::Duration::hours(1);
    let two_hours_ago = now - chrono::Duration::hours(2);

    // 3. 複数のレコードを挿入（異なるボラティリティを持つデータ）
    let rates = vec![
        // base1 (eth) - 変動率 50%
        make_token_rate(base1.clone(), quote.clone(), 1000, two_hours_ago),
        make_token_rate(base1.clone(), quote.clone(), 1500, one_hour_ago),
        // base2 (btc) - 変動率 100%
        make_token_rate(base2.clone(), quote.clone(), 20000, two_hours_ago),
        make_token_rate(base2.clone(), quote.clone(), 40000, one_hour_ago),
        // base3 (near) - 変動率 10%
        make_token_rate(base3.clone(), quote.clone(), 5, two_hours_ago),
        make_token_rate_str(base3.clone(), quote.clone(), "5.5", one_hour_ago),
    ];

    // 4. バッチ挿入
    TokenRate::batch_insert(&rates).await?;

    // 5. 時間範囲を設定
    let time_range = crate::TimeRange {
        start: two_hours_ago - chrono::Duration::minutes(5), // 少し余裕を持たせる
        end: now + chrono::Duration::minutes(5),             // 少し余裕を持たせる
    };

    // 6. get_by_volatility_in_time_rangeでボラティリティを取得
    let results = TokenRate::get_by_volatility_in_time_range(&time_range, &quote).await?;

    // 7. 結果の検証
    // 結果が3つのベーストークンを含むことを確認
    assert_eq!(
        results.len(),
        3,
        "Should find 3 base tokens with volatility data"
    );

    // ボラティリティの降順でソートされていることを確認
    // 1. btc (100%)
    // 2. eth (50%)
    // 3. near (10%)
    assert_eq!(
        results[0].base.to_string(),
        "btc.token",
        "First token should be btc with highest volatility"
    );
    assert_eq!(
        results[1].base.to_string(),
        "eth.token",
        "Second token should be eth with medium volatility"
    );
    assert_eq!(
        results[2].base.to_string(),
        "near.token",
        "Third token should be near with lowest volatility"
    );

    // 各トークンのボラティリティ値を検証
    // btcの分散が最も大きい
    assert!(
        results[0].variance > results[1].variance,
        "BTC variance should be greater than ETH variance"
    );

    // ethの分散は中程度
    assert!(
        results[1].variance > results[2].variance,
        "ETH variance should be greater than NEAR variance"
    );

    // nearの分散が最も小さい
    assert!(
        results[2].variance > 0,
        "NEAR variance should be greater than 0"
    );

    // 8. エッジケースのテスト: 最小レートが0の場合
    clean_table().await?;

    // レートが0を含むデータを挿入（HAVING MIN(rate) > 0により0を含むトークンは除外）
    let zero_rate_data = vec![
        // base1: 0を含むため除外される
        make_token_rate(
            base1.clone(),
            quote.clone(),
            0,
            two_hours_ago + chrono::Duration::minutes(1),
        ), // MIN(rate) = 0 なので除外
        make_token_rate(
            base1.clone(),
            quote.clone(),
            100,
            two_hours_ago + chrono::Duration::minutes(2),
        ),
        make_token_rate(
            base1.clone(),
            quote.clone(),
            150,
            one_hour_ago + chrono::Duration::minutes(1),
        ),
        // base2: 全て正の値のため含まれる
        make_token_rate(base2.clone(), quote.clone(), 50, one_hour_ago),
        make_token_rate(
            base2.clone(),
            quote.clone(),
            60,
            one_hour_ago + chrono::Duration::minutes(30),
        ),
    ];

    TokenRate::batch_insert(&zero_rate_data).await?;

    // ボラティリティを取得
    let zero_results = TokenRate::get_by_volatility_in_time_range(&time_range, &quote).await?;

    // 結果を検証: HAVING MIN(rate) > 0により、base1は0を含むため除外される
    // base2のみが結果に含まれる（全て正の値）
    assert_eq!(
        zero_results.len(),
        1,
        "Should find 1 token (base1 excluded due to MIN(rate) = 0)"
    );

    // base2のみが結果に含まれることを確認（全て正の値）
    let btc_result = &zero_results[0];
    assert_eq!(
        btc_result.base.to_string(),
        "btc.token",
        "Only btc token should be found"
    );

    // 分散値が0より大きいことを確認
    assert!(
        btc_result.variance > 0,
        "BTC variance should be greater than 0, got {}",
        btc_result.variance
    );

    // クリーンアップ
    clean_table().await?;

    // 9. エッジケースのテスト: データがない場合
    clean_table().await?;

    let empty_results = TokenRate::get_by_volatility_in_time_range(&time_range, &quote).await?;
    assert_eq!(
        empty_results.len(),
        0,
        "Should return empty list when no data is available"
    );

    // クリーンアップ
    clean_table().await?;

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_get_by_volatility_in_time_range_edge_cases() -> Result<()> {
    // 1. テーブルの全レコード削除
    clean_table().await?;

    // テスト用のトークンアカウント作成
    let base1: TokenOutAccount = TokenAccount::from_str("eth.token")?.into();
    let base2: TokenOutAccount = TokenAccount::from_str("btc.token")?.into();
    let _base3: TokenOutAccount = TokenAccount::from_str("near.token")?.into();
    let _base4: TokenOutAccount = TokenAccount::from_str("sol.token")?.into();
    let quote1: TokenInAccount = TokenAccount::from_str("usdt.token")?.into();
    let quote2: TokenInAccount = TokenAccount::from_str("usdc.token")?.into();

    // 2. タイムスタンプを設定
    let now = chrono::Utc::now().naive_utc();
    let one_hour_ago = now - chrono::Duration::hours(1);
    let two_hours_ago = now - chrono::Duration::hours(2);
    let just_before_range = two_hours_ago + chrono::Duration::seconds(1); // 範囲開始直後
    let just_after_range = now - chrono::Duration::seconds(1); // 範囲終了直前

    // 3. 時間範囲を設定
    let time_range = crate::TimeRange {
        start: two_hours_ago,
        end: now,
    };

    // ケース1: 境界値テスト - 時間範囲の境界値データ
    let boundary_test_data = vec![
        // 範囲内のデータ（境界値ぎりぎり）
        make_token_rate(base1.clone(), quote1.clone(), 1000, just_before_range),
        make_token_rate(base1.clone(), quote1.clone(), 1500, just_after_range),
        // 範囲外のデータ（除外されるはず）
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            800,
            two_hours_ago - chrono::Duration::seconds(1),
        ), // 範囲開始直前
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            2000,
            now + chrono::Duration::seconds(1),
        ), // 範囲終了直後
    ];

    TokenRate::batch_insert(&boundary_test_data).await?;

    // 境界値テストの結果を検証
    let boundary_results = TokenRate::get_by_volatility_in_time_range(&time_range, &quote1).await?;
    assert_eq!(
        boundary_results.len(),
        1,
        "Should find only 1 token within range"
    );
    assert_eq!(
        boundary_results[0].base.to_string(),
        "eth.token",
        "Token should be eth"
    );

    // 範囲内のデータだけが考慮されていることを確認（最大値1500、最小値1000）
    assert!(
        boundary_results[0].variance > 0,
        "Variance should be greater than 0"
    );

    // クリーンアップ
    clean_table().await?;

    // ケース2: 同一ボラティリティ値の処理
    let same_volatility_data = vec![
        // base1 (eth) - 変動率 50%
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            100,
            two_hours_ago + chrono::Duration::minutes(1),
        ),
        make_token_rate(base1.clone(), quote1.clone(), 150, one_hour_ago),
        // base2 (btc) - 変動率 50%（同じ）
        make_token_rate(
            base2.clone(),
            quote1.clone(),
            200,
            two_hours_ago + chrono::Duration::minutes(1),
        ),
        make_token_rate(base2.clone(), quote1.clone(), 300, one_hour_ago),
    ];

    TokenRate::batch_insert(&same_volatility_data).await?;

    // 同一ボラティリティテストの結果を検証
    let same_volatility_results =
        TokenRate::get_by_volatility_in_time_range(&time_range, &quote1).await?;
    assert_eq!(
        same_volatility_results.len(),
        2,
        "Should find 2 tokens with same volatility"
    );

    // 両方のトークンが同じボラティリティ（50%）を持つことを確認
    let eth_volatility = same_volatility_results[0].variance.clone();
    let btc_volatility = same_volatility_results[1].variance.clone();

    assert!(eth_volatility > 0, "ETH variance should be greater than 0");
    assert!(btc_volatility > 0, "BTC variance should be greater than 0");

    // クリーンアップ
    clean_table().await?;

    // ケース3: 最小レートが0の場合（HAVING MIN(rate) > 0により除外されるケース）
    let zero_max_rate_data = vec![
        // base1: 負の値(-10)と0の値 → MIN(rate) = -10 > 0 が false なので除外される
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            -10,
            two_hours_ago + chrono::Duration::minutes(1),
        ),
        make_token_rate(base1.clone(), quote1.clone(), 0, one_hour_ago), // MIN(rate) = -10 (negative) なので除外
        // base2: 全て正の値 → MIN(rate) = 5 > 0 が true なので含まれる
        make_token_rate(
            base2.clone(),
            quote1.clone(),
            5,
            two_hours_ago + chrono::Duration::minutes(1),
        ),
        make_token_rate(base2.clone(), quote1.clone(), 10, one_hour_ago),
    ];

    TokenRate::batch_insert(&zero_max_rate_data).await?;

    // 0レートが除外されるケースの結果を検証
    let zero_max_results = TokenRate::get_by_volatility_in_time_range(&time_range, &quote1).await?;
    assert_eq!(
        zero_max_results.len(),
        1,
        "Should find 1 token (base1 excluded due to MIN(rate) <= 0)"
    );

    // base2のみが残り、MIN(rate) = 5 > 0 なので含まれる
    let btc_result = &zero_max_results[0];
    assert_eq!(
        btc_result.base.to_string(),
        "btc.token",
        "Only btc.token should remain"
    );
    assert!(
        btc_result.variance > 0,
        "BTC variance should be greater than 0"
    );

    // クリーンアップ
    clean_table().await?;

    // ケース4: 1つのレコードのみの場合（ボラティリティ = 0）
    let single_record_data = vec![make_token_rate(
        base1.clone(),
        quote1.clone(),
        100,
        one_hour_ago,
    )];

    TokenRate::batch_insert(&single_record_data).await?;

    // 1つのレコードのみの場合の結果を検証
    let single_record_results =
        TokenRate::get_by_volatility_in_time_range(&time_range, &quote1).await?;
    assert_eq!(single_record_results.len(), 1, "Should find 1 token");
    assert_eq!(
        single_record_results[0].base.to_string(),
        "eth.token",
        "Token should be eth"
    );
    assert_eq!(
        single_record_results[0].variance,
        BigDecimal::from(0),
        "Variance should be 0"
    );

    // クリーンアップ
    clean_table().await?;

    // ケース5: 異なる quote トークンのフィルタリング
    let different_quote_data = vec![
        // quote1用のデータ
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            100,
            two_hours_ago + chrono::Duration::minutes(1),
        ),
        make_token_rate(base1.clone(), quote1.clone(), 150, one_hour_ago),
        // quote2用のデータ（フィルタリングされるはず）
        make_token_rate(
            base1.clone(),
            quote2.clone(),
            200,
            two_hours_ago + chrono::Duration::minutes(1),
        ),
        make_token_rate(base1.clone(), quote2.clone(), 400, one_hour_ago), // 変動率100%だが、quote1でフィルタリングされる
    ];

    TokenRate::batch_insert(&different_quote_data).await?;

    // 異なるquoteトークンのフィルタリングテストの結果を検証
    let quote_filter_results =
        TokenRate::get_by_volatility_in_time_range(&time_range, &quote1).await?;
    assert_eq!(
        quote_filter_results.len(),
        1,
        "Should find only 1 token for quote1"
    );
    assert_eq!(
        quote_filter_results[0].base.to_string(),
        "eth.token",
        "Token should be eth"
    );

    // quote1のデータだけが考慮されていることを確認（最大値150、最小値100）
    assert!(
        quote_filter_results[0].variance > 0,
        "Variance should be greater than 0"
    );

    // クリーンアップ
    clean_table().await?;

    // ケース6: 負の値やゼロレートの混在（HAVING MIN(rate) > 0による除外テスト）
    let mixed_rates_data = vec![
        // base1: 負の値、ゼロ、正の値を含む → MIN(rate) = -10 < 0 なので除外される
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            -10,
            two_hours_ago + chrono::Duration::minutes(10),
        ), // 確実に範囲内
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            0,
            two_hours_ago + chrono::Duration::minutes(20),
        ), // MIN(rate) = -10 なので除外
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            10,
            two_hours_ago + chrono::Duration::minutes(30),
        ), // 確実に範囲内
        // base2: 全て正の値 → MIN(rate) = 5 > 0 なので含まれる
        make_token_rate(
            base2.clone(),
            quote1.clone(),
            5,
            two_hours_ago + chrono::Duration::minutes(40),
        ), // 確実に範囲内
        make_token_rate(
            base2.clone(),
            quote1.clone(),
            15,
            two_hours_ago + chrono::Duration::minutes(50),
        ), // 確実に範囲内
    ];

    TokenRate::batch_insert(&mixed_rates_data).await?;

    // 混在レートテストの結果を検証（base1は除外される）
    let mixed_rates_results =
        TokenRate::get_by_volatility_in_time_range(&time_range, &quote1).await?;
    assert_eq!(mixed_rates_results.len(), 1, "Should find 1 token");
    assert_eq!(
        mixed_rates_results[0].base.to_string(),
        "btc.token",
        "Token should be btc"
    );

    // base2のみが結果に含まれることを確認（最大値15、最小値5）
    assert!(
        mixed_rates_results[0].variance > 0,
        "Variance should be greater than 0"
    );

    // クリーンアップ
    clean_table().await?;

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_rate_difference_calculation() -> Result<()> {
    // 1. テーブルの全レコード削除
    clean_table().await?;

    // テスト用のトークンアカウント作成
    let base1: TokenOutAccount = TokenAccount::from_str("eth.token")?.into();
    let quote1: TokenInAccount = TokenAccount::from_str("usdt.token")?.into();

    // 2. タイムスタンプを設定
    let now = chrono::Utc::now().naive_utc();
    let one_hour_ago = now - chrono::Duration::hours(1);
    let two_hours_ago = now - chrono::Duration::hours(2);

    // 3. 時間範囲を設定
    let time_range = crate::TimeRange {
        start: two_hours_ago,
        end: now,
    };

    // ケース1: 通常の計算 - 正の値
    let normal_data = vec![
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            1000,
            two_hours_ago + chrono::Duration::minutes(30),
        ),
        make_token_rate(base1.clone(), quote1.clone(), 1500, one_hour_ago),
    ];

    TokenRate::batch_insert(&normal_data).await?;

    // 通常の計算結果を検証
    let normal_results = TokenRate::get_by_volatility_in_time_range(&time_range, &quote1).await?;
    assert_eq!(normal_results.len(), 1, "Should find 1 token");
    assert_eq!(
        normal_results[0].base.to_string(),
        "eth.token",
        "Token should be eth"
    );

    // rate_difference = MAX(rate) - MIN(rate) = 1500 - 1000 = 500
    assert!(
        normal_results[0].variance > 0,
        "Variance should be greater than 0"
    );

    // クリーンアップ
    clean_table().await?;

    // ケース2: 負の値を含む計算（HAVING MIN(rate) > 0により除外される）
    let negative_data = vec![
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            10,
            two_hours_ago + chrono::Duration::minutes(1),
        ), // MIN(rate) = 10 > 0 なので含まれる
        make_token_rate(base1.clone(), quote1.clone(), 100, one_hour_ago),
    ];

    TokenRate::batch_insert(&negative_data).await?;

    // 正の値の計算結果を検証
    let positive_results = TokenRate::get_by_volatility_in_time_range(&time_range, &quote1).await?;
    assert_eq!(positive_results.len(), 1, "Should find 1 token");
    assert_eq!(
        positive_results[0].base.to_string(),
        "eth.token",
        "Token should be eth"
    );

    // rate_difference = MAX(rate) - MIN(rate) = 100 - 10 = 90
    assert!(
        positive_results[0].variance > 0,
        "Variance should be greater than 0"
    );

    // クリーンアップ
    clean_table().await?;

    // ケース3: 同一値の計算
    let same_value_data = vec![
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            100,
            two_hours_ago + chrono::Duration::minutes(30),
        ),
        make_token_rate(base1.clone(), quote1.clone(), 100, one_hour_ago),
    ];

    TokenRate::batch_insert(&same_value_data).await?;

    // 同一値の計算結果を検証
    let same_value_results =
        TokenRate::get_by_volatility_in_time_range(&time_range, &quote1).await?;
    assert_eq!(same_value_results.len(), 1, "Should find 1 token");
    assert_eq!(
        same_value_results[0].base.to_string(),
        "eth.token",
        "Token should be eth"
    );

    // rate_difference = MAX(rate) - MIN(rate) = 100 - 100 = 0
    assert_eq!(
        same_value_results[0].variance,
        BigDecimal::from(0),
        "Variance should be 0"
    );

    // クリーンアップ
    clean_table().await?;

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_cleanup_old_records() -> Result<()> {
    use crate::TimeRange;

    // 1. テーブルの全レコード削除
    clean_table().await?;

    // テスト用のトークンアカウント作成
    let base1: TokenOutAccount = TokenAccount::from_str("eth.token")?.into();
    let base2: TokenOutAccount = TokenAccount::from_str("btc.token")?.into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token")?.into();

    // 2. 異なる時期のレコードを作成
    let now = chrono::Utc::now().naive_utc();
    let days_400_ago = now - chrono::Duration::days(400);
    let days_200_ago = now - chrono::Duration::days(200);
    let days_100_ago = now - chrono::Duration::days(100);
    let days_10_ago = now - chrono::Duration::days(10);

    let old_rates = vec![
        // 400日前のレコード（削除されるはず）
        make_token_rate(base1.clone(), quote.clone(), 1000, days_400_ago),
        // 200日前のレコード（残るはず - 365日以内）
        make_token_rate(base1.clone(), quote.clone(), 1100, days_200_ago),
        // 100日前のレコード（残るはず）
        make_token_rate(base1.clone(), quote.clone(), 1200, days_100_ago),
        // 10日前のレコード（残るはず）
        make_token_rate(base1.clone(), quote.clone(), 1300, days_10_ago),
        // 今のレコード（残るはず）
        make_token_rate(base1.clone(), quote.clone(), 1400, now),
        // 別のトークンペア - 400日前（削除されるはず）
        make_token_rate(base2.clone(), quote.clone(), 20000, days_400_ago),
        // 別のトークンペア - 今（残るはず）
        make_token_rate(base2.clone(), quote.clone(), 21000, now),
    ];

    // 3. レコードを挿入（cleanup_old_recordsは自動で呼ばれ、デフォルトで365日より古いレコードが削除される）
    TokenRate::batch_insert(&old_rates).await?;

    // 4. 残っているレコード数を確認
    let wide_range = TimeRange {
        start: days_400_ago - chrono::Duration::days(1),
        end: now + chrono::Duration::days(1),
    };
    let mut history1 =
        TokenRate::get_rates_in_time_range(&wide_range, &base1, &quote, test_get_decimals())
            .await?;
    let history2 =
        TokenRate::get_rates_in_time_range(&wide_range, &base2, &quote, test_get_decimals())
            .await?;
    // 新しい順に並び替え
    history1.reverse();

    // base1: 200日前、100日前、10日前、今の4件が残る（全て365日以内）
    assert_eq!(
        history1.len(),
        4,
        "Should have 4 records for base1 (within 365 days)"
    );

    // base2: 今の1件が残る
    assert_eq!(
        history2.len(),
        1,
        "Should have 1 record for base2 (within 365 days)"
    );

    // 5. 残っているレコードのタイムスタンプを確認
    // 新しい順にソートされているので、最初が最新
    assert!(
        history1[0].timestamp >= days_10_ago,
        "Most recent record should be recent"
    );

    // デバッグ情報：レコードの詳細を確認
    println!("Debug: history1 records:");
    for (i, record) in history1.iter().enumerate() {
        println!(
            "  [{}] rate={}, timestamp={}",
            i,
            record.exchange_rate.raw_rate(),
            record.timestamp
        );
    }
    println!("Expected timestamps:");
    println!("  now        = {}", now);
    println!("  10 days ago = {}", days_10_ago);
    println!("  100 days ago = {}", days_100_ago);
    println!("  200 days ago = {}", days_200_ago);
    println!("  400 days ago = {}", days_400_ago);

    // 最も古いレコードは200日前のもの（index 3）
    // レートで確認（200日前のレコードは rate = 1100）
    assert_eq!(
        history1[3].exchange_rate.raw_rate(),
        &BigDecimal::from(1100),
        "Oldest retained record should have rate 1100 (200 days old record)"
    );

    // タイムスタンプも確認（精度の問題を考慮して、200日前から少し前後する範囲を許容）
    let timestamp_diff = if history1[3].timestamp > days_200_ago {
        history1[3].timestamp - days_200_ago
    } else {
        days_200_ago - history1[3].timestamp
    };

    assert!(
        timestamp_diff < chrono::Duration::seconds(1),
        "Oldest retained record timestamp should be close to 200 days ago. \
         Expected: {}, Actual: {}, Diff: {:?}",
        days_200_ago,
        history1[3].timestamp,
        timestamp_diff
    );

    // 400日前のレコードは削除されているので、200日前のレコードより新しい
    assert!(
        history1[3].timestamp > days_400_ago,
        "Oldest retained record should be newer than 400 days ago (400 days old records should be deleted)"
    );

    // クリーンアップ
    clean_table().await?;

    // 6. カスタム保持期間のテスト（30日）
    let recent_rates = vec![
        make_token_rate(
            base1.clone(),
            quote.clone(),
            1000,
            now - chrono::Duration::days(100),
        ),
        make_token_rate(
            base1.clone(),
            quote.clone(),
            1100,
            now - chrono::Duration::days(50),
        ),
        make_token_rate(
            base1.clone(),
            quote.clone(),
            1200,
            now - chrono::Duration::days(20),
        ),
        make_token_rate(
            base1.clone(),
            quote.clone(),
            1300,
            now - chrono::Duration::days(5),
        ),
    ];

    // まず全件挿入（デフォルトのクリーンアップで全件残る）
    TokenRate::batch_insert(&recent_rates).await?;

    // 全件残っていることを確認
    let all_history =
        TokenRate::get_rates_in_time_range(&wide_range, &base1, &quote, test_get_decimals())
            .await?;
    assert_eq!(all_history.len(), 4, "Should have all 4 records initially");

    // 7. 30日でクリーンアップを実行
    TokenRate::cleanup_old_records(30).await?;

    // 8. 30日以内のレコードだけが残っていることを確認
    let mut recent_history =
        TokenRate::get_rates_in_time_range(&wide_range, &base1, &quote, test_get_decimals())
            .await?;
    // 新しい順に並び替え
    recent_history.reverse();
    assert_eq!(
        recent_history.len(),
        2,
        "Should have 2 records (within 30 days)"
    );

    // 残っているレコードが20日前と5日前のものであることを確認
    assert!(
        recent_history[0].timestamp >= now - chrono::Duration::days(6),
        "Most recent should be ~5 days old"
    );
    assert!(
        recent_history[1].timestamp >= now - chrono::Duration::days(21),
        "Second should be ~20 days old"
    );

    // クリーンアップ
    clean_table().await?;

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_get_rates_for_multiple_tokens() -> Result<()> {
    use crate::TimeRange;

    clean_table().await?;

    // テスト用トークン
    let base1: TokenOutAccount = TokenAccount::from_str("token1.near")?.into();
    let base2: TokenOutAccount = TokenAccount::from_str("token2.near")?.into();
    let base3: TokenOutAccount = TokenAccount::from_str("token3.near")?.into();
    let quote: TokenInAccount = TokenAccount::from_str("wrap.near")?.into();

    let now = chrono::Utc::now().naive_utc();
    let one_hour_ago = now - chrono::Duration::hours(1);

    // 各トークンに2件ずつデータを挿入
    let rates = vec![
        make_token_rate(base1.clone(), quote.clone(), 100, one_hour_ago),
        make_token_rate(base1.clone(), quote.clone(), 110, now),
        make_token_rate(base2.clone(), quote.clone(), 200, one_hour_ago),
        make_token_rate(base2.clone(), quote.clone(), 220, now),
        make_token_rate(base3.clone(), quote.clone(), 300, one_hour_ago),
        make_token_rate(base3.clone(), quote.clone(), 330, now),
    ];
    TokenRate::batch_insert(&rates).await?;

    let time_range = TimeRange {
        start: one_hour_ago - chrono::Duration::minutes(1),
        end: now + chrono::Duration::minutes(1),
    };

    // 2トークンのみ取得
    let tokens = vec!["token1.near".to_string(), "token2.near".to_string()];
    let result =
        TokenRate::get_rates_for_multiple_tokens(&tokens, &quote, &time_range, test_get_decimals())
            .await?;

    assert_eq!(result.len(), 2, "Should return 2 tokens");
    assert!(result.contains_key("token1.near"), "Should contain token1");
    assert!(result.contains_key("token2.near"), "Should contain token2");
    assert!(
        !result.contains_key("token3.near"),
        "Should not contain token3"
    );

    // 各トークンに2件のデータがあることを確認
    assert_eq!(result["token1.near"].len(), 2);
    assert_eq!(result["token2.near"].len(), 2);

    // 時系列順（昇順）であることを確認
    assert!(result["token1.near"][0].timestamp < result["token1.near"][1].timestamp);

    clean_table().await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_get_rates_for_multiple_tokens_empty() -> Result<()> {
    use crate::TimeRange;

    clean_table().await?;

    let quote: TokenInAccount = TokenAccount::from_str("wrap.near")?.into();
    let now = chrono::Utc::now().naive_utc();

    let time_range = TimeRange {
        start: now - chrono::Duration::hours(1),
        end: now,
    };

    // 存在しないトークン
    let tokens = vec![
        "nonexistent1.near".to_string(),
        "nonexistent2.near".to_string(),
    ];
    let result =
        TokenRate::get_rates_for_multiple_tokens(&tokens, &quote, &time_range, test_get_decimals())
            .await?;

    assert!(
        result.is_empty(),
        "Should return empty map for nonexistent tokens"
    );

    clean_table().await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_get_rates_for_multiple_tokens_empty_input() -> Result<()> {
    use crate::TimeRange;

    let quote: TokenInAccount = TokenAccount::from_str("wrap.near")?.into();
    let now = chrono::Utc::now().naive_utc();

    let time_range = TimeRange {
        start: now - chrono::Duration::hours(1),
        end: now,
    };

    // 空のトークンリスト
    let tokens: Vec<String> = vec![];
    let result =
        TokenRate::get_rates_for_multiple_tokens(&tokens, &quote, &time_range, test_get_decimals())
            .await?;

    assert!(result.is_empty(), "Should return empty map for empty input");

    Ok(())
}

#[test]
fn test_to_spot_rate_without_path() {
    // swap_path が None の場合、元のレートがそのまま返る
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    let token_rate = make_token_rate(base, quote, 1000, timestamp);
    let spot_rate = token_rate.to_spot_rate();

    assert_eq!(
        spot_rate.raw_rate(),
        token_rate.exchange_rate.raw_rate(),
        "Spot rate should equal original rate when swap_path is None"
    );
}

#[test]
fn test_to_spot_rate_with_path() {
    // swap_path がある場合、補正されたレートが返る
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    // プールサイズ: 10,000 NEAR = 10^28 yocto
    // rate_calc_near: 10 NEAR
    // 補正係数: 1 + (10 * 10^24) / (10 * 10^27) = 1.001 (+0.1%)
    let pool_amount_yocto = "10000000000000000000000000000"; // 10,000 NEAR in yocto
    let swap_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 123,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: pool_amount_yocto.to_string(),
            amount_out: "5000000000000000000000000000".to_string(), // 5,000 NEAR in yocto
        }],
    };

    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10, // 10 NEAR
        swap_path: Some(swap_path),
    };

    let spot_rate = token_rate.to_spot_rate();

    // 補正係数: 1 + (10 * 10^24) / (10^28) = 1 + 10^-3 = 1.001
    // 期待値: 1000 * 1.001 = 1001
    let expected = BigDecimal::from_str("1001").unwrap();
    assert_eq!(
        spot_rate.raw_rate(),
        &expected,
        "Spot rate should be corrected by slippage factor"
    );
}

#[test]
fn test_to_spot_rate_with_small_pool() {
    // 小さいプールの場合、補正が大きくなる
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    // プールサイズ: 100 NEAR = 10^26 yocto
    // rate_calc_near: 10 NEAR
    // 補正係数: 1 + (10 * 10^24) / (10^26) = 1.1 (+10%)
    let pool_amount_yocto = "100000000000000000000000000"; // 100 NEAR in yocto
    let swap_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 456,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: pool_amount_yocto.to_string(),
            amount_out: "50000000000000000000000000".to_string(), // 50 NEAR in yocto
        }],
    };

    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10, // 10 NEAR
        swap_path: Some(swap_path),
    };

    let spot_rate = token_rate.to_spot_rate();

    // 補正係数: 1 + (10 * 10^24) / (10^26) = 1.1
    // 期待値: 1000 * 1.1 = 1100
    let expected = BigDecimal::from_str("1100").unwrap();
    assert_eq!(
        spot_rate.raw_rate(),
        &expected,
        "Spot rate should be corrected by larger slippage factor for small pool"
    );
}

#[test]
fn test_to_spot_rate_with_empty_pools() {
    // pools が空の場合、元のレートがそのまま返る
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    let swap_path = SwapPath { pools: vec![] };

    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10,
        swap_path: Some(swap_path),
    };

    let spot_rate = token_rate.to_spot_rate();

    assert_eq!(
        spot_rate.raw_rate(),
        token_rate.exchange_rate.raw_rate(),
        "Spot rate should equal original rate when pools is empty"
    );
}

#[test]
fn test_to_spot_rate_with_zero_pool_amount() {
    // プールサイズが 0 の場合、元のレートがそのまま返る
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    let swap_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 789,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "0".to_string(),
            amount_out: "1000".to_string(),
        }],
    };

    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10,
        swap_path: Some(swap_path),
    };

    let spot_rate = token_rate.to_spot_rate();

    assert_eq!(
        spot_rate.raw_rate(),
        token_rate.exchange_rate.raw_rate(),
        "Spot rate should equal original rate when pool amount is zero"
    );
}

#[test]
fn test_to_spot_rate_with_fallback_uses_fallback() {
    // swap_path が None の場合、フォールバックパスを使用して補正される
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    // swap_path なしのレート
    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10, // 10 NEAR
        swap_path: None,
    };

    // フォールバック用の swap_path
    // プールサイズ: 100 NEAR = 10^26 yocto
    // 補正係数: 1 + (10 * 10^24) / (10^26) = 1.1 (+10%)
    let fallback_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 456,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "100000000000000000000000000".to_string(), // 100 NEAR in yocto
            amount_out: "50000000000000000000000000".to_string(),
        }],
    };

    let spot_rate = token_rate.to_spot_rate_with_fallback(Some(&fallback_path));

    // 補正係数: 1.1
    // 期待値: 1000 * 1.1 = 1100
    let expected = BigDecimal::from_str("1100").unwrap();
    assert_eq!(
        spot_rate.raw_rate(),
        &expected,
        "Spot rate should be corrected using fallback path"
    );
}

#[test]
fn test_to_spot_rate_with_fallback_prefers_own_path() {
    // 自身の swap_path がある場合、フォールバックは使用されない
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    // プールサイズ: 10,000 NEAR = 10^28 yocto (補正係数 1.001)
    let own_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 123,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "10000000000000000000000000000".to_string(), // 10,000 NEAR in yocto
            amount_out: "5000000000000000000000000000".to_string(),
        }],
    };

    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10,
        swap_path: Some(own_path),
    };

    // フォールバック（補正係数 1.1）- 使用されないはず
    let fallback_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 456,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "100000000000000000000000000".to_string(), // 100 NEAR
            amount_out: "50000000000000000000000000".to_string(),
        }],
    };

    let spot_rate = token_rate.to_spot_rate_with_fallback(Some(&fallback_path));

    // 自身のパスで補正 (1.001): 1000 * 1.001 = 1001
    let expected = BigDecimal::from_str("1001").unwrap();
    assert_eq!(
        spot_rate.raw_rate(),
        &expected,
        "Spot rate should use own path, not fallback"
    );
}

#[test]
fn test_to_spot_rate_with_fallback_no_fallback() {
    // swap_path が None でフォールバックもない場合、元のレートが返る
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10,
        swap_path: None,
    };

    let spot_rate = token_rate.to_spot_rate_with_fallback(None);

    assert_eq!(
        spot_rate.raw_rate(),
        token_rate.exchange_rate.raw_rate(),
        "Spot rate should equal original rate when no path and no fallback"
    );
}

/// find_fallback_path のロジックをテスト
/// 「自分より新しくもっとも古い」swap_path を返すことを確認
#[test]
fn test_find_fallback_path_returns_nearest_newer() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    // 異なる pool_id を持つ swap_path を作成（区別できるように）
    let make_path = |pool_id: u32| SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "100000000000000000000000000".to_string(),
            amount_out: "50000000000000000000000000".to_string(),
        }],
    };

    // 時系列順（古い → 新しい）のレート
    // r0: 4時間前, swap_path=None
    // r1: 3時間前, swap_path=None
    // r2: 2時間前, swap_path=Some(pool_id=200)  ← r0, r1 のフォールバック
    // r3: 1時間前, swap_path=None
    // r4: 今,      swap_path=Some(pool_id=400)  ← r3 のフォールバック
    let rates = vec![
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(4),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(3),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(2),
            rate_calc_near: 10,
            swap_path: Some(make_path(200)),
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(1),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now,
            rate_calc_near: 10,
            swap_path: Some(make_path(400)),
        },
    ];

    // 各レートに対してフォールバックを検索
    // find_fallback_path: 自分より新しくもっとも古い swap_path を返す
    for (i, _rate) in rates.iter().enumerate() {
        let fallback = TokenRate::find_fallback_path(&rates, i);

        match i {
            0 | 1 => {
                // r0, r1 → r2 (pool_id=200) がフォールバック
                assert!(fallback.is_some(), "r{} should have fallback", i);
                assert_eq!(
                    fallback.unwrap().pools[0].pool_id,
                    200,
                    "r{} should use r2's path (pool_id=200)",
                    i
                );
            }
            2 => {
                // r2 は自身が swap_path を持つのでフォールバック不要（None を返す）
                assert!(fallback.is_none(), "r2 has own path, no fallback needed");
            }
            3 => {
                // r3 → r4 (pool_id=400) がフォールバック
                assert!(fallback.is_some(), "r3 should have fallback");
                assert_eq!(
                    fallback.unwrap().pools[0].pool_id,
                    400,
                    "r3 should use r4's path (pool_id=400)"
                );
            }
            4 => {
                // r4 は自身が swap_path を持つのでフォールバック不要
                assert!(fallback.is_none(), "r4 has own path, no fallback needed");
            }
            _ => unreachable!(),
        }
    }
}

/// 自分が swap_path を持つ場合、フォールバックではなく自分の path が使われることを確認
#[test]
fn test_spot_rate_uses_own_path_not_fallback() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    // 異なる補正係数を持つ swap_path を作成
    // 自身の path: 100 NEAR → 補正係数 1.1 (+10%)
    let own_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 100,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "100000000000000000000000000".to_string(), // 100 NEAR
            amount_out: "50000000000000000000000000".to_string(),
        }],
    };

    // フォールバック候補の path: 1000 NEAR → 補正係数 1.01 (+1%)
    let fallback_candidate_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 200,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "1000000000000000000000000000".to_string(), // 1000 NEAR
            amount_out: "500000000000000000000000000".to_string(),
        }],
    };

    // 時系列順のレート配列
    // r0: swap_path=own_path (100)
    // r1: swap_path=fallback_candidate_path (200) ← これがフォールバック候補だが使われない
    let rates = vec![
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(1),
            rate_calc_near: 10,
            swap_path: Some(own_path.clone()),
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now,
            rate_calc_near: 10,
            swap_path: Some(fallback_candidate_path),
        },
    ];

    // r0 のフォールバックを検索 → 自身が path を持つので None
    let fallback = TokenRate::find_fallback_path(&rates, 0);
    assert!(
        fallback.is_none(),
        "find_fallback_path should return None when rate has own path"
    );

    // スポットレートを計算
    let spot_rate = rates[0].to_spot_rate_with_fallback(fallback);

    // 自身の path (100 NEAR) で補正されるので、補正係数は 1.1
    // 1000 * 1.1 = 1100
    let expected = BigDecimal::from_str("1100").unwrap();
    assert_eq!(
        spot_rate.raw_rate(),
        &expected,
        "Should use own path (1.1 correction), not fallback (1.01 correction)"
    );

    // 比較: もしフォールバック (1000 NEAR, 補正係数 1.01) を使った場合
    // 1000 * 1.01 = 1010 になるはず
    let wrong_rate = BigDecimal::from_str("1010").unwrap();
    assert_ne!(
        spot_rate.raw_rate(),
        &wrong_rate,
        "Should NOT be 1010 (fallback's correction)"
    );
}

/// 全てのレートが swap_path を持たない場合、フォールバックは None
#[test]
fn test_find_fallback_path_all_none() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    let rates = vec![
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(2),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(1),
            rate_calc_near: 10,
            swap_path: None,
        },
    ];

    for i in 0..rates.len() {
        let fallback = TokenRate::find_fallback_path(&rates, i);
        assert!(fallback.is_none(), "No fallback when all paths are None");
    }
}

/// precompute_fallback_indices のテスト：基本ケース
/// find_fallback_path と同じ結果を返すことを確認
#[test]
fn test_precompute_fallback_indices_basic() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    let make_path = |pool_id: u32| SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "100000000000000000000000000".to_string(),
            amount_out: "50000000000000000000000000".to_string(),
        }],
    };

    // 時系列順（古い → 新しい）のレート
    // r0: swap_path=None       → フォールバック=r2 (index=2)
    // r1: swap_path=None       → フォールバック=r2 (index=2)
    // r2: swap_path=Some(200)  → フォールバック=None
    // r3: swap_path=None       → フォールバック=r4 (index=4)
    // r4: swap_path=Some(400)  → フォールバック=None
    let rates = vec![
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(4),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(3),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(2),
            rate_calc_near: 10,
            swap_path: Some(make_path(200)),
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(1),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now,
            rate_calc_near: 10,
            swap_path: Some(make_path(400)),
        },
    ];

    let fallbacks = TokenRate::precompute_fallback_indices(&rates);

    assert_eq!(fallbacks.len(), 5);
    assert_eq!(fallbacks[0], Some(2), "r0 should fallback to r2");
    assert_eq!(fallbacks[1], Some(2), "r1 should fallback to r2");
    assert_eq!(fallbacks[2], None, "r2 has own path, no fallback");
    assert_eq!(fallbacks[3], Some(4), "r3 should fallback to r4");
    assert_eq!(fallbacks[4], None, "r4 has own path, no fallback");

    // find_fallback_path と同じ結果を返すことを確認
    for (i, &fallback_idx) in fallbacks.iter().enumerate() {
        let from_linear = TokenRate::find_fallback_path(&rates, i);
        let from_precompute = fallback_idx
            .and_then(|idx| rates.get(idx))
            .and_then(|r| r.swap_path.as_ref());
        assert_eq!(
            from_linear, from_precompute,
            "precompute should match linear search at index {}",
            i
        );
    }
}

/// precompute_fallback_indices のテスト：全て None のケース
#[test]
fn test_precompute_fallback_indices_all_none() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    let rates = vec![
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(2),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(1),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now,
            rate_calc_near: 10,
            swap_path: None,
        },
    ];

    let fallbacks = TokenRate::precompute_fallback_indices(&rates);

    assert_eq!(fallbacks.len(), 3);
    for (i, fallback) in fallbacks.iter().enumerate() {
        assert!(
            fallback.is_none(),
            "No fallback when all paths are None at index {}",
            i
        );
    }
}

/// precompute_fallback_indices のテスト：全て Some のケース
#[test]
fn test_precompute_fallback_indices_all_some() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    let make_path = |pool_id: u32| SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "100000000000000000000000000".to_string(),
            amount_out: "50000000000000000000000000".to_string(),
        }],
    };

    let rates = vec![
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(2),
            rate_calc_near: 10,
            swap_path: Some(make_path(100)),
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(1),
            rate_calc_near: 10,
            swap_path: Some(make_path(200)),
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now,
            rate_calc_near: 10,
            swap_path: Some(make_path(300)),
        },
    ];

    let fallbacks = TokenRate::precompute_fallback_indices(&rates);

    assert_eq!(fallbacks.len(), 3);
    for (i, fallback) in fallbacks.iter().enumerate() {
        assert!(
            fallback.is_none(),
            "No fallback needed when rate has own path at index {}",
            i
        );
    }
}

/// precompute_fallback_indices のテスト：空の配列
#[test]
fn test_precompute_fallback_indices_empty() {
    let rates: Vec<TokenRate> = vec![];
    let fallbacks = TokenRate::precompute_fallback_indices(&rates);
    assert!(fallbacks.is_empty());
}

/// precompute_fallback_indices のテスト：単一要素
#[test]
fn test_precompute_fallback_indices_single() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    // swap_path なし → フォールバックなし（後続がない）
    let rates_none = vec![TokenRate {
        base: base.clone(),
        quote: quote.clone(),
        exchange_rate: make_rate(1000),
        timestamp: now,
        rate_calc_near: 10,
        swap_path: None,
    }];

    let fallbacks = TokenRate::precompute_fallback_indices(&rates_none);
    assert_eq!(fallbacks.len(), 1);
    assert!(fallbacks[0].is_none());

    // swap_path あり → フォールバック不要
    let rates_some = vec![TokenRate {
        base: base.clone(),
        quote: quote.clone(),
        exchange_rate: make_rate(1000),
        timestamp: now,
        rate_calc_near: 10,
        swap_path: Some(SwapPath {
            pools: vec![SwapPoolInfo {
                pool_id: 100,
                token_in_idx: 0,
                token_out_idx: 1,
                amount_in: "100000000000000000000000000".to_string(),
                amount_out: "50000000000000000000000000".to_string(),
            }],
        }),
    }];

    let fallbacks = TokenRate::precompute_fallback_indices(&rates_some);
    assert_eq!(fallbacks.len(), 1);
    assert!(fallbacks[0].is_none());
}

/// precompute_fallback_indices のテスト：先頭のみ swap_path がある場合
/// 先頭要素は自身が path を持つのでフォールバック不要、
/// 後続の要素は全てフォールバックなし（先頭より前に path がない）
#[test]
fn test_precompute_fallback_indices_first_only() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    let make_path = |pool_id: u32| SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "100000000000000000000000000".to_string(),
            amount_out: "50000000000000000000000000".to_string(),
        }],
    };

    // r0: swap_path=Some → フォールバック不要
    // r1: swap_path=None → フォールバックなし（r0より後ろに path がない）
    // r2: swap_path=None → フォールバックなし
    let rates = vec![
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(2),
            rate_calc_near: 10,
            swap_path: Some(make_path(100)),
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(1),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now,
            rate_calc_near: 10,
            swap_path: None,
        },
    ];

    let fallbacks = TokenRate::precompute_fallback_indices(&rates);

    assert_eq!(fallbacks.len(), 3);
    assert_eq!(fallbacks[0], None, "r0 has own path, no fallback");
    assert_eq!(fallbacks[1], None, "r1 has no newer path to fallback to");
    assert_eq!(fallbacks[2], None, "r2 has no newer path to fallback to");

    // find_fallback_path と一致することを確認
    for (i, &fallback_idx) in fallbacks.iter().enumerate() {
        let from_linear = TokenRate::find_fallback_path(&rates, i);
        let from_precompute = fallback_idx
            .and_then(|idx| rates.get(idx))
            .and_then(|r| r.swap_path.as_ref());
        assert_eq!(from_linear, from_precompute);
    }
}

/// precompute_fallback_indices のテスト：末尾のみ swap_path がある場合
/// 全ての先行要素が末尾にフォールバックする
#[test]
fn test_precompute_fallback_indices_last_only() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    let make_path = |pool_id: u32| SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "100000000000000000000000000".to_string(),
            amount_out: "50000000000000000000000000".to_string(),
        }],
    };

    // r0: swap_path=None → r2 にフォールバック
    // r1: swap_path=None → r2 にフォールバック
    // r2: swap_path=Some → フォールバック不要
    let rates = vec![
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(2),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(1),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now,
            rate_calc_near: 10,
            swap_path: Some(make_path(300)),
        },
    ];

    let fallbacks = TokenRate::precompute_fallback_indices(&rates);

    assert_eq!(fallbacks.len(), 3);
    assert_eq!(fallbacks[0], Some(2), "r0 should fallback to r2");
    assert_eq!(fallbacks[1], Some(2), "r1 should fallback to r2");
    assert_eq!(fallbacks[2], None, "r2 has own path, no fallback");

    // find_fallback_path と一致することを確認
    for (i, &fallback_idx) in fallbacks.iter().enumerate() {
        let from_linear = TokenRate::find_fallback_path(&rates, i);
        let from_precompute = fallback_idx
            .and_then(|idx| rates.get(idx))
            .and_then(|r| r.swap_path.as_ref());
        assert_eq!(from_linear, from_precompute);
    }
}

/// 速度比較テスト: precompute_fallback_indices (O(n)) vs find_fallback_path の全呼び出し (O(n²))
/// 大量データでの実行時間を比較し、precompute が明らかに高速であることを確認
#[test]
fn test_precompute_fallback_indices_performance() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    let make_path = |pool_id: u32| SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "100000000000000000000000000".to_string(),
            amount_out: "50000000000000000000000000".to_string(),
        }],
    };

    // 1000件のレートを生成（10件に1件 swap_path あり）
    let n = 1000;
    let rates: Vec<TokenRate> = (0..n)
        .map(|i| TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000 + i as i64),
            timestamp: now - chrono::Duration::minutes(n as i64 - i as i64),
            rate_calc_near: 10,
            swap_path: if i % 10 == 9 {
                Some(make_path(i as u32))
            } else {
                None
            },
        })
        .collect();

    // O(n) の事前計算
    let start_precompute = std::time::Instant::now();
    let fallbacks = TokenRate::precompute_fallback_indices(&rates);
    let precompute_duration = start_precompute.elapsed();

    // O(n²) の線形検索（全要素に対して find_fallback_path を呼び出し）
    let start_linear = std::time::Instant::now();
    let linear_results: Vec<Option<&SwapPath>> = (0..rates.len())
        .map(|i| TokenRate::find_fallback_path(&rates, i))
        .collect();
    let linear_duration = start_linear.elapsed();

    // 結果が一致することを確認
    for i in 0..rates.len() {
        let from_precompute = fallbacks[i]
            .and_then(|idx| rates.get(idx))
            .and_then(|r| r.swap_path.as_ref());
        assert_eq!(
            linear_results[i], from_precompute,
            "Results should match at index {}",
            i
        );
    }

    // 事前計算が線形検索より高速であることを確認
    // 注: CI環境での変動を考慮し、10倍以上の差を期待
    // n=1000 の場合: O(n²) ≈ 500,000 比較 vs O(n) ≈ 1,000
    assert!(
        precompute_duration < linear_duration,
        "precompute ({:?}) should be faster than linear search ({:?})",
        precompute_duration,
        linear_duration
    );

    // 実運用での信頼性のため、少なくとも2倍は高速であることを確認
    // （CI環境でのキャッシュ効果等を考慮した保守的な閾値）
    let speedup = linear_duration.as_nanos() as f64 / precompute_duration.as_nanos() as f64;
    assert!(
        speedup >= 2.0,
        "precompute should be at least 2x faster, but speedup was only {:.2}x",
        speedup
    );
}

/// 大規模データでのスケーラビリティテスト
/// データ量が10倍になっても処理時間が線形に増加することを確認
#[test]
fn test_precompute_fallback_indices_scalability() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    let make_path = |pool_id: u32| SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "100000000000000000000000000".to_string(),
            amount_out: "50000000000000000000000000".to_string(),
        }],
    };

    let generate_rates = |n: usize| -> Vec<TokenRate> {
        (0..n)
            .map(|i| TokenRate {
                base: base.clone(),
                quote: quote.clone(),
                exchange_rate: make_rate(1000 + i as i64),
                timestamp: now - chrono::Duration::seconds(n as i64 - i as i64),
                rate_calc_near: 10,
                swap_path: if i % 10 == 9 {
                    Some(make_path(i as u32))
                } else {
                    None
                },
            })
            .collect()
    };

    // ウォームアップ（JIT/キャッシュの影響を軽減）
    let warmup_rates = generate_rates(100);
    let _ = TokenRate::precompute_fallback_indices(&warmup_rates);

    // 小規模 (n=500)
    let small_rates = generate_rates(500);
    let start_small = std::time::Instant::now();
    for _ in 0..10 {
        let _ = TokenRate::precompute_fallback_indices(&small_rates);
    }
    let small_duration = start_small.elapsed();

    // 大規模 (n=5000, 10倍)
    let large_rates = generate_rates(5000);
    let start_large = std::time::Instant::now();
    for _ in 0..10 {
        let _ = TokenRate::precompute_fallback_indices(&large_rates);
    }
    let large_duration = start_large.elapsed();

    // O(n) アルゴリズムなので、データ量が10倍になっても処理時間は約10倍程度のはず
    // 多少のオーバーヘッドを考慮して20倍以下であることを確認
    let ratio = large_duration.as_nanos() as f64 / small_duration.as_nanos() as f64;
    assert!(
        ratio <= 20.0,
        "Processing time should scale linearly (ratio should be ~10x for 10x data), but was {:.2}x",
        ratio
    );
}

// ===========================================================================
// マルチホップ補正テスト
// ===========================================================================

/// シングルホップ: 従来の動作と同一結果を確認
#[test]
fn test_to_spot_rate_multihop_single_hop_same_as_before() {
    // シングルホップの場合、マルチホップ実装と従来実装は同じ結果を返すべき
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    // プールサイズ: 10,000 NEAR = 10^28 yocto
    // rate_calc_near: 10 NEAR
    // 補正係数: 1 + (10 * 10^24) / (10^28) = 1.001
    let swap_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 123,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "10000000000000000000000000000".to_string(), // 10,000 NEAR in yocto
            amount_out: "5000000000000000000000000000".to_string(), // 5,000 NEAR in yocto
        }],
    };

    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10,
        swap_path: Some(swap_path),
    };

    let spot_rate = token_rate.to_spot_rate();

    // 補正係数: 1 + (10 * 10^24) / (10^28) = 1.001
    // 期待値: 1000 * 1.001 = 1001
    let expected = BigDecimal::from_str("1001").unwrap();
    assert_eq!(
        spot_rate.raw_rate(),
        &expected,
        "Single hop should produce same result as before (1001)"
    );
}

/// 2ホップ: 補正係数が積算されることを確認
#[test]
fn test_to_spot_rate_multihop_two_hops() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    // 2ホップスワップ:
    // Hop1: NEAR -> TokenA
    //   - pool_amount_in: 100 NEAR = 10^26 yocto
    //   - pool_amount_out: 200 TokenA
    //   - Δx_0 = 10 NEAR = 10^25 yocto
    //   - 補正1: 1 + 10^25 / 10^26 = 1.1
    //   - Δx_1 = 10^25 * (200 / 100) = 2 * 10^25 (相対的なスケール)
    //
    // Hop2: TokenA -> TokenB
    //   - pool_amount_in: 1000 = 10^3
    //   - pool_amount_out: 500
    //   - 補正2: 1 + Δx_1 / 10^3
    //
    // 簡略化のため、同じスケールで計算:
    // Hop1: in=100, out=200, Δx=10 → correction1 = 1.1, Δx'=10*200/100=20
    // Hop2: in=1000, out=500, Δx'=20 → correction2 = 1.02
    // 総補正 = 1.1 * 1.02 = 1.122
    let swap_path = SwapPath {
        pools: vec![
            SwapPoolInfo {
                pool_id: 1,
                token_in_idx: 0,
                token_out_idx: 1,
                amount_in: "100000000000000000000000000".to_string(), // 100 NEAR in yocto
                amount_out: "200000000000000000000000000".to_string(), // 200 単位
            },
            SwapPoolInfo {
                pool_id: 2,
                token_in_idx: 0,
                token_out_idx: 1,
                amount_in: "1000000000000000000000000000".to_string(), // 1000 NEAR in yocto
                amount_out: "500000000000000000000000000".to_string(), // 500 単位
            },
        ],
    };

    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10, // 10 NEAR
        swap_path: Some(swap_path),
    };

    let spot_rate = token_rate.to_spot_rate();

    // 計算:
    // Δx_0 = 10 * 10^24 yocto
    // Hop1: pool_in = 100 * 10^24, correction1 = (100 + 10) / 100 = 1.1
    //       Δx_1 = 10 * 10^24 * (200 / 100) = 20 * 10^24
    // Hop2: pool_in = 1000 * 10^24, correction2 = (1000 + 20) / 1000 = 1.02
    // 総補正 = 1.1 * 1.02 = 1.122
    // 期待値: 1000 * 1.122 = 1122
    let expected = BigDecimal::from_str("1122").unwrap();
    assert_eq!(
        spot_rate.raw_rate(),
        &expected,
        "Two hop correction should be 1.1 * 1.02 = 1.122, so 1000 * 1.122 = 1122"
    );
}

/// 3ホップ以上: 累積補正の確認
#[test]
fn test_to_spot_rate_multihop_three_hops() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    // 3ホップスワップ:
    // Hop1: in=100, out=100 (1:1) → correction1 = 1.1, Δx'=10
    // Hop2: in=100, out=100 (1:1) → correction2 = 1.1, Δx''=10
    // Hop3: in=100, out=100 (1:1) → correction3 = 1.1
    // 総補正 = 1.1^3 = 1.331
    let swap_path = SwapPath {
        pools: vec![
            SwapPoolInfo {
                pool_id: 1,
                token_in_idx: 0,
                token_out_idx: 1,
                amount_in: "100000000000000000000000000".to_string(), // 100 NEAR
                amount_out: "100000000000000000000000000".to_string(),
            },
            SwapPoolInfo {
                pool_id: 2,
                token_in_idx: 0,
                token_out_idx: 1,
                amount_in: "100000000000000000000000000".to_string(),
                amount_out: "100000000000000000000000000".to_string(),
            },
            SwapPoolInfo {
                pool_id: 3,
                token_in_idx: 0,
                token_out_idx: 1,
                amount_in: "100000000000000000000000000".to_string(),
                amount_out: "100000000000000000000000000".to_string(),
            },
        ],
    };

    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10,
        swap_path: Some(swap_path),
    };

    let spot_rate = token_rate.to_spot_rate();

    // 1.1^3 = 1.331
    // 1000 * 1.331 = 1331
    let expected = BigDecimal::from_str("1331").unwrap();
    assert_eq!(
        spot_rate.raw_rate(),
        &expected,
        "Three hop correction should be 1.1^3 = 1.331, so 1000 * 1.331 = 1331"
    );
}

/// amount_out パース失敗時: 安全にフォールバック（current_delta を維持）
#[test]
fn test_to_spot_rate_multihop_amount_out_parse_failure() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    // Hop1 の amount_out が不正な場合、Δx は維持される
    // Hop1: in=100, out=invalid → correction1 = 1.1, Δx'=Δx (10)
    // Hop2: in=100, out=100 → correction2 = 1.1
    // 総補正 = 1.1 * 1.1 = 1.21
    let swap_path = SwapPath {
        pools: vec![
            SwapPoolInfo {
                pool_id: 1,
                token_in_idx: 0,
                token_out_idx: 1,
                amount_in: "100000000000000000000000000".to_string(),
                amount_out: "invalid_number".to_string(), // 不正な値
            },
            SwapPoolInfo {
                pool_id: 2,
                token_in_idx: 0,
                token_out_idx: 1,
                amount_in: "100000000000000000000000000".to_string(),
                amount_out: "100000000000000000000000000".to_string(),
            },
        ],
    };

    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10,
        swap_path: Some(swap_path),
    };

    let spot_rate = token_rate.to_spot_rate();

    // パース失敗時は Δx を維持するので、両ホップとも同じ Δx で補正
    // 1.1 * 1.1 = 1.21
    // 1000 * 1.21 = 1210
    let expected = BigDecimal::from_str("1210").unwrap();
    assert_eq!(
        spot_rate.raw_rate(),
        &expected,
        "When amount_out parse fails, Δx should be maintained. 1.1 * 1.1 = 1.21"
    );
}

/// amount_in パース失敗時: そのプールの補正をスキップ
#[test]
fn test_to_spot_rate_multihop_amount_in_parse_failure() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    // Hop1 の amount_in が不正な場合、そのプールの補正はスキップ
    // Hop1: in=invalid → スキップ
    // Hop2: in=100, out=100 → correction = 1.1
    // 総補正 = 1.1
    let swap_path = SwapPath {
        pools: vec![
            SwapPoolInfo {
                pool_id: 1,
                token_in_idx: 0,
                token_out_idx: 1,
                amount_in: "invalid_number".to_string(), // 不正な値
                amount_out: "100000000000000000000000000".to_string(),
            },
            SwapPoolInfo {
                pool_id: 2,
                token_in_idx: 0,
                token_out_idx: 1,
                amount_in: "100000000000000000000000000".to_string(),
                amount_out: "100000000000000000000000000".to_string(),
            },
        ],
    };

    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10,
        swap_path: Some(swap_path),
    };

    let spot_rate = token_rate.to_spot_rate();

    // Hop1 はスキップ、Hop2 のみ補正: 1.1
    // 1000 * 1.1 = 1100
    let expected = BigDecimal::from_str("1100").unwrap();
    assert_eq!(
        spot_rate.raw_rate(),
        &expected,
        "When amount_in parse fails, that pool should be skipped. Only hop2: 1.1"
    );
}

/// マルチホップでフォールバックパスを使用する場合
#[test]
fn test_to_spot_rate_multihop_with_fallback() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    // swap_path なしのレート
    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10,
        swap_path: None,
    };

    // フォールバック用の2ホップパス
    // Hop1: in=100, out=200 → correction1 = 1.1, Δx'=20
    // Hop2: in=1000, out=500 → correction2 = 1.02
    // 総補正 = 1.122
    let fallback_path = SwapPath {
        pools: vec![
            SwapPoolInfo {
                pool_id: 1,
                token_in_idx: 0,
                token_out_idx: 1,
                amount_in: "100000000000000000000000000".to_string(),
                amount_out: "200000000000000000000000000".to_string(),
            },
            SwapPoolInfo {
                pool_id: 2,
                token_in_idx: 0,
                token_out_idx: 1,
                amount_in: "1000000000000000000000000000".to_string(),
                amount_out: "500000000000000000000000000".to_string(),
            },
        ],
    };

    let spot_rate = token_rate.to_spot_rate_with_fallback(Some(&fallback_path));

    // 1000 * 1.122 = 1122
    let expected = BigDecimal::from_str("1122").unwrap();
    assert_eq!(
        spot_rate.raw_rate(),
        &expected,
        "Multihop fallback should work: 1.1 * 1.02 = 1.122"
    );
}
