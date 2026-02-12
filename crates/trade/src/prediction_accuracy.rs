use crate::Result;
use bigdecimal::BigDecimal;
use chrono::{NaiveDateTime, Utc};
use common::config;
use common::types::TimeRange;
use common::types::TokenPrice;
use common::types::{TokenAccount, TokenInAccount, TokenOutAccount};
use logging::*;
use num_traits::Zero;
use persistence::prediction_record::{NewPredictionRecord, PredictionRecord};
use persistence::token_rate::TokenRate;
use std::collections::BTreeMap;
use std::str::FromStr;

/// MAPE の閾値デフォルト値: これ以下なら予測が非常に正確（confidence = 1.0）
const DEFAULT_MAPE_EXCELLENT: f64 = 3.0;

/// MAPE の閾値デフォルト値: これ以上なら予測が不正確（confidence = 0.0）
const DEFAULT_MAPE_POOR: f64 = 15.0;

/// 評価済みレコードの保持日数デフォルト値
const DEFAULT_RECORD_RETENTION_DAYS: i64 = 30;

/// 未評価レコードの保持日数デフォルト値
const DEFAULT_UNEVALUATED_RETENTION_DAYS: i64 = 20;

/// 古い prediction_records を削除する。
///
/// 呼び出し元: evaluate_pending_predictions() の最後
/// タイミング: 評価完了後
///
/// 削除対象:
/// - 評価済みレコード: evaluated_at から PREDICTION_RECORD_RETENTION_DAYS 日以上経過
/// - 未評価レコード: target_time から PREDICTION_UNEVALUATED_RETENTION_DAYS 日以上経過
pub async fn cleanup_old_records() -> Result<(usize, usize)> {
    let log = DEFAULT.new(o!("function" => "cleanup_old_records"));

    let retention_days: i64 = config::get("PREDICTION_RECORD_RETENTION_DAYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_RECORD_RETENTION_DAYS);

    let unevaluated_retention_days: i64 = config::get("PREDICTION_UNEVALUATED_RETENTION_DAYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_UNEVALUATED_RETENTION_DAYS);

    let (evaluated_deleted, unevaluated_deleted) =
        PredictionRecord::delete_old_records(retention_days, unevaluated_retention_days).await?;

    if evaluated_deleted > 0 || unevaluated_deleted > 0 {
        info!(log, "cleaned up old prediction records";
            "evaluated_deleted" => evaluated_deleted,
            "unevaluated_deleted" => unevaluated_deleted,
            "retention_days" => retention_days,
            "unevaluated_retention_days" => unevaluated_retention_days
        );
    }

    Ok((evaluated_deleted, unevaluated_deleted))
}

/// MAPE を prediction_confidence [0.0, 1.0] に変換する（内部用）。
///
/// - MAPE ≤ excellent → 1.0（予測が正確 → Sharpe を信頼）
/// - MAPE ≥ poor → 0.0（予測が不正確 → RP に退避）
/// - 中間値は線形補間
fn mape_to_confidence(mape: f64, excellent: f64, poor: f64) -> f64 {
    ((poor - mape) / (poor - excellent)).clamp(0.0, 1.0)
}

/// 方向正解を判定: 予測と実際の変化方向が一致すれば true
fn is_direction_correct(
    prev_actual: &BigDecimal,
    predicted: &BigDecimal,
    actual: &BigDecimal,
) -> bool {
    let predicted_change = predicted - prev_actual;
    let actual_change = actual - prev_actual;

    // 両方の変化が同じ符号（または両方ゼロ）
    (predicted_change >= BigDecimal::zero()) == (actual_change >= BigDecimal::zero())
}

/// 複合スコア: MAPE と方向正解率を組み合わせ
fn calculate_composite_confidence(
    rolling_mape: f64,
    hit_rate: Option<f64>, // None = 方向データ不足
    mape_excellent: f64,
    mape_poor: f64,
) -> f64 {
    let mape_confidence = mape_to_confidence(rolling_mape, mape_excellent, mape_poor);

    match hit_rate {
        Some(hr) => {
            // 方向正解率: 50% = ランダム → 0.0, 100% → 1.0
            let direction_confidence = ((hr - 0.5) * 2.0).clamp(0.0, 1.0);
            // 重み付け合成（MAPE 60%, 方向 40%）
            0.6 * mape_confidence + 0.4 * direction_confidence
        }
        None => {
            // 方向データ不足時は MAPE のみ使用
            mape_confidence
        }
    }
}

/// 予測結果を prediction_records テーブルに記録する。
///
/// 呼び出し元: execute_portfolio_strategy()
/// タイミング: 予測ループ完了後、PortfolioData 構築前
///
/// DB 操作: INSERT INTO prediction_records (トークン数分)
pub async fn record_predictions(
    evaluation_period_id: &str,
    predictions: &BTreeMap<TokenOutAccount, TokenPrice>,
    quote_token: &str,
) -> Result<()> {
    let log = DEFAULT.new(o!("function" => "record_predictions"));

    let prediction_time = Utc::now().naive_utc();
    let target_time = prediction_time + chrono::Duration::hours(24);

    let records: Vec<NewPredictionRecord> = predictions
        .iter()
        .map(|(token, price)| NewPredictionRecord {
            evaluation_period_id: evaluation_period_id.to_string(),
            token: token.to_string(),
            quote_token: quote_token.to_string(),
            predicted_price: price.as_bigdecimal().clone(),
            prediction_time,
            target_time,
        })
        .collect();

    info!(log, "recording predictions"; "count" => records.len(), "target_time" => %target_time);
    PredictionRecord::batch_insert(&records).await?;

    Ok(())
}

/// 過去の予測を実績と比較して精度を評価する。
///
/// 呼び出し元: execute_portfolio_strategy() 内で tokio::spawn
/// タイミング: Chronos API 予測取得と並行実行
///
/// 戻り値: Option<(f64, f64)>
///   - Some((rolling_mape, confidence)): 評価済み >= MIN_SAMPLES なら直近 N 件の平均 MAPE と confidence
///   - None: データ不足
pub async fn evaluate_pending_predictions() -> Result<Option<(f64, f64)>> {
    let log = DEFAULT.new(o!("function" => "evaluate_pending_predictions"));

    let tolerance_minutes: i64 = config::get("PREDICTION_EVAL_TOLERANCE_MINUTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);

    let window: i64 = config::get("PREDICTION_ACCURACY_WINDOW")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);

    let min_samples: usize = config::get("PREDICTION_ACCURACY_MIN_SAMPLES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);

    // decimals 取得コールバック
    let get_decimals = super::make_get_decimals();

    // 未評価 & target_time 経過済みのレコードを取得
    let pending = PredictionRecord::get_pending_evaluations().await?;

    if pending.is_empty() {
        debug!(log, "no pending predictions to evaluate");
    } else {
        info!(log, "evaluating pending predictions"; "count" => pending.len());
    }

    let mut evaluated_count = 0u32;

    for record in &pending {
        let token = match TokenAccount::from_str(&record.token) {
            Ok(t) => t,
            Err(e) => {
                warn!(log, "failed to parse token"; "token" => &record.token, "error" => %e);
                continue;
            }
        };
        let token_out: TokenOutAccount = token.into();
        let quote = match TokenAccount::from_str(&record.quote_token) {
            Ok(t) => t,
            Err(e) => {
                warn!(log, "failed to parse quote_token"; "quote_token" => &record.quote_token, "error" => %e);
                continue;
            }
        };
        let quote_in: TokenInAccount = quote.into();

        // 実績価格を取得
        let actual_price = match get_actual_price_at(
            &token_out,
            &quote_in,
            record.target_time,
            tolerance_minutes,
            &get_decimals,
        )
        .await
        {
            Ok(Some(price)) => price,
            Ok(None) => {
                debug!(log, "no actual price data for evaluation, skipping";
                        "token" => &record.token, "target_time" => %record.target_time);
                continue;
            }
            Err(e) => {
                warn!(log, "failed to get actual price"; "token" => &record.token, "error" => %e);
                continue;
            }
        };

        let predicted = TokenPrice::from_near_per_token(record.predicted_price.clone());

        // MAPE = |predicted - actual| / actual * 100
        let actual_bd = actual_price.as_bigdecimal();
        let predicted_bd = predicted.as_bigdecimal();

        if actual_bd.is_zero() {
            warn!(log, "actual price is zero, skipping"; "token" => &record.token);
            continue;
        }

        let diff = predicted_bd - actual_bd;
        let absolute_error = diff.abs();
        let mape_bd = &absolute_error / actual_bd * BigDecimal::from(100);
        let mape: f64 = mape_bd.to_string().parse().unwrap_or(f64::MAX);

        debug!(log, "evaluated prediction";
            "token" => &record.token,
            "predicted" => %predicted,
            "actual" => %actual_price,
            "mape" => format!("{:.2}%", mape)
        );

        if let Err(e) =
            PredictionRecord::update_evaluation(record.id, actual_bd.clone(), mape, absolute_error)
                .await
        {
            warn!(log, "failed to update evaluation"; "id" => record.id, "error" => %e);
            continue;
        }

        evaluated_count += 1;
    }

    if evaluated_count > 0 {
        info!(log, "evaluation complete"; "evaluated" => evaluated_count);
    }

    // 古いレコードを削除（エラーは警告のみで続行）
    if let Err(e) = cleanup_old_records().await {
        warn!(log, "failed to cleanup old records"; "error" => %e);
    }

    // 直近 N 件の評価済みレコードから rolling MAPE と方向正解率を算出
    let recent = PredictionRecord::get_recent_evaluated(window).await?;

    if recent.len() < min_samples {
        debug!(log, "insufficient samples for rolling metrics";
            "available" => recent.len(), "required" => min_samples);
        return Ok(None);
    }

    // MAPE の収集
    let mape_values: Vec<f64> = recent.iter().filter_map(|r| r.mape).collect();

    if mape_values.len() < min_samples {
        return Ok(None);
    }

    let rolling_mape: f64 = mape_values.iter().sum::<f64>() / mape_values.len() as f64;

    // 方向正解率の計算（各レコードについて前レコードを参照）
    let mut direction_correct_count = 0usize;
    let mut direction_total_count = 0usize;

    for record in &recent {
        // actual_price と predicted_price が必要
        let Some(actual) = &record.actual_price else {
            continue;
        };

        // 前のレコードを取得
        let prev = match PredictionRecord::get_previous_evaluated(&record.token, record.target_time)
            .await
        {
            Ok(Some(p)) => p,
            Ok(None) => continue, // 前レコードなし（初回など）
            Err(e) => {
                debug!(log, "failed to get previous record"; "error" => %e);
                continue;
            }
        };

        let Some(prev_actual) = prev.actual_price else {
            continue;
        };

        // 方向判定
        if is_direction_correct(&prev_actual, &record.predicted_price, actual) {
            direction_correct_count += 1;
        }
        direction_total_count += 1;
    }

    // hit_rate 計算（十分なサンプルがあれば）
    let hit_rate = if direction_total_count >= min_samples {
        Some(direction_correct_count as f64 / direction_total_count as f64)
    } else {
        None
    };

    // 環境変数からしきい値を取得
    let mape_excellent: f64 = config::get("PREDICTION_MAPE_EXCELLENT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_MAPE_EXCELLENT);

    let mape_poor: f64 = config::get("PREDICTION_MAPE_POOR")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_MAPE_POOR);

    // 複合 confidence 計算
    let confidence =
        calculate_composite_confidence(rolling_mape, hit_rate, mape_excellent, mape_poor);

    info!(log, "prediction accuracy calculated";
        "rolling_mape" => format!("{:.2}%", rolling_mape),
        "hit_rate" => hit_rate.map(|h| format!("{:.1}%", h * 100.0)),
        "direction_samples" => direction_total_count,
        "confidence" => format!("{:.3}", confidence),
        "thresholds" => format!("excellent={}, poor={}", mape_excellent, mape_poor),
    );

    Ok(Some((rolling_mape, confidence)))
}

/// target_time に最も近い実績価格を token_rates から取得し TokenPrice に変換する。
async fn get_actual_price_at(
    token: &TokenOutAccount,
    quote_token: &TokenInAccount,
    target_time: NaiveDateTime,
    tolerance_minutes: i64,
    get_decimals: &persistence::token_rate::GetDecimalsFn,
) -> Result<Option<TokenPrice>> {
    let range = TimeRange {
        start: target_time - chrono::Duration::minutes(tolerance_minutes),
        end: target_time + chrono::Duration::minutes(tolerance_minutes),
    };
    let rates =
        TokenRate::get_rates_in_time_range(&range, token, quote_token, get_decimals).await?;

    let closest = rates
        .into_iter()
        .min_by_key(|r| (r.timestamp - target_time).num_seconds().unsigned_abs());

    Ok(closest.map(|r| r.to_spot_rate().to_price()))
}

#[cfg(test)]
mod tests;
