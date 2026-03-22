use super::*;
use common::types::{TokenAccount, TokenOutAccount};
use std::str::FromStr;

/// テスト用ヘルパー: 文字列から TokenOutAccount を作成
fn tok(s: &str) -> TokenOutAccount {
    TokenAccount::from_str(s).unwrap().into()
}

/// テスト用の基準時刻を作成
fn base_time() -> NaiveDateTime {
    chrono::DateTime::from_timestamp(1_700_000_000, 0)
        .unwrap()
        .naive_utc()
}

/// ソート順テスト: get_recent_evaluated_for_tokens が target_time DESC で返すこと
///
/// evaluated_at の順序が target_time と異なるデータで検証し、
/// ORDER BY evaluated_at DESC に戻すとこのテストが失敗することを保証する
#[tokio::test]
#[serial]
async fn test_sort_order_by_target_time_desc() -> Result<()> {
    clean_table().await?;

    let base = base_time();
    let token = "token_a.near";
    let quote = "wrap.near";

    // target_time が古い順に t1 < t2 < t3
    // evaluated_at は意図的に target_time と逆順にする:
    //   t1 の evaluated_at が最も新しく、t3 の evaluated_at が最も古い
    // これにより ORDER BY evaluated_at DESC と ORDER BY target_time DESC で
    // 結果の順序が異なることを保証する

    // t1: target_time = base, prediction_time = base - 24h
    let t1_target = base;
    let t1_prediction = base - chrono::Duration::hours(24);

    // t2: target_time = base + 1h
    let t2_target = base + chrono::Duration::hours(1);
    let t2_prediction = base - chrono::Duration::hours(23);

    // t3: target_time = base + 2h
    let t3_target = base + chrono::Duration::hours(2);
    let t3_prediction = base - chrono::Duration::hours(22);

    // 挿入（evaluated_at はヘルパー内で target_time + 1h に設定されるが、
    // ここでは手動で上書きして逆順にする）
    let r1 = insert_evaluated_record(token, quote, 100, 105, t1_prediction, t1_target).await?;
    let r2 = insert_evaluated_record(token, quote, 110, 108, t2_prediction, t2_target).await?;
    let r3 = insert_evaluated_record(token, quote, 120, 115, t3_prediction, t3_target).await?;

    // evaluated_at を意図的に target_time と逆順に設定
    // r1 (oldest target) → evaluated_at = base + 10h (newest)
    // r2 (middle target) → evaluated_at = base + 5h (middle)
    // r3 (newest target) → evaluated_at = base + 3h (oldest)
    let conn = connection_pool::get().await?;
    let r1_id = r1.id;
    let r2_id = r2.id;
    let r3_id = r3.id;
    let eval_r1 = base + chrono::Duration::hours(10);
    let eval_r2 = base + chrono::Duration::hours(5);
    let eval_r3 = base + chrono::Duration::hours(3);
    conn.interact(move |conn| {
        diesel::update(prediction_records::table.filter(prediction_records::id.eq(r1_id)))
            .set(prediction_records::evaluated_at.eq(eval_r1))
            .execute(conn)?;
        diesel::update(prediction_records::table.filter(prediction_records::id.eq(r2_id)))
            .set(prediction_records::evaluated_at.eq(eval_r2))
            .execute(conn)?;
        diesel::update(prediction_records::table.filter(prediction_records::id.eq(r3_id)))
            .set(prediction_records::evaluated_at.eq(eval_r3))
            .execute(conn)?;
        Ok::<_, diesel::result::Error>(())
    })
    .await
    .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

    // クエリ実行
    let token_out = TokenAccount::from_str(token)?.into();
    let results = PredictionRecord::get_recent_evaluated_for_tokens(10, &[token_out]).await?;

    assert_eq!(results.len(), 3);

    // target_time DESC 順: r3 (base+2h), r2 (base+1h), r1 (base)
    // evaluated_at DESC 順だと: r1 (base+10h), r2 (base+5h), r3 (base+3h) — これは不正解
    assert_eq!(
        results[0].target_time, t3_target,
        "First result should have newest target_time"
    );
    assert_eq!(
        results[1].target_time, t2_target,
        "Second result should have middle target_time"
    );
    assert_eq!(
        results[2].target_time, t1_target,
        "Third result should have oldest target_time"
    );

    Ok(())
}

/// LIMIT テスト: limit=N で正しく N 件に切り詰められ、
/// 切り詰め対象が target_time 基準であること（最新 N 件が返る）
#[tokio::test]
#[serial]
async fn test_limit_returns_most_recent_by_target_time() -> Result<()> {
    clean_table().await?;

    let base = base_time();
    let token = "token_b.near";
    let quote = "wrap.near";

    // 5件のレコードを挿入
    for i in 0i64..5 {
        let target = base + chrono::Duration::hours(i);
        let prediction = target - chrono::Duration::hours(24);
        insert_evaluated_record(token, quote, 100 + i, 105 + i, prediction, target).await?;
    }

    let token_out = TokenAccount::from_str(token)?.into();

    // limit=3: 最新の 3 件が返ること
    let results = PredictionRecord::get_recent_evaluated_for_tokens(3, &[token_out]).await?;
    assert_eq!(results.len(), 3);

    // target_time DESC で最新 3 件: base+4h, base+3h, base+2h
    let expected_targets = [
        base + chrono::Duration::hours(4),
        base + chrono::Duration::hours(3),
        base + chrono::Duration::hours(2),
    ];

    for (i, expected) in expected_targets.iter().enumerate() {
        assert_eq!(
            results[i].target_time, *expected,
            "Result {i} should have target_time {expected}"
        );
    }

    Ok(())
}

/// トークンフィルタテスト: 指定トークンのレコードのみ返すこと
#[tokio::test]
#[serial]
async fn test_token_filter() -> Result<()> {
    clean_table().await?;

    let base = base_time();
    let quote = "wrap.near";

    // 2つの異なるトークンのレコードを挿入
    let token_a = "token_filter_a.near";
    let token_b = "token_filter_b.near";

    for i in 0..3 {
        let target = base + chrono::Duration::hours(i);
        let prediction = target - chrono::Duration::hours(24);
        insert_evaluated_record(token_a, quote, 100, 105, prediction, target).await?;
        insert_evaluated_record(token_b, quote, 200, 210, prediction, target).await?;
    }

    // token_a のみ指定
    let token_a_out = TokenAccount::from_str(token_a)?.into();
    let results = PredictionRecord::get_recent_evaluated_for_tokens(100, &[token_a_out]).await?;

    assert_eq!(results.len(), 3, "Should return only token_a records");
    for r in &results {
        assert_eq!(r.token, token_a, "All results should be for token_a");
    }

    // 両方指定
    let token_a_out = TokenAccount::from_str(token_a)?.into();
    let token_b_out = TokenAccount::from_str(token_b)?.into();
    let results =
        PredictionRecord::get_recent_evaluated_for_tokens(100, &[token_a_out, token_b_out]).await?;

    assert_eq!(
        results.len(),
        6,
        "Should return all records for both tokens"
    );

    Ok(())
}

/// 空トークンリストテスト: 空 Vec → 空結果
#[tokio::test]
#[serial]
async fn test_empty_token_list_returns_empty() -> Result<()> {
    clean_table().await?;

    let base = base_time();
    let token = "token_empty.near";
    let quote = "wrap.near";

    // レコードを1件挿入しておく
    let target = base;
    let prediction = base - chrono::Duration::hours(24);
    insert_evaluated_record(token, quote, 100, 105, prediction, target).await?;

    // 空トークンリストで呼び出し
    let results = PredictionRecord::get_recent_evaluated_for_tokens(100, &[]).await?;

    assert!(
        results.is_empty(),
        "Empty token list should return empty results"
    );

    Ok(())
}

/// evaluated_at NULL 除外テスト: 未評価レコードが結果に含まれないこと
#[tokio::test]
#[serial]
async fn test_excludes_unevaluated_records() -> Result<()> {
    clean_table().await?;

    let base = base_time();
    let token = "token_eval.near";
    let quote = "wrap.near";

    // 評価済みレコード 2 件
    let target1 = base;
    let prediction1 = base - chrono::Duration::hours(24);
    insert_evaluated_record(token, quote, 100, 105, prediction1, target1).await?;

    let target2 = base + chrono::Duration::hours(1);
    let prediction2 = base - chrono::Duration::hours(23);
    insert_evaluated_record(token, quote, 110, 108, prediction2, target2).await?;

    // 未評価レコード 1 件（evaluated_at = NULL）
    let target3 = base + chrono::Duration::hours(2);
    let prediction3 = base - chrono::Duration::hours(22);
    insert_unevaluated_record(token, quote, 120, prediction3, target3).await?;

    let token_out = TokenAccount::from_str(token)?.into();
    let results = PredictionRecord::get_recent_evaluated_for_tokens(100, &[token_out]).await?;

    assert_eq!(
        results.len(),
        2,
        "Should return only evaluated records, not the unevaluated one"
    );
    for r in &results {
        assert!(
            r.evaluated_at.is_some(),
            "All returned records should have evaluated_at set"
        );
    }

    Ok(())
}

// ── get_latest_fresh_predictions ──

/// target_time フィルタ: target_time が as_of 以前のレコードは除外されること
#[tokio::test]
#[serial]
async fn test_fresh_predictions_filters_by_target_time() -> Result<()> {
    clean_table().await?;

    let base = base_time();
    let token = "token_a.near";
    let quote = "wrap.near";

    // target_time が as_of より前（除外されるべき）
    let past_prediction = base - chrono::Duration::hours(48);
    let past_target = base - chrono::Duration::hours(24);
    insert_unevaluated_record(token, quote, 100, past_prediction, past_target).await?;

    // target_time が as_of より後（含まれるべき）
    let future_prediction = base - chrono::Duration::hours(12);
    let future_target = base + chrono::Duration::hours(12);
    insert_unevaluated_record(token, quote, 200, future_prediction, future_target).await?;

    let as_of = base;
    let results = PredictionRecord::get_latest_fresh_predictions(&[tok(token)], as_of).await?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].predicted_price, BigDecimal::from(200));

    Ok(())
}

/// 最新1件選択: 同一トークンに複数予測がある場合、最新の prediction_time が返ること
#[tokio::test]
#[serial]
async fn test_fresh_predictions_returns_latest_per_token() -> Result<()> {
    clean_table().await?;

    let base = base_time();
    let token = "token_a.near";
    let quote = "wrap.near";

    // 同一トークン、同一 target_time だが prediction_time が異なる
    let target = base + chrono::Duration::hours(24);
    let older_prediction = base - chrono::Duration::hours(2);
    let newer_prediction = base;
    insert_unevaluated_record(token, quote, 100, older_prediction, target).await?;
    insert_unevaluated_record(token, quote, 200, newer_prediction, target).await?;

    let as_of = base - chrono::Duration::hours(1);
    let results = PredictionRecord::get_latest_fresh_predictions(&[tok(token)], as_of).await?;

    assert_eq!(
        results.len(),
        1,
        "Should return exactly one record per token"
    );
    assert_eq!(
        results[0].predicted_price,
        BigDecimal::from(200),
        "Should return the prediction with the latest prediction_time"
    );

    Ok(())
}

/// トークン分離: 異なるトークンの予測が正しく分離されること
#[tokio::test]
#[serial]
async fn test_fresh_predictions_separates_tokens() -> Result<()> {
    clean_table().await?;

    let base = base_time();
    let token_a = "token_a.near";
    let token_b = "token_b.near";
    let quote = "wrap.near";

    let target = base + chrono::Duration::hours(24);
    let prediction_time = base;

    insert_unevaluated_record(token_a, quote, 100, prediction_time, target).await?;
    insert_unevaluated_record(token_b, quote, 200, prediction_time, target).await?;

    let results =
        PredictionRecord::get_latest_fresh_predictions(&[tok(token_a), tok(token_b)], base).await?;

    assert_eq!(results.len(), 2, "Should return one record per token");

    let prices: std::collections::BTreeSet<_> =
        results.iter().map(|r| r.predicted_price.clone()).collect();
    assert!(prices.contains(&BigDecimal::from(100)));
    assert!(prices.contains(&BigDecimal::from(200)));

    Ok(())
}

/// 空トークンリスト: 空リストで空結果が返ること
#[tokio::test]
#[serial]
async fn test_fresh_predictions_empty_tokens() -> Result<()> {
    clean_table().await?;

    let base = base_time();
    let target = base + chrono::Duration::hours(24);
    insert_unevaluated_record("token_a.near", "wrap.near", 100, base, target).await?;

    let results = PredictionRecord::get_latest_fresh_predictions(&[], base).await?;

    assert!(
        results.is_empty(),
        "Empty token list should return empty results"
    );

    Ok(())
}

/// 境界値: target_time が as_of ちょうどのレコードは除外されること（gt の確認）
#[tokio::test]
#[serial]
async fn test_fresh_predictions_boundary_excluded() -> Result<()> {
    clean_table().await?;

    let base = base_time();
    let token = "token_a.near";
    let quote = "wrap.near";

    // target_time == as_of（ちょうど境界、gt なので除外されるべき）
    let prediction_time = base - chrono::Duration::hours(24);
    insert_unevaluated_record(token, quote, 100, prediction_time, base).await?;

    let results = PredictionRecord::get_latest_fresh_predictions(&[tok(token)], base).await?;

    assert!(
        results.is_empty(),
        "Record with target_time == as_of should be excluded (gt, not gte)"
    );

    Ok(())
}
