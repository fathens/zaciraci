use crate::Result;
use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
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

pub(crate) use common::algorithm::prediction::PREDICTION_HORIZON_HOURS;

/// 古い prediction_records を削除する。
///
/// 呼び出し元: evaluate_pending_predictions() の最後
/// タイミング: 評価完了後
///
/// 削除対象:
/// - 評価済みレコード: evaluated_at から PREDICTION_RECORD_RETENTION_DAYS 日以上経過
/// - 未評価レコード: target_time から PREDICTION_UNEVALUATED_RETENTION_DAYS 日以上経過
pub(crate) async fn cleanup_old_records(cfg: &impl ConfigAccess) -> Result<(usize, usize)> {
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
    debug_assert!(
        poor >= excellent,
        "poor ({poor}) must be >= excellent ({excellent})"
    );
    debug_assert!(
        mape >= 0.0 || !mape.is_finite(),
        "mape ({mape}) must be non-negative"
    );
    if !mape.is_finite() {
        // NaN → 0.0 (worst), ±Infinity → 0.0 (worst)
        // MAPE is non-negative by definition; any non-finite value indicates a bug
        return 0.0;
    }
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

/// BTreeMap から NewPredictionRecord の Vec を生成する（DB 非依存）。
fn build_prediction_records(
    predictions: &BTreeMap<TokenOutAccount, (TokenPrice, NaiveDateTime)>,
    quote_token: &TokenInAccount,
) -> Vec<NewPredictionRecord> {
    predictions
        .iter()
        .map(|(token, (price, data_cutoff_time))| {
            let target_time =
                *data_cutoff_time + chrono::TimeDelta::hours(PREDICTION_HORIZON_HOURS as i64);
            NewPredictionRecord {
                token: token.to_string(),
                quote_token: quote_token.to_string(),
                predicted_price: price.as_bigdecimal().clone(),
                data_cutoff_time: *data_cutoff_time,
                target_time,
            }
        })
        .collect()
}

/// 予測結果を prediction_records テーブルに記録する。
///
/// DB 操作: INSERT INTO prediction_records (トークン数分)
pub(crate) async fn record_predictions(
    predictions: &BTreeMap<TokenOutAccount, (TokenPrice, NaiveDateTime)>,
    quote_token: &TokenInAccount,
) -> Result<()> {
    let log = DEFAULT.new(o!("function" => "record_predictions"));

    let records = build_prediction_records(predictions, quote_token);

    info!(log, "recording predictions"; "count" => records.len());
    PredictionRecord::batch_insert(&records).await?;

    Ok(())
}

/// 過去の予測を実績と比較して精度を評価する（ハウスキーピング）。
///
/// 呼び出し元: run_predictions() の冒頭
/// タイミング: トレード戦略実行前
///
/// 戻り値: 評価したレコード数
pub(crate) async fn evaluate_pending_predictions(cfg: &impl ConfigAccess) -> Result<u32> {
    let count = evaluate_predictions_as_of(chrono::Utc::now(), cfg).await?;

    // 古いレコードを削除（エラーは警告のみで続行）
    if let Err(e) = cleanup_old_records(cfg).await {
        let log = DEFAULT.new(o!("function" => "evaluate_pending_predictions"));
        warn!(log, "failed to cleanup old records"; "error" => %e);
    }

    Ok(count)
}

/// 指定時刻基準で未評価の予測を実績と比較して評価する。
/// cleanup_old_records は呼ばない（呼び出し元が必要に応じて行う）。
///
/// `as_of` にはシミュレーション日時など過去の時点を指定する。
/// 未来日時を渡した場合、target_time 未到来の予測も評価対象になるが、
/// 実績データが存在しないためスキップされる。
///
/// 戻り値: 評価したレコード数
pub async fn evaluate_predictions_as_of(
    as_of: chrono::DateTime<chrono::Utc>,
    cfg: &impl ConfigAccess,
) -> Result<u32> {
    let log = DEFAULT.new(o!("function" => "evaluate_predictions_as_of"));

    let tolerance_minutes = cfg.prediction_eval_tolerance_minutes();

    // 未評価 & target_time 経過済みのレコードを取得
    let pending = PredictionRecord::get_pending_evaluations_as_of(as_of.naive_utc()).await?;

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

    Ok(evaluated_count)
}

/// ソート済みレコード（target_time DESC）の隣接ペアから方向正解率を計算する。
/// DB アクセスなしで計算（N+1 クエリ排除）。
///
/// 隣接レコード間の時間ギャップが `PREDICTION_HORIZON_HOURS` の 1.5 倍を超えるペアは
/// スキップする。予測は `data_cutoff_time` 基準の 24h 先を想定しており、ギャップが
/// 大きいと `prev_actual` が予測の基準時点から乖離し、方向比較の統計的意味が薄れるため。
fn calculate_direction_accuracy_for_records(
    records: &[DbPredictionRecord],
    log: &slog::Logger,
) -> (usize, usize) {
    debug_assert!(
        records
            .windows(2)
            .all(|w| w[0].target_time >= w[1].target_time),
        "records must be sorted by target_time DESC"
    );
    let max_gap = chrono::TimeDelta::hours((PREDICTION_HORIZON_HOURS as i64 * 3) / 2);
    let mut correct = 0usize;
    let mut total = 0usize;
    for pair in records.windows(2) {
        let gap = pair[0].target_time - pair[1].target_time;
        if gap > max_gap {
            warn!(log, "skipping direction accuracy pair due to large time gap";
                "token" => &pair[0].token,
                "newer_target_time" => %pair[0].target_time,
                "older_target_time" => %pair[1].target_time,
                "gap_hours" => gap.num_hours(),
                "max_gap_hours" => max_gap.num_hours(),
            );
            continue;
        }
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
const MAX_PREDICTION_QUERY_LIMIT: i64 = 10_000;

/// 各トークンの平均 MAPE と方向正解率から複合 confidence を算出。
///
/// 戻り値: Result<BTreeMap<TokenOutAccount, f64>>
///   - Ok(map): 計算成功。エントリあり = confidence 計算済み、エントリなし = データ不足
///   - Err: DB アクセス失敗
pub(crate) async fn calculate_per_token_confidence(
    tokens: &[TokenOutAccount],
    cfg: &impl ConfigAccess,
) -> crate::Result<BTreeMap<TokenOutAccount, f64>> {
    let log = DEFAULT.new(o!("function" => "calculate_per_token_confidence"));
    let window = cfg.prediction_accuracy_window().max(1);
    let min_samples = cfg.prediction_accuracy_min_samples();
    let mape_excellent = cfg.prediction_mape_excellent();
    let mape_poor = cfg.prediction_mape_poor();

    // 1回の DB クエリで全トークンのレコードを取得
    // NOTE: tokens.len() は実用上 i64 範囲を超えない（メモリ制約）。
    // 万一変換に失敗した場合は MAX_PREDICTION_QUERY_LIMIT にフォールバックし、
    // 下記の warn ログで検知される。
    let token_count = i64::try_from(tokens.len()).unwrap_or(MAX_PREDICTION_QUERY_LIMIT);
    // NOTE: キャップ発生時は高頻度トークンがレコードを独占し、低頻度トークンの
    // confidence が min_samples 未満で計算不能（コールドスタート扱い）になりうる。
    // 現在の運用規模（window=30, tokens~10 → 300 << 10,000）では問題ないが、
    // トークン数が大幅に増加した場合は warn ログで検知すること。
    let raw_limit = window.saturating_mul(token_count);
    let limit = raw_limit.min(MAX_PREDICTION_QUERY_LIMIT);
    if raw_limit > MAX_PREDICTION_QUERY_LIMIT {
        warn!(log, "prediction query limit capped";
            "requested" => raw_limit, "capped" => MAX_PREDICTION_QUERY_LIMIT,
            "tokens" => tokens.len(), "window" => window);
    }
    let all_records = PredictionRecord::get_recent_evaluated_for_tokens(limit, tokens)
        .await
        .map_err(|e| {
            warn!(log, "failed to get prediction records"; "error" => %e);
            e
        })?;

    // Rust 側でトークンごとにグルーピング
    let mut by_token: BTreeMap<String, Vec<DbPredictionRecord>> = BTreeMap::new();
    for r in all_records {
        by_token.entry(r.token.clone()).or_default().push(r);
    }

    // 各トークン内を target_time DESC でソート（DB も target_time DESC だがグルーピング後に保証）
    for entries in by_token.values_mut() {
        entries.sort_by(|a, b| b.target_time.cmp(&a.target_time));
        entries.truncate(window as usize); // window は .max(1) 済みのため正値保証
    }

    let mut result = BTreeMap::new();

    for token in tokens {
        let token_str = token.to_string();
        let records = by_token.get(&token_str);
        let mape_values: Vec<f64> = records
            .into_iter()
            .flatten()
            .filter_map(|r| r.mape)
            .filter(|m| m.is_finite())
            .collect();

        if mape_values.len() < min_samples {
            continue;
        }

        // NOTE: min_samples >= 1（デフォルト 5）であるため mape_values は非空。
        // 仮に min_samples == 0 に設定された場合でもゼロ除算は NaN → mape_to_confidence
        // の NaN ガードで confidence = 0.0（安全側）になる。
        let avg_mape = mape_values.iter().sum::<f64>() / mape_values.len() as f64;

        let direction_data = records.map(|rs| calculate_direction_accuracy_for_records(rs, &log));
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

    Ok(result)
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
        start: target_time - chrono::TimeDelta::minutes(tolerance_minutes),
        end: target_time + chrono::TimeDelta::minutes(tolerance_minutes),
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
