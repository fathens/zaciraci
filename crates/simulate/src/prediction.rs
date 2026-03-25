use anyhow::Result;
use chrono::{NaiveDate, NaiveTime, TimeZone, Utc};
use common::algorithm::prediction::PREDICTION_HORIZON_HOURS;
use common::config::ConfigAccess;
use logging::*;
use persistence::prediction_record::PredictionRecord;

/// シミュレーション期間の予測を生成・評価する。
///
/// 処理フロー:
/// 1. 期間内の既存予測を削除（旧アルゴリズムの予測が優先されるのを防ぐ）
/// 2. 各日について: 前日予測を評価 → 当日予測を生成
/// 3. 最終日の予測を評価
pub async fn generate_predictions_for_range(
    start_date: NaiveDate,
    end_date: NaiveDate,
    cfg: &impl ConfigAccess,
) -> Result<()> {
    let log = DEFAULT.new(o!("function" => "generate_predictions_for_range"));

    if start_date > end_date {
        anyhow::bail!(
            "invalid date range: start_date ({}) > end_date ({})",
            start_date,
            end_date
        );
    }

    // 1. 削除範囲を計算（horizon ベース）
    let start_naive = start_date.and_time(NaiveTime::MIN);
    let buffer = chrono::Duration::hours(PREDICTION_HORIZON_HOURS as i64);
    let end_naive = end_date.and_time(NaiveTime::MIN) + buffer;

    let deleted = PredictionRecord::delete_by_target_time_range(start_naive, end_naive).await?;
    if deleted > 0 {
        info!(log, "deleted existing predictions in range"; "deleted" => deleted);
    }

    // 2. 各日について予測生成・評価
    let mut current_date = start_date;
    let mut success_count = 0u32;
    let mut fail_count = 0u32;
    let mut total_evaluated = 0u32;
    let mut total_generated = 0usize;

    while current_date <= end_date {
        let sim_day = Utc.from_utc_datetime(&current_date.and_time(NaiveTime::MIN));

        // 前日の予測を評価（初回はシミュレーション範囲外の未評価レコードも対象になる）
        match trade::prediction_accuracy::evaluate_predictions_as_of(sim_day, cfg).await {
            Ok(count) => total_evaluated += count,
            Err(e) => {
                warn!(log, "prediction evaluation failed"; "date" => %current_date, "error" => ?e);
            }
        }

        // 当日の予測を生成
        match trade::run_prediction_cycle(sim_day, cfg).await {
            Ok(count) => {
                total_generated += count;
                success_count += 1;
                info!(log, "predictions generated"; "date" => %current_date, "count" => count);
            }
            Err(e) => {
                fail_count += 1;
                warn!(log, "prediction generation failed, skipping"; "date" => %current_date, "error" => ?e);
            }
        }

        current_date += chrono::Duration::days(1);
    }

    // 3. 最終日の予測を評価
    let final_eval_day =
        Utc.from_utc_datetime(&(end_date + chrono::Duration::days(1)).and_time(NaiveTime::MIN));
    match trade::prediction_accuracy::evaluate_predictions_as_of(final_eval_day, cfg).await {
        Ok(count) => total_evaluated += count,
        Err(e) => {
            warn!(log, "final prediction evaluation failed"; "error" => ?e);
        }
    }

    info!(log, "prediction generation complete";
        "success_days" => success_count,
        "fail_days" => fail_count,
        "total_generated" => total_generated,
        "total_evaluated" => total_evaluated,
    );

    // 全日失敗の場合はエラー
    if success_count == 0 {
        return Err(anyhow::anyhow!(
            "all prediction generation days failed ({} days)",
            fail_count
        ));
    }

    Ok(())
}
