use crate::Result;
use bigdecimal::BigDecimal;
use chrono::{NaiveDateTime, Utc};
use common::config::ConfigAccess;
use common::types::TimeRange;
use common::types::TokenPrice;
use common::types::{TokenAccount, TokenInAccount, TokenOutAccount};
use logging::*;
use num_traits::{ToPrimitive, Zero};
use persistence::prediction_record::{DbPredictionRecord, NewPredictionRecord, PredictionRecord};
use persistence::token_rate::TokenRate;
use std::collections::BTreeMap;
use std::str::FromStr;

/// 古い prediction_records を削除する。
///
/// 呼び出し元: evaluate_pending_predictions() の最後
/// タイミング: 評価完了後
///
/// 削除対象:
/// - 評価済みレコード: evaluated_at から PREDICTION_RECORD_RETENTION_DAYS 日以上経過
/// - 未評価レコード: target_time から PREDICTION_UNEVALUATED_RETENTION_DAYS 日以上経過
pub async fn cleanup_old_records(cfg: &impl ConfigAccess) -> Result<(usize, usize)> {
    let log = DEFAULT.new(o!("function" => "cleanup_old_records"));

    let retention_days = cfg.prediction_record_retention_days();

    let unevaluated_retention_days = cfg.prediction_unevaluated_retention_days();

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
    let range = poor - excellent;
    if range.abs() < 1e-9 {
        return if mape <= excellent { 1.0 } else { 0.0 };
    }
    ((poor - mape) / range).clamp(0.0, 1.0)
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
    quote_token: &TokenInAccount,
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

/// 過去の予測を実績と比較して精度を評価する（ハウスキーピング）。
///
/// 呼び出し元: start() の冒頭
/// タイミング: トレード戦略実行前
///
/// 戻り値: 評価したレコード数
pub async fn evaluate_pending_predictions(cfg: &impl ConfigAccess) -> Result<u32> {
    let log = DEFAULT.new(o!("function" => "evaluate_pending_predictions"));

    let tolerance_minutes = cfg.prediction_eval_tolerance_minutes();

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
        let Some(mape) = mape_bd.to_f64() else {
            warn!(log, "mape conversion failed, skipping"; "token" => &record.token);
            continue;
        };

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
    if let Err(e) = cleanup_old_records(cfg).await {
        warn!(log, "failed to cleanup old records"; "error" => %e);
    }

    Ok(evaluated_count)
}

/// ソート済みレコード（target_time DESC）の隣接ペアから方向正解率を計算する。
/// DB アクセスなしで計算（N+1 クエリ排除）。
fn calculate_direction_accuracy_for_records(records: &[DbPredictionRecord]) -> (usize, usize) {
    let mut correct = 0usize;
    let mut total = 0usize;
    for pair in records.windows(2) {
        let (Some(actual), Some(prev_actual)) = (&pair[0].actual_price, &pair[1].actual_price)
        else {
            continue;
        };
        if is_direction_correct(prev_actual, &pair[0].predicted_price, actual) {
            correct += 1;
        }
        total += 1;
    }
    (correct, total)
}

/// トークンごとの prediction confidence を計算する。
///
/// 1回の DB クエリで全トークンのレコードを取得し、Rust 側でグルーピング。
/// 各トークンの平均 MAPE と方向正解率から複合 confidence を算出。
///
/// 戻り値: BTreeMap<TokenOutAccount, f64>
///   - エントリあり: 十分なサンプルがあり confidence 計算済み
///   - エントリなし: データ不足（コールドスタート）
pub async fn calculate_per_token_confidence(
    tokens: &[TokenOutAccount],
    cfg: &impl ConfigAccess,
) -> BTreeMap<TokenOutAccount, f64> {
    let log = DEFAULT.new(o!("function" => "calculate_per_token_confidence"));
    let window = cfg.prediction_accuracy_window();
    let min_samples = cfg.prediction_accuracy_min_samples();
    let mape_excellent = cfg.prediction_mape_excellent();
    let mape_poor = cfg.prediction_mape_poor();

    // 1回の DB クエリで全トークンのレコードを取得
    let all_records = match PredictionRecord::get_recent_evaluated_for_tokens(
        window * tokens.len() as i64,
        tokens,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!(log, "failed to get prediction records"; "error" => %e);
            return BTreeMap::new();
        }
    };

    // Rust 側でトークンごとにグルーピング
    let mut by_token: BTreeMap<String, Vec<DbPredictionRecord>> =
        all_records.into_iter().fold(BTreeMap::new(), |mut map, r| {
            map.entry(r.token.clone()).or_default().push(r);
            map
        });

    // 各トークン内を target_time DESC でソート（方向判定に必要）
    for entries in by_token.values_mut() {
        entries.sort_by(|a, b| b.target_time.cmp(&a.target_time));
        entries.truncate(window as usize);
    }

    let mut result = BTreeMap::new();

    for token in tokens {
        let token_str = token.to_string();
        let records = by_token.get(&token_str);
        let mape_values: Vec<f64> = records
            .map(|rs| rs.iter().filter_map(|r| r.mape).collect())
            .unwrap_or_default();

        if mape_values.len() < min_samples {
            continue;
        }

        let avg_mape = mape_values.iter().sum::<f64>() / mape_values.len() as f64;

        let direction_data = records.map(|rs| calculate_direction_accuracy_for_records(rs));
        let hit_rate = direction_data.and_then(|(correct, total)| {
            if total >= min_samples {
                Some(correct as f64 / total as f64)
            } else {
                None
            }
        });

        let confidence =
            calculate_composite_confidence(avg_mape, hit_rate, mape_excellent, mape_poor);

        debug!(log, "token prediction confidence";
            "token" => %token_str,
            "avg_mape" => format!("{:.2}%", avg_mape),
            "hit_rate" => hit_rate.map(|h| format!("{:.1}%", h * 100.0)),
            "confidence" => format!("{:.3}", confidence)
        );

        result.insert(token.clone(), confidence);
    }

    result
}

/// target_time に最も近い実績価格を token_rates から取得し TokenPrice に変換する。
///
/// 実質ゼロのレートは無効データとして除外し、
/// 残りのうち target_time に最も近いものを返す。
async fn get_actual_price_at(
    token: &TokenOutAccount,
    quote_token: &TokenInAccount,
    target_time: NaiveDateTime,
    tolerance_minutes: i64,
) -> Result<Option<TokenPrice>> {
    let range = TimeRange {
        start: target_time - chrono::Duration::minutes(tolerance_minutes),
        end: target_time + chrono::Duration::minutes(tolerance_minutes),
    };
    let rates = TokenRate::get_rates_in_time_range(&range, token, quote_token).await?;

    let spot_rates = TokenRate::to_spot_rates(&rates);
    if spot_rates.is_empty() {
        return Ok(None);
    }
    let (_, closest_rate) = spot_rates
        .iter()
        .min_by_key(|(ts, _)| (*ts - target_time).num_seconds().unsigned_abs())
        .expect("spot_rates is non-empty (checked above)");
    Ok(Some(closest_rate.to_price()))
}

#[cfg(test)]
mod tests;
