use crate::Result;
use crate::config;
use crate::logging::*;
use crate::persistence::TimeRange;
use crate::persistence::prediction_record::{NewPredictionRecord, PredictionRecord};
use crate::persistence::token_rate::TokenRate;
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use bigdecimal::BigDecimal;
use chrono::{NaiveDateTime, Utc};
use num_traits::Zero;
use std::collections::BTreeMap;
use std::str::FromStr;
use zaciraci_common::types::TokenPrice;

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
/// 戻り値: Option<f64>
///   - Some(mape): 評価済み >= MIN_SAMPLES なら直近 N 件の平均 MAPE
///   - None: データ不足
pub async fn evaluate_pending_predictions() -> Result<Option<f64>> {
    let log = DEFAULT.new(o!("function" => "evaluate_pending_predictions"));

    let tolerance_minutes: i64 = config::get("PREDICTION_EVAL_TOLERANCE_MINUTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);

    let window: i64 = config::get("PREDICTION_ACCURACY_WINDOW")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);

    let min_samples: usize = config::get("PREDICTION_ACCURACY_MIN_SAMPLES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);

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
        let absolute_error = if diff < BigDecimal::from(0) {
            -diff.clone()
        } else {
            diff.clone()
        };
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

    // 直近 N 件の評価済みレコードから rolling MAPE を算出
    let recent = PredictionRecord::get_recent_evaluated(window).await?;

    if recent.len() < min_samples {
        debug!(log, "insufficient samples for rolling MAPE";
            "available" => recent.len(), "required" => min_samples);
        return Ok(None);
    }

    let mape_values: Vec<f64> = recent.iter().filter_map(|r| r.mape).collect();

    if mape_values.len() < min_samples {
        return Ok(None);
    }

    let rolling_mape: f64 = mape_values.iter().sum::<f64>() / mape_values.len() as f64;
    info!(log, "rolling MAPE calculated";
        "rolling_mape" => format!("{:.2}%", rolling_mape),
        "sample_count" => mape_values.len());

    Ok(Some(rolling_mape))
}

/// target_time に最も近い実績価格を token_rates から取得し TokenPrice に変換する。
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

    let closest = rates
        .into_iter()
        .min_by_key(|r| (r.timestamp - target_time).num_seconds().unsigned_abs());

    Ok(closest.map(|r| r.exchange_rate.to_price()))
}
