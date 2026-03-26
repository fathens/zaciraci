use super::*;
use chrono::NaiveDate;
use common::config::ConfigResolver;

fn make_date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).expect("valid test date")
}

/// start_date > end_date の場合は早期エラーを返す（DB不要）
#[tokio::test]
async fn invalid_date_range_returns_error() {
    let start = make_date(2026, 3, 20);
    let end = make_date(2026, 3, 15);
    let cfg = ConfigResolver;

    let result = generate_predictions_for_range(start, end, &cfg).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("invalid date range"),
        "expected 'invalid date range' error, got: {err}"
    );
}

/// 期間内の既存予測レコードが削除されること（削除件数を検証）
#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial]
#[ignore = "requires run_test environment with DB access"]
async fn deletes_existing_predictions_in_range() {
    use persistence::prediction_record::PredictionRecord;

    let start = make_date(2026, 3, 15);
    let end = make_date(2026, 3, 16);

    let start_naive = start.and_time(NaiveTime::MIN);
    let buffer = chrono::TimeDelta::hours(PREDICTION_HORIZON_HOURS as i64);
    let end_naive = end.and_time(NaiveTime::MIN) + buffer;

    // テスト用レコードを事前挿入（範囲内の target_time）
    let mid_target = start_naive + chrono::TimeDelta::hours(12);
    let record = persistence::prediction_record::NewPredictionRecord {
        token: "test_del.near".to_string(),
        quote_token: "wrap.near".to_string(),
        predicted_price: bigdecimal::BigDecimal::from(100),
        data_cutoff_time: start_naive - chrono::TimeDelta::hours(24),
        target_time: mid_target,
    };
    PredictionRecord::batch_insert(&[record])
        .await
        .expect("insert test record");

    // 範囲内にレコードが存在することを確認
    let before = PredictionRecord::get_pending_evaluations_as_of(end_naive)
        .await
        .expect("query before");
    let count_before = before.iter().filter(|r| r.token == "test_del.near").count();
    assert!(
        count_before >= 1,
        "should have at least 1 test record before deletion"
    );

    // generate_predictions_for_range は最初に範囲内を削除する
    let cfg = ConfigResolver;
    let _ = generate_predictions_for_range(start, end, &cfg).await;

    // 事前挿入したレコードが削除されていることを確認
    let after = PredictionRecord::get_pending_evaluations_as_of(end_naive)
        .await
        .expect("query after");
    let count_after = after
        .iter()
        .filter(|r| r.token == "test_del.near" && r.target_time == mid_target)
        .count();
    assert_eq!(
        count_after, 0,
        "test record should be deleted by generate_predictions_for_range"
    );
}
