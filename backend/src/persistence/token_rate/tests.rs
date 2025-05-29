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
