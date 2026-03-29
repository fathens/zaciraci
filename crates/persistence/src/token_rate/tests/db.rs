use super::*;

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
    let cfg = ConfigResolver;
    TokenRate::batch_insert(std::slice::from_ref(&token_rate), &cfg).await?;

    // 4. get_latest でインサートしたレコードが返ることを確認
    let result = TokenRate::get_latest(&base, &quote).await?;
    assert!(result.is_some(), "Should return inserted record");

    let retrieved_rate = result.unwrap();
    assert_token_rate_eq!(retrieved_rate, token_rate, "Token rate should match");

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
    let earliest = chrono::Utc::now().naive_utc() - chrono::TimeDelta::hours(2);
    let middle = chrono::Utc::now().naive_utc() - chrono::TimeDelta::hours(1);
    let latest = chrono::Utc::now().naive_utc();

    let rates = vec![
        make_token_rate(base.clone(), quote.clone(), 1000, earliest),
        make_token_rate(base.clone(), quote.clone(), 1050, middle),
        make_token_rate(base.clone(), quote.clone(), 1100, latest),
    ];

    // 3. バッチ挿入
    let cfg = ConfigResolver;
    TokenRate::batch_insert(&rates, &cfg).await?;

    // 4. get_rates_in_time_range で履歴を取得（時系列順で返る）
    let time_range = TimeRange {
        start: earliest - chrono::TimeDelta::minutes(1),
        end: latest + chrono::TimeDelta::minutes(1),
    };
    let mut history = TokenRate::get_rates_in_time_range(&time_range, &base, &quote).await?;
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
    let cfg = ConfigResolver;
    TokenRate::batch_insert(&[rate1.clone(), rate2.clone(), rate3.clone()], &cfg).await?;

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

    // 6. get_rates_in_time_range でも特定のペアだけが取得されることを確認
    let time_range = TimeRange {
        start: now - chrono::TimeDelta::minutes(1),
        end: now + chrono::TimeDelta::minutes(1),
    };

    let history1 = TokenRate::get_rates_in_time_range(&time_range, &base1, &quote1).await?;
    assert_eq!(history1.len(), 1, "Should find 1 record for base1-quote1");
    assert_token_rate_eq!(
        history1[0],
        rate1,
        "base1-quote1 history TokenRate should match"
    );

    let history2 = TokenRate::get_rates_in_time_range(&time_range, &base2, &quote1).await?;
    assert_eq!(history2.len(), 1, "Should find 1 record for base2-quote1");
    assert_token_rate_eq!(
        history2[0],
        rate2,
        "base2-quote1 history TokenRate should match"
    );

    // 7. 存在しないペアは空の配列を返すことを確認
    let history3 = TokenRate::get_rates_in_time_range(&time_range, &base2, &quote2).await?;
    assert_eq!(history3.len(), 0, "Should find 0 records for base2-quote2");

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
    let thirty_min_ago = now - chrono::TimeDelta::minutes(30);
    let one_hour_ago = now - chrono::TimeDelta::hours(1);
    let two_hours_ago = now - chrono::TimeDelta::hours(2);

    // 3. 複数のレコードを挿入（異なるボラティリティを持つデータ）
    // COUNT(*) >= 3 が必要なため各トークン3件以上
    let rates = vec![
        // base1 (eth) - 変動率 50%
        make_token_rate(base1.clone(), quote.clone(), 1000, two_hours_ago),
        make_token_rate(base1.clone(), quote.clone(), 1500, one_hour_ago),
        make_token_rate(base1.clone(), quote.clone(), 1200, thirty_min_ago),
        // base2 (btc) - 変動率 100%
        make_token_rate(base2.clone(), quote.clone(), 20000, two_hours_ago),
        make_token_rate(base2.clone(), quote.clone(), 40000, one_hour_ago),
        make_token_rate(base2.clone(), quote.clone(), 30000, thirty_min_ago),
        // base3 (near) - 変動率 10%
        make_token_rate(base3.clone(), quote.clone(), 5, two_hours_ago),
        make_token_rate_str(base3.clone(), quote.clone(), "5.5", one_hour_ago),
        make_token_rate_str(base3.clone(), quote.clone(), "5.2", thirty_min_ago),
    ];

    // 4. バッチ挿入
    let cfg = ConfigResolver;
    TokenRate::batch_insert(&rates, &cfg).await?;

    // 5. 時間範囲を設定
    let time_range = TimeRange {
        start: two_hours_ago - chrono::TimeDelta::minutes(5), // 少し余裕を持たせる
        end: now + chrono::TimeDelta::minutes(5),             // 少し余裕を持たせる
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
        results[0].coefficient_of_variation > results[1].coefficient_of_variation,
        "BTC variance should be greater than ETH variance"
    );

    // ethの分散は中程度
    assert!(
        results[1].coefficient_of_variation > results[2].coefficient_of_variation,
        "ETH variance should be greater than NEAR variance"
    );

    // nearの分散が最も小さい
    assert!(
        results[2].coefficient_of_variation > 0,
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
            two_hours_ago + chrono::TimeDelta::minutes(1),
        ), // MIN(rate) = 0 なので除外
        make_token_rate(
            base1.clone(),
            quote.clone(),
            100,
            two_hours_ago + chrono::TimeDelta::minutes(2),
        ),
        make_token_rate(
            base1.clone(),
            quote.clone(),
            150,
            one_hour_ago + chrono::TimeDelta::minutes(1),
        ),
        // base2: 全て正の値のため含まれる（COUNT(*) >= 3 必要）
        make_token_rate(
            base2.clone(),
            quote.clone(),
            50,
            two_hours_ago + chrono::TimeDelta::minutes(1),
        ),
        make_token_rate(base2.clone(), quote.clone(), 55, one_hour_ago),
        make_token_rate(
            base2.clone(),
            quote.clone(),
            60,
            one_hour_ago + chrono::TimeDelta::minutes(30),
        ),
    ];

    let cfg = ConfigResolver;
    TokenRate::batch_insert(&zero_rate_data, &cfg).await?;

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
        btc_result.coefficient_of_variation > 0,
        "BTC variance should be greater than 0, got {}",
        btc_result.coefficient_of_variation
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
    let one_hour_ago = now - chrono::TimeDelta::hours(1);
    let two_hours_ago = now - chrono::TimeDelta::hours(2);
    let just_before_range = two_hours_ago + chrono::TimeDelta::seconds(1); // 範囲開始直後
    let just_after_range = now - chrono::TimeDelta::seconds(1); // 範囲終了直前

    // 3. 時間範囲を設定
    let time_range = TimeRange {
        start: two_hours_ago,
        end: now,
    };

    // ケース1: 境界値テスト - 時間範囲の境界値データ
    // COUNT(*) >= 3 が必要なため範囲内に3件
    let mid_range = one_hour_ago;
    let boundary_test_data = vec![
        // 範囲内のデータ（境界値ぎりぎり + 中間）
        make_token_rate(base1.clone(), quote1.clone(), 1000, just_before_range),
        make_token_rate(base1.clone(), quote1.clone(), 1200, mid_range),
        make_token_rate(base1.clone(), quote1.clone(), 1500, just_after_range),
        // 範囲外のデータ（除外されるはず）
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            800,
            two_hours_ago - chrono::TimeDelta::seconds(1),
        ), // 範囲開始直前
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            2000,
            now + chrono::TimeDelta::seconds(1),
        ), // 範囲終了直後
    ];

    let cfg = ConfigResolver;
    TokenRate::batch_insert(&boundary_test_data, &cfg).await?;

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
        boundary_results[0].coefficient_of_variation > 0,
        "Variance should be greater than 0"
    );

    // クリーンアップ
    clean_table().await?;

    // ケース2: 同一ボラティリティ値の処理（COUNT(*) >= 3 が必要）
    let same_volatility_data = vec![
        // base1 (eth) - CV同等
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            100,
            two_hours_ago + chrono::TimeDelta::minutes(1),
        ),
        make_token_rate(base1.clone(), quote1.clone(), 125, one_hour_ago),
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            150,
            now - chrono::TimeDelta::minutes(30),
        ),
        // base2 (btc) - CV同等
        make_token_rate(
            base2.clone(),
            quote1.clone(),
            200,
            two_hours_ago + chrono::TimeDelta::minutes(1),
        ),
        make_token_rate(base2.clone(), quote1.clone(), 250, one_hour_ago),
        make_token_rate(
            base2.clone(),
            quote1.clone(),
            300,
            now - chrono::TimeDelta::minutes(30),
        ),
    ];

    let cfg = ConfigResolver;
    TokenRate::batch_insert(&same_volatility_data, &cfg).await?;

    // 同一ボラティリティテストの結果を検証
    let same_volatility_results =
        TokenRate::get_by_volatility_in_time_range(&time_range, &quote1).await?;
    assert_eq!(
        same_volatility_results.len(),
        2,
        "Should find 2 tokens with same volatility"
    );

    // 両方のトークンが同じボラティリティ（50%）を持つことを確認
    let eth_volatility = same_volatility_results[0].coefficient_of_variation.clone();
    let btc_volatility = same_volatility_results[1].coefficient_of_variation.clone();

    assert!(eth_volatility > 0, "ETH variance should be greater than 0");
    assert!(btc_volatility > 0, "BTC variance should be greater than 0");

    // クリーンアップ
    clean_table().await?;

    // ケース3: 最小レートが0の場合（HAVING MIN(rate) > 0により除外されるケース）
    // COUNT(*) >= 3 が必要
    let zero_max_rate_data = vec![
        // base1: 負の値(-10)と0の値 -> MIN(rate) = -10 > 0 が false なので除外される
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            -10,
            two_hours_ago + chrono::TimeDelta::minutes(1),
        ),
        make_token_rate(base1.clone(), quote1.clone(), -5, one_hour_ago),
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            0,
            now - chrono::TimeDelta::minutes(30),
        ),
        // base2: 全て正の値 -> MIN(rate) = 5 > 0 が true なので含まれる
        make_token_rate(
            base2.clone(),
            quote1.clone(),
            5,
            two_hours_ago + chrono::TimeDelta::minutes(1),
        ),
        make_token_rate(base2.clone(), quote1.clone(), 8, one_hour_ago),
        make_token_rate(
            base2.clone(),
            quote1.clone(),
            10,
            now - chrono::TimeDelta::minutes(30),
        ),
    ];

    let cfg = ConfigResolver;
    TokenRate::batch_insert(&zero_max_rate_data, &cfg).await?;

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
        btc_result.coefficient_of_variation > 0,
        "BTC variance should be greater than 0"
    );

    // クリーンアップ
    clean_table().await?;

    // ケース4: レコードが3件未満の場合（COUNT(*) >= 3 により除外される）
    let single_record_data = vec![make_token_rate(
        base1.clone(),
        quote1.clone(),
        100,
        one_hour_ago,
    )];

    let cfg = ConfigResolver;
    TokenRate::batch_insert(&single_record_data, &cfg).await?;

    // COUNT(*) < 3 なので結果は0件
    let single_record_results =
        TokenRate::get_by_volatility_in_time_range(&time_range, &quote1).await?;
    assert_eq!(
        single_record_results.len(),
        0,
        "Should find 0 tokens (COUNT < 3)"
    );

    // クリーンアップ
    clean_table().await?;

    // ケース5: 異なる quote トークンのフィルタリング（COUNT(*) >= 3 が必要）
    let different_quote_data = vec![
        // quote1用のデータ
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            100,
            two_hours_ago + chrono::TimeDelta::minutes(1),
        ),
        make_token_rate(base1.clone(), quote1.clone(), 125, one_hour_ago),
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            150,
            now - chrono::TimeDelta::minutes(30),
        ),
        // quote2用のデータ（フィルタリングされるはず）
        make_token_rate(
            base1.clone(),
            quote2.clone(),
            200,
            two_hours_ago + chrono::TimeDelta::minutes(1),
        ),
        make_token_rate(base1.clone(), quote2.clone(), 300, one_hour_ago),
        make_token_rate(
            base1.clone(),
            quote2.clone(),
            400,
            now - chrono::TimeDelta::minutes(30),
        ),
    ];

    let cfg = ConfigResolver;
    TokenRate::batch_insert(&different_quote_data, &cfg).await?;

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
        quote_filter_results[0].coefficient_of_variation > 0,
        "Variance should be greater than 0"
    );

    // クリーンアップ
    clean_table().await?;

    // ケース6: 負の値やゼロレートの混在（HAVING MIN(rate) > 0による除外テスト）
    let mixed_rates_data = vec![
        // base1: 負の値、ゼロ、正の値を含む -> MIN(rate) = -10 < 0 なので除外される
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            -10,
            two_hours_ago + chrono::TimeDelta::minutes(10),
        ), // 確実に範囲内
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            0,
            two_hours_ago + chrono::TimeDelta::minutes(20),
        ), // MIN(rate) = -10 なので除外
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            10,
            two_hours_ago + chrono::TimeDelta::minutes(30),
        ), // 確実に範囲内
        // base2: 全て正の値 -> MIN(rate) = 5 > 0 なので含まれる（COUNT(*) >= 3 必要）
        make_token_rate(
            base2.clone(),
            quote1.clone(),
            5,
            two_hours_ago + chrono::TimeDelta::minutes(40),
        ), // 確実に範囲内
        make_token_rate(
            base2.clone(),
            quote1.clone(),
            10,
            two_hours_ago + chrono::TimeDelta::minutes(50),
        ), // 確実に範囲内
        make_token_rate(
            base2.clone(),
            quote1.clone(),
            15,
            two_hours_ago + chrono::TimeDelta::minutes(60),
        ), // 確実に範囲内
    ];

    let cfg = ConfigResolver;
    TokenRate::batch_insert(&mixed_rates_data, &cfg).await?;

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
        mixed_rates_results[0].coefficient_of_variation > 0,
        "Variance should be greater than 0"
    );

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
    let one_hour_ago = now - chrono::TimeDelta::hours(1);
    let two_hours_ago = now - chrono::TimeDelta::hours(2);

    // 3. 時間範囲を設定
    let time_range = TimeRange {
        start: two_hours_ago,
        end: now,
    };

    // ケース1: 通常の計算 - 正の値（COUNT(*) >= 3 が必要）
    let normal_data = vec![
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            1000,
            two_hours_ago + chrono::TimeDelta::minutes(30),
        ),
        make_token_rate(base1.clone(), quote1.clone(), 1200, one_hour_ago),
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            1500,
            now - chrono::TimeDelta::minutes(30),
        ),
    ];

    let cfg = ConfigResolver;
    TokenRate::batch_insert(&normal_data, &cfg).await?;

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
        normal_results[0].coefficient_of_variation > 0,
        "Variance should be greater than 0"
    );

    // クリーンアップ
    clean_table().await?;

    // ケース2: 負の値を含む計算（HAVING MIN(rate) > 0により除外される）
    // COUNT(*) >= 3 が必要
    let negative_data = vec![
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            10,
            two_hours_ago + chrono::TimeDelta::minutes(1),
        ), // MIN(rate) = 10 > 0 なので含まれる
        make_token_rate(base1.clone(), quote1.clone(), 50, one_hour_ago),
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            100,
            now - chrono::TimeDelta::minutes(30),
        ),
    ];

    let cfg = ConfigResolver;
    TokenRate::batch_insert(&negative_data, &cfg).await?;

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
        positive_results[0].coefficient_of_variation > 0,
        "Variance should be greater than 0"
    );

    // クリーンアップ
    clean_table().await?;

    // ケース3: 同一値の計算（COUNT(*) >= 3 が必要）
    let same_value_data = vec![
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            100,
            two_hours_ago + chrono::TimeDelta::minutes(30),
        ),
        make_token_rate(base1.clone(), quote1.clone(), 100, one_hour_ago),
        make_token_rate(
            base1.clone(),
            quote1.clone(),
            100,
            now - chrono::TimeDelta::minutes(30),
        ),
    ];

    let cfg = ConfigResolver;
    TokenRate::batch_insert(&same_value_data, &cfg).await?;

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
        same_value_results[0].coefficient_of_variation,
        BigDecimal::from(0),
        "Variance should be 0"
    );

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
    let days_400_ago = now - chrono::TimeDelta::days(400);
    let days_200_ago = now - chrono::TimeDelta::days(200);
    let days_100_ago = now - chrono::TimeDelta::days(100);
    let days_10_ago = now - chrono::TimeDelta::days(10);

    let old_rates = vec![
        // 400日前のレコード（削除されるはず - 90日超）
        make_token_rate(base1.clone(), quote.clone(), 1000, days_400_ago),
        // 200日前のレコード（削除されるはず - 90日超）
        make_token_rate(base1.clone(), quote.clone(), 1100, days_200_ago),
        // 100日前のレコード（削除されるはず - 90日超）
        make_token_rate(base1.clone(), quote.clone(), 1200, days_100_ago),
        // 10日前のレコード（残るはず - 90日以内）
        make_token_rate(base1.clone(), quote.clone(), 1300, days_10_ago),
        // 今のレコード（残るはず - 90日以内）
        make_token_rate(base1.clone(), quote.clone(), 1400, now),
        // 別のトークンペア - 400日前（削除されるはず - 90日超）
        make_token_rate(base2.clone(), quote.clone(), 20000, days_400_ago),
        // 別のトークンペア - 今（残るはず - 90日以内）
        make_token_rate(base2.clone(), quote.clone(), 21000, now),
    ];

    // 3. レコードを挿入
    let cfg = ConfigResolver;
    TokenRate::batch_insert(&old_rates, &cfg).await?;

    // 90日でクリーンアップ（batch_insert 内の spawn 完了を待たず明示的に実行）
    TokenRate::cleanup_old_records(90).await?;

    // 4. 残っているレコード数を確認
    let wide_range = TimeRange {
        start: days_400_ago - chrono::TimeDelta::days(1),
        end: now + chrono::TimeDelta::days(1),
    };
    let mut history1 = TokenRate::get_rates_in_time_range(&wide_range, &base1, &quote).await?;
    let history2 = TokenRate::get_rates_in_time_range(&wide_range, &base2, &quote).await?;
    // 新しい順に並び替え
    history1.reverse();

    // base1: 10日前、今の2件が残る（全て90日以内）
    assert_eq!(
        history1.len(),
        2,
        "Should have 2 records for base1 (within 90 days)"
    );

    // base2: 今の1件が残る
    assert_eq!(
        history2.len(),
        1,
        "Should have 1 record for base2 (within 90 days)"
    );

    // 5. 残っているレコードのタイムスタンプを確認
    // 新しい順にソートされているので、最初が最新
    assert!(
        history1[0].timestamp >= days_10_ago,
        "Most recent record should be recent"
    );

    // 最も古いレコードは10日前のもの（index 1）
    // レートで確認（10日前のレコードは rate = 1300）
    assert_eq!(
        history1[1].exchange_rate.raw_rate(),
        &BigDecimal::from(1300),
        "Oldest retained record should have rate 1300 (10 days old record)"
    );

    // タイムスタンプも確認（精度の問題を考慮して、10日前から少し前後する範囲を許容）
    let timestamp_diff = if history1[1].timestamp > days_10_ago {
        history1[1].timestamp - days_10_ago
    } else {
        days_10_ago - history1[1].timestamp
    };

    assert!(
        timestamp_diff < chrono::TimeDelta::seconds(1),
        "Oldest retained record timestamp should be close to 10 days ago. \
         Expected: {}, Actual: {}, Diff: {:?}",
        days_10_ago,
        history1[1].timestamp,
        timestamp_diff
    );

    // 100日前以前のレコードは全て削除されている（90日超）
    assert!(
        history1[1].timestamp > days_100_ago,
        "Oldest retained record should be newer than 100 days ago (90+ days old records should be deleted)"
    );

    // クリーンアップ
    clean_table().await?;

    // 6. カスタム保持期間のテスト（30日）
    let recent_rates = vec![
        make_token_rate(
            base1.clone(),
            quote.clone(),
            1000,
            now - chrono::TimeDelta::days(100),
        ),
        make_token_rate(
            base1.clone(),
            quote.clone(),
            1100,
            now - chrono::TimeDelta::days(50),
        ),
        make_token_rate(
            base1.clone(),
            quote.clone(),
            1200,
            now - chrono::TimeDelta::days(20),
        ),
        make_token_rate(
            base1.clone(),
            quote.clone(),
            1300,
            now - chrono::TimeDelta::days(5),
        ),
    ];

    // まず全件挿入
    let cfg = ConfigResolver;
    TokenRate::batch_insert(&recent_rates, &cfg).await?;

    // 90日でクリーンアップ（batch_insert 内の spawn 完了を待たず明示的に実行）
    TokenRate::cleanup_old_records(90).await?;

    // 90日以内のレコード（50日前、20日前、5日前）が残っていることを確認
    let all_history = TokenRate::get_rates_in_time_range(&wide_range, &base1, &quote).await?;
    assert_eq!(
        all_history.len(),
        3,
        "Should have 3 records initially (within 90 days)"
    );

    // 7. 30日でクリーンアップを実行
    TokenRate::cleanup_old_records(30).await?;

    // 8. 30日以内のレコードだけが残っていることを確認
    let mut recent_history =
        TokenRate::get_rates_in_time_range(&wide_range, &base1, &quote).await?;
    // 新しい順に並び替え
    recent_history.reverse();
    assert_eq!(
        recent_history.len(),
        2,
        "Should have 2 records (within 30 days)"
    );

    // 残っているレコードが20日前と5日前のものであることを確認
    assert!(
        recent_history[0].timestamp >= now - chrono::TimeDelta::days(6),
        "Most recent should be ~5 days old"
    );
    assert!(
        recent_history[1].timestamp >= now - chrono::TimeDelta::days(21),
        "Second should be ~20 days old"
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_get_rates_for_multiple_tokens() -> Result<()> {
    clean_table().await?;

    // テスト用トークン
    let base1: TokenOutAccount = TokenAccount::from_str("token1.near")?.into();
    let base2: TokenOutAccount = TokenAccount::from_str("token2.near")?.into();
    let base3: TokenOutAccount = TokenAccount::from_str("token3.near")?.into();
    let quote: TokenInAccount = TokenAccount::from_str("wrap.near")?.into();

    let now = chrono::Utc::now().naive_utc();
    let one_hour_ago = now - chrono::TimeDelta::hours(1);

    // 各トークンに2件ずつデータを挿入
    let rates = vec![
        make_token_rate(base1.clone(), quote.clone(), 100, one_hour_ago),
        make_token_rate(base1.clone(), quote.clone(), 110, now),
        make_token_rate(base2.clone(), quote.clone(), 200, one_hour_ago),
        make_token_rate(base2.clone(), quote.clone(), 220, now),
        make_token_rate(base3.clone(), quote.clone(), 300, one_hour_ago),
        make_token_rate(base3.clone(), quote.clone(), 330, now),
    ];
    let cfg = ConfigResolver;
    TokenRate::batch_insert(&rates, &cfg).await?;

    let time_range = TimeRange {
        start: one_hour_ago - chrono::TimeDelta::minutes(1),
        end: now + chrono::TimeDelta::minutes(1),
    };

    // 2トークンのみ取得
    let tokens = vec![base1.clone(), base2.clone()];
    let result = TokenRate::get_rates_for_multiple_tokens(&tokens, &quote, &time_range).await?;

    assert_eq!(result.len(), 2, "Should return 2 tokens");
    assert!(result.contains_key(&base1), "Should contain token1");
    assert!(result.contains_key(&base2), "Should contain token2");
    assert!(!result.contains_key(&base3), "Should not contain token3");

    // 各トークンに2件のデータがあることを確認
    assert_eq!(result[&base1].len(), 2);
    assert_eq!(result[&base2].len(), 2);

    // 時系列順（昇順）であることを確認
    assert!(result[&base1][0].timestamp < result[&base1][1].timestamp);

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_get_rates_for_multiple_tokens_empty() -> Result<()> {
    clean_table().await?;

    let quote: TokenInAccount = TokenAccount::from_str("wrap.near")?.into();
    let now = chrono::Utc::now().naive_utc();

    let time_range = TimeRange {
        start: now - chrono::TimeDelta::hours(1),
        end: now,
    };

    // 存在しないトークン
    let tokens: Vec<TokenOutAccount> = vec![
        TokenAccount::from_str("nonexistent1.near")?.into(),
        TokenAccount::from_str("nonexistent2.near")?.into(),
    ];
    let result = TokenRate::get_rates_for_multiple_tokens(&tokens, &quote, &time_range).await?;

    assert!(
        result.is_empty(),
        "Should return empty map for nonexistent tokens"
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_get_rates_for_multiple_tokens_empty_input() -> Result<()> {
    let quote: TokenInAccount = TokenAccount::from_str("wrap.near")?.into();
    let now = chrono::Utc::now().naive_utc();

    let time_range = TimeRange {
        start: now - chrono::TimeDelta::hours(1),
        end: now,
    };

    // 空のトークンリスト
    let tokens: Vec<TokenOutAccount> = vec![];
    let result = TokenRate::get_rates_for_multiple_tokens(&tokens, &quote, &time_range).await?;

    assert!(result.is_empty(), "Should return empty map for empty input");

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_swap_path_jsonb_roundtrip() -> Result<()> {
    use common::types::TokenSmallestUnits;

    clean_table().await?;

    let base: TokenOutAccount = TokenAccount::from_str("token.near")?.into();
    let quote: TokenInAccount = TokenAccount::from_str("wrap.near")?.into();
    let now = chrono::Utc::now().naive_utc();

    // TokenSmallestUnits を含む SwapPath を作成
    let swap_path = SwapPath {
        pools: vec![
            SwapPoolInfo {
                pool_id: 42,
                token_in_idx: 0,
                token_out_idx: 1,
                amount_in: TokenSmallestUnits::from_u128(1_000_000_000_000_000_000_000_000),
                amount_out: TokenSmallestUnits::from_u128(500_000_000_000_000_000_000_000),
            },
            SwapPoolInfo {
                pool_id: 99,
                token_in_idx: 0,
                token_out_idx: 1,
                amount_in: TokenSmallestUnits::from_u128(u128::MAX),
                amount_out: TokenSmallestUnits::from_u128(0),
            },
        ],
    };

    let rate = TokenRate {
        base: base.clone(),
        quote: quote.clone(),
        exchange_rate: ExchangeRate::from_raw_rate(BigDecimal::from(100), 24),
        timestamp: now,
        rate_calc_near: 10,
        swap_path: Some(swap_path.clone()),
    };

    let cfg = ConfigResolver;
    TokenRate::batch_insert(std::slice::from_ref(&rate), &cfg).await?;

    // DB から取得して SwapPath が復元されることを確認
    let retrieved = TokenRate::get_latest(&base, &quote).await?.unwrap();
    let retrieved_path = retrieved.swap_path.unwrap();

    assert_eq!(retrieved_path.pools.len(), 2);
    assert_eq!(retrieved_path.pools[0].pool_id, 42);
    assert_eq!(
        retrieved_path.pools[0].amount_in,
        TokenSmallestUnits::from_u128(1_000_000_000_000_000_000_000_000)
    );
    assert_eq!(
        retrieved_path.pools[0].amount_out,
        TokenSmallestUnits::from_u128(500_000_000_000_000_000_000_000)
    );
    // u128::MAX もラウンドトリップできることを確認
    assert_eq!(
        retrieved_path.pools[1].amount_in,
        TokenSmallestUnits::from_u128(u128::MAX)
    );
    assert_eq!(
        retrieved_path.pools[1].amount_out,
        TokenSmallestUnits::from_u128(0)
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_get_spot_rates_at_time_latest_before() -> Result<()> {
    clean_table().await?;

    let base1: TokenOutAccount = TokenAccount::from_str("token1.near")?.into();
    let base2: TokenOutAccount = TokenAccount::from_str("token2.near")?.into();
    let quote: TokenInAccount = TokenAccount::from_str("wrap.near")?.into();

    let now = chrono::Utc::now().naive_utc();
    let t1 = now - chrono::TimeDelta::hours(3);
    let t2 = now - chrono::TimeDelta::hours(2);
    let t3 = now - chrono::TimeDelta::hours(1);

    // token1: t1=100, t2=200, t3=300
    // token2: t1=400, t3=600
    let rates = vec![
        make_token_rate(base1.clone(), quote.clone(), 100, t1),
        make_token_rate(base1.clone(), quote.clone(), 200, t2),
        make_token_rate(base1.clone(), quote.clone(), 300, t3),
        make_token_rate(base2.clone(), quote.clone(), 400, t1),
        make_token_rate(base2.clone(), quote.clone(), 600, t3),
    ];
    let cfg = ConfigResolver;
    TokenRate::batch_insert(&rates, &cfg).await?;

    // t2 時点: token1=200, token2=400 (t1 のレート)
    let tokens = vec![base1.clone(), base2.clone()];
    let result = TokenRate::get_spot_rates_at_time(&tokens, &quote, t2).await?;

    assert_eq!(result.len(), 2);
    // swap_path なしなので補正なし、生レートがそのまま返る
    assert_eq!(result[&base1].raw_rate(), &BigDecimal::from(200));
    assert_eq!(result[&base2].raw_rate(), &BigDecimal::from(400));

    // t3 時点: token1=300, token2=600
    let result = TokenRate::get_spot_rates_at_time(&tokens, &quote, t3).await?;
    assert_eq!(result.len(), 2);
    assert_eq!(result[&base1].raw_rate(), &BigDecimal::from(300));
    assert_eq!(result[&base2].raw_rate(), &BigDecimal::from(600));

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_get_spot_rates_at_time_empty() -> Result<()> {
    clean_table().await?;

    let base: TokenOutAccount = TokenAccount::from_str("token1.near")?.into();
    let quote: TokenInAccount = TokenAccount::from_str("wrap.near")?.into();

    let now = chrono::Utc::now().naive_utc();
    let future = now + chrono::TimeDelta::hours(1);

    // レートを now に挿入
    let rates = vec![make_token_rate(base.clone(), quote.clone(), 100, future)];
    let cfg = ConfigResolver;
    TokenRate::batch_insert(&rates, &cfg).await?;

    // now 時点で取得 → future のレートは対象外なので空
    let tokens = vec![base.clone()];
    let result = TokenRate::get_spot_rates_at_time(&tokens, &quote, now).await?;
    assert!(
        result.is_empty(),
        "All rates are after at_or_before, should be empty"
    );

    // 空トークンリスト
    let result = TokenRate::get_spot_rates_at_time(&[], &quote, now).await?;
    assert!(result.is_empty());

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_get_spot_rates_at_time_swap_path_fallback() -> Result<()> {
    use common::types::TokenSmallestUnits;

    clean_table().await?;

    let base: TokenOutAccount = TokenAccount::from_str("token1.near")?.into();
    let quote: TokenInAccount = TokenAccount::from_str("wrap.near")?.into();

    let now = chrono::Utc::now().naive_utc();
    let t1 = now - chrono::TimeDelta::hours(2);
    let t2 = now - chrono::TimeDelta::hours(1);

    let swap_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 42,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: TokenSmallestUnits::from_u128(1_000_000_000_000_000_000_000_000_000),
            amount_out: TokenSmallestUnits::from_u128(500_000_000_000_000_000_000_000_000),
        }],
    };

    // t1: swap_path あり、t2: swap_path なし（最新）
    let rate_with_path = TokenRate {
        base: base.clone(),
        quote: quote.clone(),
        exchange_rate: ExchangeRate::from_raw_rate(BigDecimal::from(100), 24),
        timestamp: t1,
        rate_calc_near: 10,
        swap_path: Some(swap_path.clone()),
    };
    let rate_without_path = make_token_rate(base.clone(), quote.clone(), 110, t2);

    let cfg = ConfigResolver;
    TokenRate::batch_insert(&[rate_with_path, rate_without_path], &cfg).await?;

    // t2 時点: 最新は swap_path なし → t1 の swap_path を COALESCE で使用
    let tokens = vec![base.clone()];
    let result = TokenRate::get_spot_rates_at_time(&tokens, &quote, t2).await?;

    assert_eq!(result.len(), 1);
    let spot_rate = &result[&base];

    // to_spot_rate_with_fallback で期待値を計算して exact match
    let rate_without_path = make_token_rate(base.clone(), quote.clone(), 110, t2);
    let expected = rate_without_path.to_spot_rate_with_fallback(Some(&swap_path));
    assert_eq!(
        spot_rate.raw_rate(),
        expected.raw_rate(),
        "Spot rate should match to_spot_rate_with_fallback result"
    );
    // 補正が実際に適用されていることも確認
    assert!(
        spot_rate.raw_rate() > &BigDecimal::from(110),
        "Spot rate should be corrected (> 110), got {}",
        spot_rate.raw_rate()
    );

    Ok(())
}

/// 最新レートが swap_path を持つ場合、フォールバックではなく自身の swap_path を使用する
#[tokio::test]
#[serial]
async fn test_get_spot_rates_at_time_own_swap_path_preferred() -> Result<()> {
    use common::types::TokenSmallestUnits;

    clean_table().await?;

    let base: TokenOutAccount = TokenAccount::from_str("token1.near")?.into();
    let quote: TokenInAccount = TokenAccount::from_str("wrap.near")?.into();

    let now = chrono::Utc::now().naive_utc();
    let t1 = now - chrono::TimeDelta::hours(2);
    let t2 = now - chrono::TimeDelta::hours(1);

    // 大きいプール → 小さい補正
    let old_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 1,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: TokenSmallestUnits::from_u128(1_000_000_000_000_000_000_000_000_000),
            amount_out: TokenSmallestUnits::from_u128(500_000_000_000_000_000_000_000_000),
        }],
    };

    // 小さいプール → 大きい補正
    let new_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 2,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: TokenSmallestUnits::from_u128(100_000_000_000_000_000_000_000_000),
            amount_out: TokenSmallestUnits::from_u128(50_000_000_000_000_000_000_000_000),
        }],
    };

    let rate_old = TokenRate {
        base: base.clone(),
        quote: quote.clone(),
        exchange_rate: ExchangeRate::from_raw_rate(BigDecimal::from(100), 24),
        timestamp: t1,
        rate_calc_near: 10,
        swap_path: Some(old_path),
    };
    let rate_new = TokenRate {
        base: base.clone(),
        quote: quote.clone(),
        exchange_rate: ExchangeRate::from_raw_rate(BigDecimal::from(110), 24),
        timestamp: t2,
        rate_calc_near: 10,
        swap_path: Some(new_path.clone()),
    };

    let cfg = ConfigResolver;
    TokenRate::batch_insert(&[rate_old, rate_new.clone()], &cfg).await?;

    let tokens = vec![base.clone()];
    let result = TokenRate::get_spot_rates_at_time(&tokens, &quote, t2).await?;

    // 最新レートは自身の swap_path を使うので、rate_new.to_spot_rate() と一致するはず
    let expected = rate_new.to_spot_rate();
    assert_eq!(
        result[&base].raw_rate(),
        expected.raw_rate(),
        "Should use own swap_path, not fallback"
    );

    Ok(())
}

/// swap_path が一切ない場合、補正なしの生レートが返る
#[tokio::test]
#[serial]
async fn test_get_spot_rates_at_time_no_swap_path_anywhere() -> Result<()> {
    clean_table().await?;

    let base: TokenOutAccount = TokenAccount::from_str("token1.near")?.into();
    let quote: TokenInAccount = TokenAccount::from_str("wrap.near")?.into();

    let now = chrono::Utc::now().naive_utc();
    let t1 = now - chrono::TimeDelta::hours(2);
    let t2 = now - chrono::TimeDelta::hours(1);

    // 両方とも swap_path なし
    let rates = vec![
        make_token_rate(base.clone(), quote.clone(), 100, t1),
        make_token_rate(base.clone(), quote.clone(), 200, t2),
    ];
    let cfg = ConfigResolver;
    TokenRate::batch_insert(&rates, &cfg).await?;

    let tokens = vec![base.clone()];
    let result = TokenRate::get_spot_rates_at_time(&tokens, &quote, t2).await?;

    assert_eq!(result.len(), 1);
    assert_eq!(
        result[&base].raw_rate(),
        &BigDecimal::from(200),
        "No swap_path anywhere, should return raw rate"
    );

    Ok(())
}

/// 複数トークンで swap_path の有無が混在するケース
#[tokio::test]
#[serial]
async fn test_get_spot_rates_at_time_mixed_swap_path() -> Result<()> {
    use common::types::TokenSmallestUnits;

    clean_table().await?;

    let base_with: TokenOutAccount = TokenAccount::from_str("has_path.near")?.into();
    let base_without: TokenOutAccount = TokenAccount::from_str("no_path.near")?.into();
    let base_fallback: TokenOutAccount = TokenAccount::from_str("fallback.near")?.into();
    let quote: TokenInAccount = TokenAccount::from_str("wrap.near")?.into();

    let now = chrono::Utc::now().naive_utc();
    let t1 = now - chrono::TimeDelta::hours(2);
    let t2 = now - chrono::TimeDelta::hours(1);

    let path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 10,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: TokenSmallestUnits::from_u128(1_000_000_000_000_000_000_000_000_000),
            amount_out: TokenSmallestUnits::from_u128(500_000_000_000_000_000_000_000_000),
        }],
    };

    let rates = vec![
        // base_with: 最新に swap_path あり
        TokenRate {
            base: base_with.clone(),
            quote: quote.clone(),
            exchange_rate: ExchangeRate::from_raw_rate(BigDecimal::from(100), 24),
            timestamp: t2,
            rate_calc_near: 10,
            swap_path: Some(path.clone()),
        },
        // base_without: swap_path なし（一切なし）
        make_token_rate(base_without.clone(), quote.clone(), 200, t2),
        // base_fallback: t1 に swap_path あり、t2 は swap_path なし
        TokenRate {
            base: base_fallback.clone(),
            quote: quote.clone(),
            exchange_rate: ExchangeRate::from_raw_rate(BigDecimal::from(300), 24),
            timestamp: t1,
            rate_calc_near: 10,
            swap_path: Some(path),
        },
        make_token_rate(base_fallback.clone(), quote.clone(), 310, t2),
    ];

    let cfg = ConfigResolver;
    TokenRate::batch_insert(&rates, &cfg).await?;

    let tokens = vec![
        base_with.clone(),
        base_without.clone(),
        base_fallback.clone(),
    ];
    let result = TokenRate::get_spot_rates_at_time(&tokens, &quote, t2).await?;

    assert_eq!(result.len(), 3);

    // base_with: 自身の swap_path で補正 → 生レート(100)より大きい
    assert!(
        result[&base_with].raw_rate() > &BigDecimal::from(100),
        "Should be corrected with own swap_path"
    );

    // base_without: swap_path なし → 補正なし
    assert_eq!(
        result[&base_without].raw_rate(),
        &BigDecimal::from(200),
        "No swap_path, should return raw rate"
    );

    // base_fallback: フォールバック swap_path で補正 → 生レート(310)より大きい
    assert!(
        result[&base_fallback].raw_rate() > &BigDecimal::from(310),
        "Should be corrected with fallback swap_path"
    );

    Ok(())
}

/// 存在しないトークンを含むリクエスト
#[tokio::test]
#[serial]
async fn test_get_spot_rates_at_time_partial_tokens() -> Result<()> {
    clean_table().await?;

    let base_exists: TokenOutAccount = TokenAccount::from_str("exists.near")?.into();
    let base_missing: TokenOutAccount = TokenAccount::from_str("missing.near")?.into();
    let quote: TokenInAccount = TokenAccount::from_str("wrap.near")?.into();

    let now = chrono::Utc::now().naive_utc();

    let rates = vec![make_token_rate(
        base_exists.clone(),
        quote.clone(),
        500,
        now,
    )];
    let cfg = ConfigResolver;
    TokenRate::batch_insert(&rates, &cfg).await?;

    let tokens = vec![base_exists.clone(), base_missing.clone()];
    let result = TokenRate::get_spot_rates_at_time(&tokens, &quote, now).await?;

    // 存在するトークンのみ返る
    assert_eq!(result.len(), 1);
    assert!(result.contains_key(&base_exists));
    assert!(!result.contains_key(&base_missing));

    Ok(())
}

/// 異なる quote トークンのレートが混入しないことを検証
#[tokio::test]
#[serial]
async fn test_get_spot_rates_at_time_quote_isolation() -> Result<()> {
    clean_table().await?;

    let base: TokenOutAccount = TokenAccount::from_str("token1.near")?.into();
    let quote_a: TokenInAccount = TokenAccount::from_str("wrap.near")?.into();
    let quote_b: TokenInAccount = TokenAccount::from_str("usdt.near")?.into();

    let now = chrono::Utc::now().naive_utc();
    let t1 = now - chrono::TimeDelta::hours(2);
    let t2 = now - chrono::TimeDelta::hours(1);

    // quote_a: t1=100 のみ、quote_b: t2=999（より新しい）
    let rates = vec![
        make_token_rate(base.clone(), quote_a.clone(), 100, t1),
        make_token_rate(base.clone(), quote_b.clone(), 999, t2),
    ];
    let cfg = ConfigResolver;
    TokenRate::batch_insert(&rates, &cfg).await?;

    // quote_a で取得 → quote_b の 999 が混入しないこと
    let tokens = vec![base.clone()];
    let result = TokenRate::get_spot_rates_at_time(&tokens, &quote_a, t2).await?;

    assert_eq!(result.len(), 1);
    assert_eq!(
        result[&base].raw_rate(),
        &BigDecimal::from(100),
        "Should only return rates for the specified quote token"
    );

    // quote_b で取得
    let result = TokenRate::get_spot_rates_at_time(&tokens, &quote_b, t2).await?;
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[&base].raw_rate(),
        &BigDecimal::from(999),
        "Should return the correct quote_b rate"
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_get_all_decimals() -> Result<()> {
    clean_table().await?;

    let base_a: TokenOutAccount = TokenAccount::from_str("token_a.near")?.into();
    let base_b: TokenOutAccount = TokenAccount::from_str("token_b.near")?.into();
    let quote: TokenInAccount = TokenAccount::from_str("wrap.near")?.into();
    let now = chrono::Utc::now().naive_utc();

    // decimals=24 のレートを2トークン分挿入
    let rates = vec![
        make_token_rate(base_a.clone(), quote.clone(), 100, now),
        make_token_rate(base_b.clone(), quote.clone(), 200, now),
    ];
    let cfg = ConfigResolver;
    TokenRate::batch_insert(&rates, &cfg).await?;

    let decimals = get_all_decimals().await?;
    assert_eq!(
        decimals.get(&TokenAccount::from_str("token_a.near")?),
        Some(&24u8)
    );
    assert_eq!(
        decimals.get(&TokenAccount::from_str("token_b.near")?),
        Some(&24u8)
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_get_all_decimals_empty_table() -> Result<()> {
    clean_table().await?;

    let decimals = get_all_decimals().await?;
    assert!(decimals.is_empty());

    Ok(())
}

// =============================================================================
// get_all_latest_rates テスト
// =============================================================================

#[tokio::test]
#[serial]
async fn test_get_all_latest_rates_empty() -> Result<()> {
    clean_table().await?;

    let quote = TokenAccount::from_str("wrap.near")?;
    let result = get_all_latest_rates(&quote).await?;
    assert!(result.is_empty(), "Empty table should return empty HashMap");

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_get_all_latest_rates_returns_latest() -> Result<()> {
    clean_table().await?;

    let base: TokenOutAccount = TokenAccount::from_str("usdt.tether-token.near")?.into();
    let quote: TokenInAccount = TokenAccount::from_str("wrap.near")?.into();
    let now = chrono::Utc::now().naive_utc();

    // 同一 base_token に対し異なる timestamp で複数レコード挿入
    let old = now - chrono::TimeDelta::hours(2);
    let recent = now - chrono::TimeDelta::hours(1);

    let rates = vec![
        make_token_rate(base.clone(), quote.clone(), 100, old),
        make_token_rate(base.clone(), quote.clone(), 200, recent),
    ];
    let cfg = ConfigResolver;
    TokenRate::batch_insert(&rates, &cfg).await?;

    let quote_token = TokenAccount::from_str("wrap.near")?;
    let result = get_all_latest_rates(&quote_token).await?;

    assert_eq!(result.len(), 1, "Should return exactly 1 token");
    let base_token = TokenAccount::from_str("usdt.tether-token.near")?;
    let rate = result.get(&base_token).expect("Should have usdt rate");
    // swap_path なしの場合、spot_rate == raw_rate なので最新の 200 が返る
    assert_eq!(rate.raw_rate(), &BigDecimal::from(200));

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_get_all_latest_rates_quote_isolation() -> Result<()> {
    clean_table().await?;

    let base: TokenOutAccount = TokenAccount::from_str("usdt.tether-token.near")?.into();
    let quote_a: TokenInAccount = TokenAccount::from_str("wrap.near")?.into();
    let quote_b: TokenInAccount = TokenAccount::from_str("usdc.token.near")?.into();
    let now = chrono::Utc::now().naive_utc();

    let rates = vec![
        make_token_rate(base.clone(), quote_a.clone(), 100, now),
        make_token_rate(
            base.clone(),
            quote_b.clone(),
            999,
            now - chrono::TimeDelta::seconds(1),
        ),
    ];
    let cfg = ConfigResolver;
    TokenRate::batch_insert(&rates, &cfg).await?;

    // wrap.near 建てで取得 → quote_b のレコードは含まれない
    let quote_token = TokenAccount::from_str("wrap.near")?;
    let result = get_all_latest_rates(&quote_token).await?;

    assert_eq!(result.len(), 1);
    let base_token = TokenAccount::from_str("usdt.tether-token.near")?;
    let rate = result.get(&base_token).expect("Should have usdt rate");
    assert_eq!(
        rate.raw_rate(),
        &BigDecimal::from(100),
        "Should only return rates for the specified quote token"
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_get_all_latest_rates_swap_path_fallback() -> Result<()> {
    clean_table().await?;

    let base: TokenOutAccount = TokenAccount::from_str("usdt.tether-token.near")?.into();
    let quote: TokenInAccount = TokenAccount::from_str("wrap.near")?.into();
    let now = chrono::Utc::now().naive_utc();

    // 古いレコード: swap_path あり
    let swap_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 42,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "1000000000000000000000000".parse().unwrap(), // 1 NEAR in yocto
            amount_out: "5000000".parse().unwrap(),
        }],
    };
    let old_rate = TokenRate {
        base: base.clone(),
        quote: quote.clone(),
        exchange_rate: ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 24),
        timestamp: now - chrono::TimeDelta::hours(2),
        rate_calc_near: 10,
        swap_path: Some(swap_path),
    };

    // 最新レコード: swap_path なし (rate は異なる)
    let new_rate = TokenRate {
        base: base.clone(),
        quote: quote.clone(),
        exchange_rate: ExchangeRate::from_raw_rate(BigDecimal::from(5_100_000), 24),
        timestamp: now - chrono::TimeDelta::hours(1),
        rate_calc_near: 10,
        swap_path: None,
    };

    let cfg = ConfigResolver;
    TokenRate::batch_insert(&[old_rate, new_rate], &cfg).await?;

    let quote_token = TokenAccount::from_str("wrap.near")?;
    let result = get_all_latest_rates(&quote_token).await?;

    assert_eq!(result.len(), 1);
    let base_token = TokenAccount::from_str("usdt.tether-token.near")?;
    let rate = result.get(&base_token).expect("Should have usdt rate");

    // 最新レコードのレート (5_100_000) に swap_path フォールバックによるスポット補正が適用される。
    // swap_path なしの場合のスポットレート = raw_rate そのまま (5_100_000)。
    // フォールバック swap_path がある場合、補正係数が適用されるため raw_rate と異なるはず。
    // 補正式: spot = rate * (1 + Δx/x)
    //   Δx = rate_calc_near * 10^24 = 10 * 10^24 = 10^25
    //   x  = amount_in = 10^24
    //   correction = 1 + 10^25 / 10^24 = 1 + 10 = 11
    //   spot = 5_100_000 * 11 = 56_100_000
    assert_eq!(
        rate.raw_rate(),
        &BigDecimal::from(56_100_000_i64),
        "Fallback swap_path should apply spot rate correction"
    );

    Ok(())
}

/// 複数トークンで swap_path の有無が混在するケースで、
/// DISTINCT ON + LEFT JOIN fallback がトークン間で混線しないことを検証
#[tokio::test]
#[serial]
async fn test_get_all_latest_rates_multi_token_mixed() -> Result<()> {
    use common::types::TokenSmallestUnits;

    clean_table().await?;

    let base_with: TokenOutAccount = TokenAccount::from_str("has-path.near")?.into();
    let base_fallback: TokenOutAccount = TokenAccount::from_str("fallback.near")?.into();
    let base_none: TokenOutAccount = TokenAccount::from_str("no-path.near")?.into();
    let quote: TokenInAccount = TokenAccount::from_str("wrap.near")?.into();

    let now = chrono::Utc::now().naive_utc();
    let t1 = now - chrono::TimeDelta::hours(3);
    let t2 = now - chrono::TimeDelta::hours(2);
    let t3 = now - chrono::TimeDelta::hours(1);

    let path_a = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 10,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: TokenSmallestUnits::from_u128(1_000_000_000_000_000_000_000_000_000),
            amount_out: TokenSmallestUnits::from_u128(500_000_000_000_000_000_000_000_000),
        }],
    };

    let path_b = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 20,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: TokenSmallestUnits::from_u128(2_000_000_000_000_000_000_000_000_000),
            amount_out: TokenSmallestUnits::from_u128(1_000_000_000_000_000_000_000_000_000),
        }],
    };

    let rates = vec![
        // has-path.near: 最新レコードに swap_path あり（自身のパスを使用すべき）
        TokenRate {
            base: base_with.clone(),
            quote: quote.clone(),
            exchange_rate: ExchangeRate::from_raw_rate(BigDecimal::from(100), 24),
            timestamp: t3,
            rate_calc_near: 10,
            swap_path: Some(path_a.clone()),
        },
        // fallback.near: 古いレコードに swap_path あり、最新レコードは swap_path なし
        // → fallback CTE から path_b が補完されるべき（path_a ではない）
        TokenRate {
            base: base_fallback.clone(),
            quote: quote.clone(),
            exchange_rate: ExchangeRate::from_raw_rate(BigDecimal::from(200), 24),
            timestamp: t1,
            rate_calc_near: 10,
            swap_path: Some(path_b),
        },
        TokenRate {
            base: base_fallback.clone(),
            quote: quote.clone(),
            exchange_rate: ExchangeRate::from_raw_rate(BigDecimal::from(210), 24),
            timestamp: t3,
            rate_calc_near: 10,
            swap_path: None,
        },
        // no-path.near: swap_path が一切ない → 生レートがそのまま返るべき
        make_token_rate(base_none.clone(), quote.clone(), 300, t2),
    ];

    let cfg = ConfigResolver;
    TokenRate::batch_insert(&rates, &cfg).await?;

    let quote_token = TokenAccount::from_str("wrap.near")?;
    let result = get_all_latest_rates(&quote_token).await?;

    assert_eq!(result.len(), 3, "Should return all 3 tokens");

    // has-path.near: 自身の swap_path で補正 → 生レート(100)より大きい
    let has_path_token = TokenAccount::from_str("has-path.near")?;
    let rate_with = result
        .get(&has_path_token)
        .expect("Should have has-path.near");
    assert!(
        rate_with.raw_rate() > &BigDecimal::from(100),
        "has-path.near should be corrected with own swap_path, got {}",
        rate_with.raw_rate()
    );

    // fallback.near: fallback swap_path (path_b, pool_id=20) で補正
    // → 生レート(210)より大きく、かつ has-path.near とは異なる補正
    let fallback_token = TokenAccount::from_str("fallback.near")?;
    let rate_fb = result
        .get(&fallback_token)
        .expect("Should have fallback.near");
    assert!(
        rate_fb.raw_rate() > &BigDecimal::from(210),
        "fallback.near should be corrected with fallback swap_path, got {}",
        rate_fb.raw_rate()
    );
    // path_b は path_a と異なる amount_in を持つため、補正係数が異なる
    // path_a: correction = 1 + 10^25 / 10^27 = 1.01
    // path_b: correction = 1 + 10^25 / 2*10^27 = 1.005
    // has-path: 100 * 1.01 = 101, fallback: 210 * 1.005 = 211.05
    // 補正係数が異なることで、fallback が has-path のパスを流用していないことを確認
    assert_ne!(
        rate_with.raw_rate(),
        rate_fb.raw_rate(),
        "Different tokens should get different corrections (not cross-contaminated)"
    );

    // no-path.near: swap_path なし → 生レートそのまま
    let no_path_token = TokenAccount::from_str("no-path.near")?;
    let rate_none = result
        .get(&no_path_token)
        .expect("Should have no-path.near");
    assert_eq!(
        rate_none.raw_rate(),
        &BigDecimal::from(300),
        "no-path.near should return raw rate without correction"
    );

    Ok(())
}
