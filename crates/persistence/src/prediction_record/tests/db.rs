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

    // t1: target_time = base, data_cutoff_time = base - 24h
    let t1_target = base;
    let t1_prediction = base - chrono::TimeDelta::hours(24);

    // t2: target_time = base + 1h
    let t2_target = base + chrono::TimeDelta::hours(1);
    let t2_prediction = base - chrono::TimeDelta::hours(23);

    // t3: target_time = base + 2h
    let t3_target = base + chrono::TimeDelta::hours(2);
    let t3_prediction = base - chrono::TimeDelta::hours(22);

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
    let eval_r1 = base + chrono::TimeDelta::hours(10);
    let eval_r2 = base + chrono::TimeDelta::hours(5);
    let eval_r3 = base + chrono::TimeDelta::hours(3);
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
        let target = base + chrono::TimeDelta::hours(i);
        let prediction = target - chrono::TimeDelta::hours(24);
        insert_evaluated_record(token, quote, 100 + i, 105 + i, prediction, target).await?;
    }

    let token_out = TokenAccount::from_str(token)?.into();

    // limit=3: 最新の 3 件が返ること
    let results = PredictionRecord::get_recent_evaluated_for_tokens(3, &[token_out]).await?;
    assert_eq!(results.len(), 3);

    // target_time DESC で最新 3 件: base+4h, base+3h, base+2h
    let expected_targets = [
        base + chrono::TimeDelta::hours(4),
        base + chrono::TimeDelta::hours(3),
        base + chrono::TimeDelta::hours(2),
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
        let target = base + chrono::TimeDelta::hours(i);
        let prediction = target - chrono::TimeDelta::hours(24);
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
    let prediction = base - chrono::TimeDelta::hours(24);
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
    let prediction1 = base - chrono::TimeDelta::hours(24);
    insert_evaluated_record(token, quote, 100, 105, prediction1, target1).await?;

    let target2 = base + chrono::TimeDelta::hours(1);
    let prediction2 = base - chrono::TimeDelta::hours(23);
    insert_evaluated_record(token, quote, 110, 108, prediction2, target2).await?;

    // 未評価レコード 1 件（evaluated_at = NULL）
    let target3 = base + chrono::TimeDelta::hours(2);
    let prediction3 = base - chrono::TimeDelta::hours(22);
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
    let past_prediction = base - chrono::TimeDelta::hours(48);
    let past_target = base - chrono::TimeDelta::hours(24);
    insert_unevaluated_record(token, quote, 100, past_prediction, past_target).await?;

    // target_time が as_of より後（含まれるべき）
    let future_prediction = base - chrono::TimeDelta::hours(12);
    let future_target = base + chrono::TimeDelta::hours(12);
    insert_unevaluated_record(token, quote, 200, future_prediction, future_target).await?;

    let as_of = base;
    let results = PredictionRecord::get_latest_fresh_predictions(&[tok(token)], as_of).await?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].predicted_price, BigDecimal::from(200));

    Ok(())
}

/// 最新1件選択: 同一トークンに複数予測がある場合、最新の data_cutoff_time が返ること
#[tokio::test]
#[serial]
async fn test_fresh_predictions_returns_latest_per_token() -> Result<()> {
    clean_table().await?;

    let base = base_time();
    let token = "token_a.near";
    let quote = "wrap.near";

    // 同一トークン、同一 target_time だが data_cutoff_time が異なる
    let target = base + chrono::TimeDelta::hours(24);
    let older_prediction = base - chrono::TimeDelta::hours(2);
    let newer_prediction = base;
    insert_unevaluated_record(token, quote, 100, older_prediction, target).await?;
    insert_unevaluated_record(token, quote, 200, newer_prediction, target).await?;

    let as_of = base - chrono::TimeDelta::hours(1);
    let results = PredictionRecord::get_latest_fresh_predictions(&[tok(token)], as_of).await?;

    assert_eq!(
        results.len(),
        1,
        "Should return exactly one record per token"
    );
    assert_eq!(
        results[0].predicted_price,
        BigDecimal::from(200),
        "Should return the prediction with the latest data_cutoff_time"
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

    let target = base + chrono::TimeDelta::hours(24);
    let data_cutoff_time = base;

    insert_unevaluated_record(token_a, quote, 100, data_cutoff_time, target).await?;
    insert_unevaluated_record(token_b, quote, 200, data_cutoff_time, target).await?;

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
    let target = base + chrono::TimeDelta::hours(24);
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
    let data_cutoff_time = base - chrono::TimeDelta::hours(24);
    insert_unevaluated_record(token, quote, 100, data_cutoff_time, base).await?;

    let results = PredictionRecord::get_latest_fresh_predictions(&[tok(token)], base).await?;

    assert!(
        results.is_empty(),
        "Record with target_time == as_of should be excluded (gt, not gte)"
    );

    Ok(())
}

// ── delete_by_target_time_range ──

/// start > end の場合はエラーを返すこと
#[tokio::test]
#[serial]
async fn test_delete_by_target_time_range_invalid_range() -> Result<()> {
    let base = base_time();
    let start = base + chrono::TimeDelta::hours(10);
    let end = base;

    let result = PredictionRecord::delete_by_target_time_range(start, end).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("invalid range"),
        "expected 'invalid range' error, got: {err}"
    );

    Ok(())
}

/// 空テーブルでも正常に 0 件削除として返ること
#[tokio::test]
#[serial]
async fn test_delete_by_target_time_range_empty_table() -> Result<()> {
    clean_table().await?;

    let base = base_time();
    let deleted =
        PredictionRecord::delete_by_target_time_range(base, base + chrono::TimeDelta::hours(24))
            .await?;

    assert_eq!(deleted, 0);

    Ok(())
}

/// start == end（1点）の場合、target_time がちょうどその時刻のレコードのみ削除されること
#[tokio::test]
#[serial]
async fn test_delete_by_target_time_range_single_point() -> Result<()> {
    clean_table().await?;

    let base = base_time();
    let token = "token_del_point.near";
    let quote = "wrap.near";

    // target_time = base のレコード（削除対象）
    let prediction = base - chrono::TimeDelta::hours(24);
    insert_unevaluated_record(token, quote, 100, prediction, base).await?;

    // target_time = base + 1h のレコード（削除対象外）
    let target2 = base + chrono::TimeDelta::hours(1);
    insert_unevaluated_record(token, quote, 200, prediction, target2).await?;

    let deleted = PredictionRecord::delete_by_target_time_range(base, base).await?;

    assert_eq!(
        deleted, 1,
        "Should delete exactly 1 record at the exact point"
    );

    // 残りのレコードが target2 のものであることを確認
    let remaining =
        PredictionRecord::get_pending_evaluations_as_of(base + chrono::TimeDelta::hours(2)).await?;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].target_time, target2);

    Ok(())
}

/// inclusive range: start と end ちょうどのレコードが削除に含まれること（ge + le）
#[tokio::test]
#[serial]
async fn test_delete_by_target_time_range_inclusive_boundary() -> Result<()> {
    clean_table().await?;

    let base = base_time();
    let token = "token_del_boundary.near";
    let quote = "wrap.near";
    let prediction = base - chrono::TimeDelta::hours(24);

    let start = base;
    let end = base + chrono::TimeDelta::hours(3);

    // target_time = start - 1s（範囲外、保持されるべき）
    let before = start - chrono::TimeDelta::seconds(1);
    insert_unevaluated_record(token, quote, 10, prediction, before).await?;

    // target_time = start ちょうど（範囲内、削除されるべき）
    insert_unevaluated_record(token, quote, 100, prediction, start).await?;

    // target_time = start + 1h（範囲内、削除されるべき）
    let middle = start + chrono::TimeDelta::hours(1);
    insert_unevaluated_record(token, quote, 150, prediction, middle).await?;

    // target_time = end ちょうど（範囲内、削除されるべき）
    insert_unevaluated_record(token, quote, 200, prediction, end).await?;

    // target_time = end + 1s（範囲外、保持されるべき）
    let after = end + chrono::TimeDelta::seconds(1);
    insert_unevaluated_record(token, quote, 300, prediction, after).await?;

    let deleted = PredictionRecord::delete_by_target_time_range(start, end).await?;

    assert_eq!(
        deleted, 3,
        "Should delete records at start, middle, and end (inclusive)"
    );

    // 残りのレコードが範囲外の 2 件であることを確認
    let remaining =
        PredictionRecord::get_pending_evaluations_as_of(after + chrono::TimeDelta::hours(1))
            .await?;
    assert_eq!(remaining.len(), 2);

    let prices: Vec<_> = remaining.iter().map(|r| &r.predicted_price).collect();
    assert!(prices.contains(&&BigDecimal::from(10)));
    assert!(prices.contains(&&BigDecimal::from(300)));

    Ok(())
}

// ── get_pending_evaluations_as_of ──

/// as_of == target_time のレコードが含まれること（le の確認）
#[tokio::test]
#[serial]
async fn test_pending_evaluations_as_of_boundary_included() -> Result<()> {
    clean_table().await?;

    let base = base_time();
    let token = "token_pending_boundary.near";
    let quote = "wrap.near";
    let prediction = base - chrono::TimeDelta::hours(24);

    // target_time = base（as_of = base で le なので含まれるべき）
    insert_unevaluated_record(token, quote, 100, prediction, base).await?;

    let results = PredictionRecord::get_pending_evaluations_as_of(base).await?;

    assert_eq!(
        results.len(),
        1,
        "Record with target_time == as_of should be included (le)"
    );
    assert_eq!(results[0].predicted_price, BigDecimal::from(100));

    Ok(())
}

/// target_time > as_of のレコードは除外されること
#[tokio::test]
#[serial]
async fn test_pending_evaluations_as_of_future_excluded() -> Result<()> {
    clean_table().await?;

    let base = base_time();
    let token = "token_pending_future.near";
    let quote = "wrap.near";
    let prediction = base - chrono::TimeDelta::hours(24);

    // target_time = base（含まれるべき）
    insert_unevaluated_record(token, quote, 100, prediction, base).await?;

    // target_time = base + 1h（除外されるべき）
    let future_target = base + chrono::TimeDelta::hours(1);
    insert_unevaluated_record(token, quote, 200, prediction, future_target).await?;

    let results = PredictionRecord::get_pending_evaluations_as_of(base).await?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].predicted_price, BigDecimal::from(100));

    Ok(())
}

/// evaluated_at が非 NULL のレコードは除外されること
#[tokio::test]
#[serial]
async fn test_pending_evaluations_as_of_excludes_evaluated() -> Result<()> {
    clean_table().await?;

    let base = base_time();
    let token = "token_pending_eval.near";
    let quote = "wrap.near";
    let prediction = base - chrono::TimeDelta::hours(24);

    // 未評価レコード（含まれるべき）
    insert_unevaluated_record(token, quote, 100, prediction, base).await?;

    // 評価済みレコード（除外されるべき）
    let target2 = base + chrono::TimeDelta::hours(1);
    insert_evaluated_record(token, quote, 200, 210, prediction, target2).await?;

    // as_of を十分先に設定して両方の target_time を包含
    let as_of = base + chrono::TimeDelta::hours(2);
    let results = PredictionRecord::get_pending_evaluations_as_of(as_of).await?;

    assert_eq!(results.len(), 1, "Should return only unevaluated records");
    assert_eq!(results[0].predicted_price, BigDecimal::from(100));
    assert!(results[0].evaluated_at.is_none());

    Ok(())
}

/// target_time ASC でソートされて返ること
#[tokio::test]
#[serial]
async fn test_pending_evaluations_as_of_ordered_by_target_time_asc() -> Result<()> {
    clean_table().await?;

    let base = base_time();
    let token = "token_pending_order.near";
    let quote = "wrap.near";
    let prediction = base - chrono::TimeDelta::hours(24);

    // 逆順に挿入（target_time が新しい方を先に）
    let t3 = base + chrono::TimeDelta::hours(2);
    let t1 = base;
    let t2 = base + chrono::TimeDelta::hours(1);

    insert_unevaluated_record(token, quote, 300, prediction, t3).await?;
    insert_unevaluated_record(token, quote, 100, prediction, t1).await?;
    insert_unevaluated_record(token, quote, 200, prediction, t2).await?;

    let as_of = base + chrono::TimeDelta::hours(3);
    let results = PredictionRecord::get_pending_evaluations_as_of(as_of).await?;

    assert_eq!(results.len(), 3);
    assert_eq!(
        results[0].target_time, t1,
        "First result should have earliest target_time"
    );
    assert_eq!(results[1].target_time, t2);
    assert_eq!(results[2].target_time, t3);

    Ok(())
}

/// target_time 優先: 同一トークン・同一 data_cutoff_time で異なる target_time がある場合、
/// 最新の target_time を持つレコードが返ること
#[tokio::test]
#[serial]
async fn test_fresh_predictions_prefers_latest_target_time() -> Result<()> {
    clean_table().await?;

    let base = base_time();
    let token = "token_a.near";
    let quote = "wrap.near";

    let data_cutoff_time = base;

    // 近い未来の target_time（+12h）
    let near_target = base + chrono::TimeDelta::hours(12);
    insert_unevaluated_record(token, quote, 100, data_cutoff_time, near_target).await?;

    // 遠い未来の target_time（+36h）
    let far_target = base + chrono::TimeDelta::hours(36);
    insert_unevaluated_record(token, quote, 200, data_cutoff_time, far_target).await?;

    let as_of = base;
    let results = PredictionRecord::get_latest_fresh_predictions(&[tok(token)], as_of).await?;

    assert_eq!(
        results.len(),
        1,
        "Should return exactly one record per token"
    );
    assert_eq!(
        results[0].predicted_price,
        BigDecimal::from(200),
        "Should prefer the prediction with the latest target_time"
    );

    Ok(())
}
