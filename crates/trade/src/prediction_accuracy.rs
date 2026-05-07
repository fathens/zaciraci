use crate::Result;
use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use common::config::ConfigAccess;
use common::types::TimeRange;
use common::types::TokenPrice;
use common::types::{TokenAccount, TokenInAccount, TokenOutAccount};
use logging::*;
use num_traits::{FromPrimitive, ToPrimitive, Zero};
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

/// `build_prediction_records` で許容する skip 比率の上限。
///
/// 1 cycle 内で `try_new` が `Err` で skip された予測の比率がこの閾値を超えた場合、
/// caller は systematic な data leakage / 環境異常 (NTP step backward 等) と判断
/// して **当該 cycle 全体を abort** する。Markowitz 最適化が成立する最小トークン数
/// (~5 銘柄) のうち 50% 以上が脱落 = portfolio 機能停止と等価のため、fail-loud で
/// alert を発火させる。
///
/// **config 化禁止**: `CONFIG_STORE` / `DB_STORE` 経由で `0.0` 等を流し込まれると
/// systematic violation 検知が無効化される DoS 経路になるため、named const で
/// hard-code する。
const SYSTEMATIC_VIOLATION_THRESHOLD: f64 = 0.5;

/// BTreeMap から NewPredictionRecord の Vec を生成する（DB 非依存）。
///
/// `try_new` の `Err` (data leakage 不変条件違反) は当該 token を skip し、
/// `error!` ログで alert 発火可能化する (Layer 1 fail-soft 防御)。
/// 戻り値は `(records, skipped_count)`。
fn build_prediction_records(
    predictions: &BTreeMap<TokenOutAccount, (TokenPrice, NaiveDateTime)>,
    quote_token: &TokenInAccount,
    created_at: NaiveDateTime,
) -> (Vec<NewPredictionRecord>, usize) {
    let log = DEFAULT.new(o!("function" => "build_prediction_records"));
    let mut records = Vec::with_capacity(predictions.len());
    let mut skipped = 0usize;
    for (token, (price, data_cutoff_time)) in predictions.iter() {
        let target_time =
            *data_cutoff_time + chrono::TimeDelta::hours(PREDICTION_HORIZON_HOURS as i64);
        match NewPredictionRecord::try_new(
            token.to_string(),
            quote_token.to_string(),
            price.as_bigdecimal().clone(),
            *data_cutoff_time,
            target_time,
            created_at,
        ) {
            Ok(record) => records.push(record),
            Err(e) => {
                // data leakage 経路を fail-loud に通知 (info!/warn! ではなく error!)。
                // skip された予測は当該 token サイクルを欠落させ optimizer の weights を
                // 変動させるため、alert 監視レベルで記録する必要がある。
                error!(log, "skipping prediction record due to invariant violation";
                    "token" => %token, "error" => %e);
                skipped += 1;
            }
        }
    }
    (records, skipped)
}

/// 予測結果を prediction_records テーブルに記録する。
///
/// `created_at` は呼び出し側の「現在時刻」を明示的に渡す。production では
/// `Utc::now()` 相当、シミュレーションでは sim_day を渡すことで、engine の
/// fresh-prediction 判定が両経路で同じセマンティクスを持つ。
///
/// `NewPredictionRecord::try_new` が `Err` を返した token は skip するが、
/// skip 比率が [`SYSTEMATIC_VIOLATION_THRESHOLD`] を超えた場合は systematic
/// violation と判断して `Err` で当該 cycle を abort する (Markowitz 最適化が
/// 縮退する閾値)。
///
/// DB 操作: INSERT INTO prediction_records (skip 後のトークン数分)
pub(crate) async fn record_predictions(
    predictions: &BTreeMap<TokenOutAccount, (TokenPrice, NaiveDateTime)>,
    quote_token: &TokenInAccount,
    created_at: NaiveDateTime,
) -> Result<()> {
    let log = DEFAULT.new(o!("function" => "record_predictions"));

    let total = predictions.len();
    let (records, skipped) = build_prediction_records(predictions, quote_token, created_at);

    if total > 0 {
        let skip_ratio = skipped as f64 / total as f64;
        if skip_ratio > SYSTEMATIC_VIOLATION_THRESHOLD {
            error!(log, "systematic prediction invariant violation; aborting cycle";
                "skipped" => skipped, "total" => total, "ratio" => skip_ratio,
                "threshold" => SYSTEMATIC_VIOLATION_THRESHOLD);
            return Err(anyhow::anyhow!(
                "data quality breakdown: {} of {} predictions skipped (ratio {:.2} > {:.2})",
                skipped,
                total,
                skip_ratio,
                SYSTEMATIC_VIOLATION_THRESHOLD
            ));
        }
        if skipped > 0 {
            warn!(log, "some predictions skipped due to invariant violation";
                "skipped" => skipped, "total" => total);
        }
    }

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

    if as_of > chrono::Utc::now() {
        debug!(log, "as_of is in the future; predictions without actual data will be skipped";
            "as_of" => %as_of);
    }

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
            .array_windows::<2>()
            .all(|[newer, older]| newer.target_time >= older.target_time),
        "records must be sorted by target_time DESC"
    );
    let max_gap = chrono::TimeDelta::hours((PREDICTION_HORIZON_HOURS as i64 * 3) / 2);
    let mut correct = 0usize;
    let mut total = 0usize;
    for [newer, older] in records.array_windows::<2>() {
        let gap = newer.target_time - older.target_time;
        if gap > max_gap {
            warn!(log, "skipping direction accuracy pair due to large time gap";
                "token" => &newer.token,
                "newer_target_time" => %newer.target_time,
                "older_target_time" => %older.target_time,
                "gap_hours" => gap.num_hours(),
                "max_gap_hours" => max_gap.num_hours(),
            );
            continue;
        }
        let (Some(actual), Some(prev_actual)) = (&newer.actual_price, &older.actual_price) else {
            continue;
        };
        if is_direction_correct(prev_actual, &newer.predicted_price, actual) {
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

/// 指定トークン群の最近評価済み prediction_records を取得し、トークンごとに
/// グルーピングして返す（`target_time DESC` ソート + `window` 件にトリム済み）。
///
/// `window` は呼び出し側で `cfg.prediction_accuracy_window().max(1)` などにより
/// 1 以上にクランプ済みである前提（`debug_assert!` で検証）。
///
/// `BTreeMap` のキーが `String` なのは、`DbPredictionRecord::token` が DB 由来の
/// `String` のままであり、ここで `TokenOutAccount` へ再パースするとパース失敗の
/// エラーパス（DB データ汚損時のサイレント脱落）が新たに生じるため。プライベート
/// ヘルパで型は外部に漏れず、内部的な lookup も `token.to_string()` で完結する。
async fn fetch_records_grouped_by_token(
    tokens: &[TokenOutAccount],
    window: i64,
    log: &slog::Logger,
) -> Result<BTreeMap<String, Vec<DbPredictionRecord>>> {
    debug_assert!(window >= 1, "window must be clamped to >= 1");

    let token_count = i64::try_from(tokens.len()).unwrap_or(MAX_PREDICTION_QUERY_LIMIT);
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

    let mut by_token: BTreeMap<String, Vec<DbPredictionRecord>> = BTreeMap::new();
    for r in all_records {
        by_token.entry(r.token.clone()).or_default().push(r);
    }
    for entries in by_token.values_mut() {
        entries.sort_by(|a, b| b.target_time.cmp(&a.target_time));
        entries.truncate(window as usize);
    }

    Ok(by_token)
}

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

        // NOTE: F004 の `clamp_min_samples` で `min_samples >= 1` が保証され、
        // 直前の `mape_values.len() < min_samples` ガードを通過した時点で
        // `mape_values` は非空。万一 0 に設定されてもクランプで 1 に丸められる。
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

/// 各トークンの過去予測の系統的バイアス中央値を計算する。
///
/// 各レコードの `(predicted - actual) / actual` を集計し、トークンごとに中央値を返す。
/// 中央値は外れ値に頑健で、強気/弱気バイアスの推定に適する。
///
/// 戻り値:
///   - エントリあり: bias > 0 が過大予測、bias < 0 が過小予測
///   - エントリなし: `min_samples` 未満で計算不能（コールドスタート扱い）
///   - Err: DB アクセス失敗
pub(crate) async fn calculate_per_token_bias(
    tokens: &[TokenOutAccount],
    cfg: &impl ConfigAccess,
) -> crate::Result<BTreeMap<TokenOutAccount, f64>> {
    let log = DEFAULT.new(o!("function" => "calculate_per_token_bias"));
    let window = cfg.prediction_accuracy_window().max(1);
    let min_samples = cfg.prediction_accuracy_min_samples();

    let by_token = fetch_records_grouped_by_token(tokens, window, &log).await?;

    let mut result = BTreeMap::new();
    for token in tokens {
        let token_str = token.to_string();
        let Some(records) = by_token.get(&token_str) else {
            continue;
        };

        let mut bias_values: Vec<f64> = records
            .iter()
            .filter_map(|r| {
                let actual = r.actual_price.as_ref()?;
                if actual.is_zero() {
                    return None;
                }
                let actual_f = actual.to_f64()?;
                let predicted_f = r.predicted_price.to_f64()?;
                let bias = (predicted_f - actual_f) / actual_f;
                if bias.is_finite() { Some(bias) } else { None }
            })
            .collect();

        if bias_values.len() < min_samples {
            continue;
        }

        // The filter_map above keeps only `is_finite` values, so NaN cannot
        // appear here; `total_cmp` provides a zero-cost total order anyway.
        bias_values.sort_by(|a, b| a.total_cmp(b));
        let Some(median) = compute_median(&bias_values) else {
            // Logic-bug indicator: bias_values is non-empty (length passed
            // the `< min_samples` gate, and `min_samples >= 1` after the
            // typed-config clamp) so `compute_median` should always return
            // `Some`. Skip the token instead of panicking; warn so the
            // unexpected path is observable in production logs.
            warn!(log, "compute_median returned None despite passing min_samples gate, excluding token";
                "token" => %token_str,
                "samples" => bias_values.len(),
                "min_samples" => min_samples
            );
            continue;
        };

        debug!(log, "token prediction bias";
            "token" => %token_str,
            "samples" => bias_values.len(),
            "bias" => format!("{:.4}", median)
        );
        result.insert(token.clone(), median);
    }

    Ok(result)
}

/// ソート済みスライスの中央値を計算する。
///
/// 偶数長は中央 2 要素の平均、奇数長は中央要素を返す。
/// 空入力には `None` を返す（F004: defense-in-depth — `min_samples` クランプで
/// 通常はここに到達しないが、想定外経路で空 slice が渡されても release で
/// panic させない安全弁として）。
fn compute_median(sorted: &[f64]) -> Option<f64> {
    let n = sorted.len();
    if n == 0 {
        return None;
    }
    Some(if n.is_multiple_of(2) {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    } else {
        sorted[n / 2]
    })
}

/// 各トークンの **mean squared relative error (MSRE)** を計算する。
///
/// 各レコードの `(mape / 100.0)²` を集計し、トークンごとに **平均**を返す。
/// 名前に "variance" を含むが、これは統計的なサンプル分散 `Var()` ではなく
/// `mean of (mape / 100)²` = MSRE である。共分散行列の対角インフレ用に
/// **スケール一致 proxy** として利用する（return² スケールで対角と単位整合）。
///
/// 注意: サンプル分散 `Var()` を使うと return⁴ になり共分散の対角と単位不整合
/// となるため、平均（MSRE）を採用している。
///
/// API 利用上の注意: 戻り値および `PredErrDiagonal::variances` フィールドは
/// 名称こそ "variance" だが、中身は MSRE である。サンプル分散として扱わないこと。
/// （関数名・フィールド名の rename と `MeanSquaredError` newtype 化は F009 Phase 2
/// で別 PR にて対応予定。）
///
/// 戻り値:
///   - エントリあり: MSRE > 0（return² スケール）
///   - エントリなし: `min_samples` 未満で計算不能
///   - Err: DB アクセス失敗
pub(crate) async fn calculate_per_token_pred_err_variance(
    tokens: &[TokenOutAccount],
    cfg: &impl ConfigAccess,
) -> crate::Result<BTreeMap<TokenOutAccount, f64>> {
    let log = DEFAULT.new(o!("function" => "calculate_per_token_pred_err_variance"));
    let window = cfg.prediction_accuracy_window().max(1);
    let min_samples = cfg.prediction_accuracy_min_samples();

    let by_token = fetch_records_grouped_by_token(tokens, window, &log).await?;

    let mut result = BTreeMap::new();
    for token in tokens {
        let token_str = token.to_string();
        let Some(records) = by_token.get(&token_str) else {
            continue;
        };

        let squared: Vec<f64> = records
            .iter()
            .filter_map(|r| {
                let mape = r.mape?;
                if !mape.is_finite() {
                    return None;
                }
                let ratio = mape / 100.0;
                Some(ratio * ratio)
            })
            .collect();

        if squared.len() < min_samples {
            continue;
        }

        let mean = squared.iter().sum::<f64>() / squared.len() as f64;
        debug!(log, "token prediction error variance";
            "token" => %token_str,
            "samples" => squared.len(),
            "variance" => format!("{:.6}", mean)
        );
        result.insert(token.clone(), mean);
    }

    Ok(result)
}

/// バイアス補正の安全クランプ範囲（下限）。
///
/// `±50%` を超える bias はモデル推定誤差ではなく入力データ異常
/// （価格急変、欠損、外れ値混入など）と判断し、安全側に丸める。
/// 50% を超える補正は (1 + bias) が 0 や負値に近づき
/// `correct_prediction` の数式が破綻するため、構造的に防止する目的も兼ねる。
const BIAS_CLAMP_LOWER: f64 = -0.5;

/// バイアス補正の安全クランプ範囲（上限）。詳細は [`BIAS_CLAMP_LOWER`] を参照。
const BIAS_CLAMP_UPPER: f64 = 0.5;

/// バイアス中央値を用いて予測価格を補正する（3 層 defense-in-depth）。
///
/// - L1（入力ガード）: bias を `[BIAS_CLAMP_LOWER, BIAS_CLAMP_UPPER]` にクランプし、モデル破綻時の暴走を防ぐ
/// - L2（数式安全）: `corrected = predicted / (1 + bias_clamped)` で正値保証 + ゼロ除算回避
/// - L3（型ガード）: 数学的に破綻するケース（factor <= 0、結果がゼロ）は `None` を返し、
///   呼び出し側でトークン除外して Sell trigger 発火を構造的に防ぐ
///
/// 補正方向: bias > 0（過大予測）→ 価格下方修正、bias < 0（過小予測）→ 価格上方修正
pub(crate) fn correct_prediction(predicted: &TokenPrice, bias: f64) -> Option<TokenPrice> {
    if !bias.is_finite() {
        return None;
    }
    let bias_clamped = bias.clamp(BIAS_CLAMP_LOWER, BIAS_CLAMP_UPPER);
    let factor = 1.0 + bias_clamped;
    if factor <= 0.0 {
        return None;
    }
    let factor_bd = BigDecimal::from_f64(factor)?;
    if factor_bd.is_zero() {
        return None;
    }
    let corrected_bd = predicted.as_bigdecimal() / &factor_bd;
    // TokenPrice の不変条件（非負）を信頼: factor > 0 かつ predicted >= 0 → corrected >= 0
    // ゼロのみ除外で十分
    if corrected_bd.is_zero() {
        return None;
    }
    Some(TokenPrice::from_near_per_token(corrected_bd))
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
