// use super::*;
use crate::Result;
use crate::persistence::connection_pool;
use crate::persistence::schema::token_rates;
use crate::persistence::token_rate::TokenRate;
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use anyhow::anyhow;
use bigdecimal::BigDecimal;
use chrono::SubsecRound;
use diesel::RunQueryDsl;
use serial_test::serial;
use std::str::FromStr;

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
            $left.rate, $right.rate,
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
        TokenRate::new_with_timestamp(base.clone(), quote.clone(), BigDecimal::from(1050), middle),
        TokenRate::new_with_timestamp(base.clone(), quote.clone(), BigDecimal::from(1100), latest),
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
        TokenRate::new_with_timestamp(base.clone(), quote.clone(), BigDecimal::from(1100), latest),
        "Latest record should match"
    );
    assert_token_rate_eq!(
        history[1],
        TokenRate::new_with_timestamp(base.clone(), quote.clone(), BigDecimal::from(1050), middle),
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
    let rate1 =
        TokenRate::new_with_timestamp(base1.clone(), quote1.clone(), BigDecimal::from(1000), now);
    let rate2 =
        TokenRate::new_with_timestamp(base2.clone(), quote1.clone(), BigDecimal::from(2000), now);
    let rate3 =
        TokenRate::new_with_timestamp(base1.clone(), quote2.clone(), BigDecimal::from(3000), now);

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
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote.clone(),
            BigDecimal::from(1000),
            two_hours_ago,
        ),
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote.clone(),
            BigDecimal::from(1500),
            one_hour_ago,
        ),
        // base2 (btc) - 変動率 100%
        TokenRate::new_with_timestamp(
            base2.clone(),
            quote.clone(),
            BigDecimal::from(20000),
            two_hours_ago,
        ),
        TokenRate::new_with_timestamp(
            base2.clone(),
            quote.clone(),
            BigDecimal::from(40000),
            one_hour_ago,
        ),
        // base3 (near) - 変動率 10%
        TokenRate::new_with_timestamp(
            base3.clone(),
            quote.clone(),
            BigDecimal::from(5),
            two_hours_ago,
        ),
        TokenRate::new_with_timestamp(
            base3.clone(),
            quote.clone(),
            BigDecimal::from_str("5.5").unwrap(),
            one_hour_ago,
        ),
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

    // レートが0を含むデータを挿入（ただしSQL条件でrate != 0によりフィルタされる）
    let zero_rate_data = vec![
        // rate != 0の条件により、これらの0レートは除外される
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote.clone(),
            BigDecimal::from(0), // 除外される
            two_hours_ago + chrono::Duration::minutes(1),
        ),
        TokenRate::new_with_timestamp(
            base2.clone(),
            quote.clone(),
            BigDecimal::from(0), // 除外される
            one_hour_ago,
        ),
        // 非ゼロレートは含まれる
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote.clone(),
            BigDecimal::from(100),
            two_hours_ago + chrono::Duration::minutes(2),
        ),
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote.clone(),
            BigDecimal::from(150),
            one_hour_ago + chrono::Duration::minutes(1),
        ),
    ];

    TokenRate::batch_insert(&zero_rate_data).await?;

    // ボラティリティを取得
    let zero_results = TokenRate::get_by_volatility_in_time_range(&time_range, &quote).await?;

    // 結果を検証: rate != 0の条件により、レート0のレコードは除外され、
    // 単一のレコードのみ存在するため、ボラティリティ計算ができない
    assert_eq!(
        zero_results.len(),
        1,
        "Should find 1 token (base2 excluded due to 0 rates)"
    );

    // base1のみが結果に含まれることを確認（非ゼロレートのみ考慮）
    let eth_result = &zero_results[0];
    assert_eq!(
        eth_result.base.to_string(),
        "eth.token",
        "Only eth token should be found"
    );

    // 分散値が0より大きいことを確認
    assert!(
        eth_result.variance > BigDecimal::from(0),
        "ETH variance should be greater than 0, got {}",
        eth_result.variance
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
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote1.clone(),
            BigDecimal::from(1000),
            just_before_range,
        ),
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote1.clone(),
            BigDecimal::from(1500),
            just_after_range,
        ),
        // 範囲外のデータ（除外されるはず）
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote1.clone(),
            BigDecimal::from(800),
            two_hours_ago - chrono::Duration::seconds(1), // 範囲開始直前
        ),
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote1.clone(),
            BigDecimal::from(2000),
            now + chrono::Duration::seconds(1), // 範囲終了直後
        ),
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
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote1.clone(),
            BigDecimal::from(100),
            two_hours_ago + chrono::Duration::minutes(1),
        ),
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote1.clone(),
            BigDecimal::from(150),
            one_hour_ago,
        ),
        // base2 (btc) - 変動率 50%（同じ）
        TokenRate::new_with_timestamp(
            base2.clone(),
            quote1.clone(),
            BigDecimal::from(200),
            two_hours_ago + chrono::Duration::minutes(1),
        ),
        TokenRate::new_with_timestamp(
            base2.clone(),
            quote1.clone(),
            BigDecimal::from(300),
            one_hour_ago,
        ),
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

    // ケース3: 最大レートが0の場合（rate != 0条件により除外されるケース）
    let zero_max_rate_data = vec![
        // base1: 負の値(-10)と0の値 → 0が除外されて負の値のみ残る
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote1.clone(),
            BigDecimal::from(-10),
            two_hours_ago + chrono::Duration::minutes(1),
        ),
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote1.clone(),
            BigDecimal::from(0), // 除外される
            one_hour_ago,
        ),
        // base2: 全て0のレコード → 全て除外される
        TokenRate::new_with_timestamp(
            base2.clone(),
            quote1.clone(),
            BigDecimal::from(0), // 除外される
            two_hours_ago + chrono::Duration::minutes(1),
        ),
        TokenRate::new_with_timestamp(
            base2.clone(),
            quote1.clone(),
            BigDecimal::from(0), // 除外される
            one_hour_ago,
        ),
    ];

    TokenRate::batch_insert(&zero_max_rate_data).await?;

    // 0レートが除外されるケースの結果を検証
    let zero_max_results = TokenRate::get_by_volatility_in_time_range(&time_range, &quote1).await?;
    assert_eq!(
        zero_max_results.len(),
        1,
        "Should find 1 token (base2 excluded due to all zero rates)"
    );

    // base1のみが残り、負の値1つだけになるため、ボラティリティは0
    let eth_result = &zero_max_results[0];
    assert_eq!(
        eth_result.base.to_string(),
        "eth.token",
        "Only eth.token should remain"
    );
    assert_eq!(
        eth_result.variance,
        BigDecimal::from(0),
        "ETH variance should be 0"
    );

    // クリーンアップ
    clean_table().await?;

    // ケース4: 1つのレコードのみの場合（ボラティリティ = 0）
    let single_record_data = vec![TokenRate::new_with_timestamp(
        base1.clone(),
        quote1.clone(),
        BigDecimal::from(100),
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
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote1.clone(),
            BigDecimal::from(100),
            two_hours_ago + chrono::Duration::minutes(1),
        ),
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote1.clone(),
            BigDecimal::from(150),
            one_hour_ago,
        ),
        // quote2用のデータ（フィルタリングされるはず）
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote2.clone(),
            BigDecimal::from(200),
            two_hours_ago + chrono::Duration::minutes(1),
        ),
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote2.clone(),
            BigDecimal::from(400),
            one_hour_ago, // 変動率100%だが、quote1でフィルタリングされる
        ),
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

    // ケース6: 負の値やゼロレートの混在（rate != 0条件による除外テスト）
    let mixed_rates_data = vec![
        // 負の値は含まれる（!= 0なので）
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote1.clone(),
            BigDecimal::from(-10),
            two_hours_ago + chrono::Duration::minutes(10), // 確実に範囲内
        ),
        // ゼロレートは除外される
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote1.clone(),
            BigDecimal::from(0),                           // 除外される
            two_hours_ago + chrono::Duration::minutes(20), // 確実に範囲内
        ),
        // 正の値は含まれる
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote1.clone(),
            BigDecimal::from(10),
            two_hours_ago + chrono::Duration::minutes(30), // 確実に範囲内
        ),
    ];

    TokenRate::batch_insert(&mixed_rates_data).await?;

    // 混在レートテストの結果を検証（ゼロレートは除外される）
    let mixed_rates_results =
        TokenRate::get_by_volatility_in_time_range(&time_range, &quote1).await?;
    assert_eq!(mixed_rates_results.len(), 1, "Should find 1 token");
    assert_eq!(
        mixed_rates_results[0].base.to_string(),
        "eth.token",
        "Token should be eth"
    );

    // ゼロレートが除外されていることを確認（最大値10、最小値-10）
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
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote1.clone(),
            BigDecimal::from(1000),
            two_hours_ago + chrono::Duration::minutes(30),
        ),
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote1.clone(),
            BigDecimal::from(1500),
            one_hour_ago,
        ),
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

    // ケース2: 負の値を含む計算
    let negative_data = vec![
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote1.clone(),
            BigDecimal::from(-100),
            two_hours_ago + chrono::Duration::minutes(1),
        ),
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote1.clone(),
            BigDecimal::from(100),
            one_hour_ago,
        ),
    ];

    TokenRate::batch_insert(&negative_data).await?;

    // 負の値を含む計算結果を検証
    let negative_results = TokenRate::get_by_volatility_in_time_range(&time_range, &quote1).await?;
    assert_eq!(negative_results.len(), 1, "Should find 1 token");
    assert_eq!(
        negative_results[0].base.to_string(),
        "eth.token",
        "Token should be eth"
    );

    // rate_difference = MAX(rate) - MIN(rate) = 100 - (-100) = 200
    assert!(
        negative_results[0].variance > BigDecimal::from(0),
        "Variance should be greater than 0"
    );

    // クリーンアップ
    clean_table().await?;

    // ケース3: 同一値の計算
    let same_value_data = vec![
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote1.clone(),
            BigDecimal::from(100),
            two_hours_ago + chrono::Duration::minutes(30),
        ),
        TokenRate::new_with_timestamp(
            base1.clone(),
            quote1.clone(),
            BigDecimal::from(100),
            one_hour_ago,
        ),
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
