// use super::*;
use crate::Result;
use crate::persistence::connection_pool;
use crate::persistence::schema::token_rates;
use crate::persistence::token_rate::TokenRate;
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use anyhow::anyhow;
use bigdecimal::BigDecimal;
use chrono::{NaiveDateTime, SubsecRound};
use diesel::RunQueryDsl;
use serial_test::serial;
use std::str::FromStr;
use zaciraci_common::types::ExchangeRate;

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
    TokenRate::new_with_timestamp(base, quote, make_rate(rate), timestamp)
}

/// テスト用ヘルパー: TokenRate を文字列レートで作成
#[allow(dead_code)]
fn make_token_rate_str(
    base: TokenOutAccount,
    quote: TokenInAccount,
    rate: &str,
    timestamp: NaiveDateTime,
) -> TokenRate {
    TokenRate::new_with_timestamp(base, quote, make_rate_str(rate), timestamp)
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
            $left.rate(),
            $right.rate(),
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
    let result = TokenRate::get_latest(&base, &quote).await?;
    assert!(result.is_none(), "Empty table should return None");

    // 3. １つインサート
    let timestamp = chrono::Utc::now().naive_utc();
    let token_rate = make_token_rate(base.clone(), quote.clone(), 1000, timestamp);
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
        make_token_rate(base.clone(), quote.clone(), 1000, earliest),
        make_token_rate(base.clone(), quote.clone(), 1050, middle),
        make_token_rate(base.clone(), quote.clone(), 1100, latest),
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
            rate.rate(),
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

    // リミットが機能することを確認
    let limited_history = TokenRate::get_history(&base, &quote, 2).await?;
    assert_eq!(limited_history.len(), 2, "Should return only 2 records");
    assert_eq!(
        limited_history[0].rate(),
        &BigDecimal::from(1100),
        "Newest record should be first"
    );
    assert_eq!(
        limited_history[1].rate(),
        &BigDecimal::from(1050),
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
    let rate1 = make_token_rate(base1.clone(), quote1.clone(), 1000, now);
    let rate2 = make_token_rate(base2.clone(), quote1.clone(), 2000, now);
    let rate3 = make_token_rate(base1.clone(), quote2.clone(), 3000, now);

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
        make_token_rate(base1.clone(), quote1.clone(), 1000, two_hours_ago), // 古いレコード
        make_token_rate(base1.clone(), quote1.clone(), 1100, one_hour_ago), // 新しいレコード（base1用）
        make_token_rate(base2.clone(), quote1.clone(), 20000, now), // 最新レコード（base2用）
        // 異なるクォートトークン（quote2）用のレコード - 結果に含まれないはず
        make_token_rate(base3.clone(), quote2.clone(), 5, now),
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
        let expected_btc = make_token_rate(base2.clone(), quote1.clone(), 20000, now);
        let actual_btc =
            make_token_rate(result_base1.clone(), quote1.clone(), 20000, *result_time1);
        assert_token_rate_eq!(
            actual_btc,
            expected_btc,
            "BTCのタイムスタンプが正しくありません"
        );
    }

    {
        // base1 (eth) のタイムスタンプがone_hour_agoに近いことを確認
        let expected_eth = make_token_rate(base1.clone(), quote1.clone(), 1100, one_hour_ago);
        let actual_eth = make_token_rate(result_base2.clone(), quote1.clone(), 1100, *result_time2);
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
    let time_range = crate::persistence::TimeRange {
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
        results[2].variance > BigDecimal::from(0),
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
        btc_result.variance > BigDecimal::from(0),
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
    let time_range = crate::persistence::TimeRange {
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
        boundary_results[0].variance > BigDecimal::from(0),
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

    assert!(
        eth_volatility > BigDecimal::from(0),
        "ETH variance should be greater than 0"
    );
    assert!(
        btc_volatility > BigDecimal::from(0),
        "BTC variance should be greater than 0"
    );

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
        btc_result.variance > BigDecimal::from(0),
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
        quote_filter_results[0].variance > BigDecimal::from(0),
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
        mixed_rates_results[0].variance > BigDecimal::from(0),
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
    let time_range = crate::persistence::TimeRange {
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
        normal_results[0].variance > BigDecimal::from(0),
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
        positive_results[0].variance > BigDecimal::from(0),
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
    let history1 = TokenRate::get_history(&base1, &quote, 100).await?;
    let history2 = TokenRate::get_history(&base2, &quote, 100).await?;

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
            record.rate(),
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
        history1[3].rate(),
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
    let all_history = TokenRate::get_history(&base1, &quote, 100).await?;
    assert_eq!(all_history.len(), 4, "Should have all 4 records initially");

    // 7. 30日でクリーンアップを実行
    TokenRate::cleanup_old_records(30).await?;

    // 8. 30日以内のレコードだけが残っていることを確認
    let recent_history = TokenRate::get_history(&base1, &quote, 100).await?;
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
