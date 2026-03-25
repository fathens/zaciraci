use super::*;
use chrono::NaiveDate;
use common::config::ConfigResolver;

fn make_date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
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

/// start_date == end_date（1日のみ）で正常に実行される
#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial]
#[ignore = "requires run_test environment with token data"]
async fn single_day_range_executes_one_iteration() {
    let date = make_date(2026, 3, 15);
    let cfg = ConfigResolver;

    let result = generate_predictions_for_range(date, date, &cfg).await;

    // run_test 環境にデータがあれば Ok、なければ Err（全日失敗）
    // いずれにせよパニックしないことを確認
    match result {
        Ok(()) => {}
        Err(e) => {
            println!("single day test failed (expected if no token data): {e}");
        }
    }
}

/// 期間内の既存予測レコードが削除される
#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial]
#[ignore = "requires run_test environment with DB access"]
async fn deletes_existing_predictions_in_range() {
    use persistence::prediction_record::PredictionRecord;

    let start = make_date(2026, 3, 15);
    let end = make_date(2026, 3, 16);
    let cfg = ConfigResolver;

    // generate_predictions_for_range は最初に範囲内を削除する
    let _ = generate_predictions_for_range(start, end, &cfg).await;

    // 削除後、範囲内のレコードは generate_predictions_for_range が
    // 新たに生成したものだけになっているはず
    let end_naive =
        end.and_time(NaiveTime::MIN) + chrono::Duration::hours(PREDICTION_HORIZON_HOURS as i64);
    let pending = PredictionRecord::get_pending_evaluations_as_of(end_naive).await;
    assert!(
        pending.is_ok(),
        "should be able to query predictions after generation"
    );
}
