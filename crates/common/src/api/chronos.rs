use anyhow::Context;
use bigdecimal::{BigDecimal, FromPrimitive, ToPrimitive};
use chrono::{DateTime, TimeDelta, Utc};
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::prediction::ChronosPredictionResponse;

/// 線形トレンド成分。`predict_price_with_detrend` の pre/post 処理で使う。
///
/// `intercept`, `slope` は **時刻原点 `origin` (= 履歴最古点)** からの経過時間
/// (時間単位) を入力とする 1 次式 `value = intercept + slope × hours`。
/// `r_squared` は trend の決定係数で、閾値未満なら detrend は適用しない
/// (raw Chronos にフォールバック)。
struct LinearTrend {
    slope: f64,
    intercept: f64,
    r_squared: f64,
    origin: DateTime<Utc>,
}

impl LinearTrend {
    fn hours_since_origin(&self, ts: DateTime<Utc>) -> f64 {
        (ts - self.origin).num_seconds() as f64 / 3600.0
    }
}

/// 履歴 `data` (timestamp → price) に対する単純線形回帰。
///
/// f64 で計算する (BigDecimal の精度を捨てる) が、trend 検出は粗い概数で
/// 十分なので問題ない。trend extrapolation の数値精度は再合成 (`+`) で BigDecimal
/// に戻るため、最終 forecast の精度は detrend 適用前と同等。
fn compute_linear_trend(data: &BTreeMap<DateTime<Utc>, BigDecimal>) -> Option<LinearTrend> {
    let n = data.len() as f64;
    if n < 2.0 {
        return None;
    }
    let origin = *data.keys().next()?;
    let pairs: Vec<(f64, f64)> = data
        .iter()
        .filter_map(|(ts, price)| {
            let t_hours = (*ts - origin).num_seconds() as f64 / 3600.0;
            let y = price.to_f64()?;
            Some((t_hours, y))
        })
        .collect();
    if pairs.len() < 2 {
        return None;
    }
    let mean_t = pairs.iter().map(|(t, _)| *t).sum::<f64>() / pairs.len() as f64;
    let mean_y = pairs.iter().map(|(_, y)| *y).sum::<f64>() / pairs.len() as f64;
    let var_t: f64 = pairs.iter().map(|(t, _)| (t - mean_t).powi(2)).sum();
    if var_t <= 0.0 {
        return None;
    }
    let cov_ty: f64 = pairs.iter().map(|(t, y)| (t - mean_t) * (y - mean_y)).sum();
    let slope = cov_ty / var_t;
    let intercept = mean_y - slope * mean_t;
    let ss_tot: f64 = pairs.iter().map(|(_, y)| (y - mean_y).powi(2)).sum();
    let ss_res: f64 = pairs
        .iter()
        .map(|(t, y)| (y - (intercept + slope * t)).powi(2))
        .sum();
    let r_squared = if ss_tot > 0.0 {
        (1.0 - ss_res / ss_tot).clamp(0.0, 1.0)
    } else {
        0.0
    };
    Some(LinearTrend {
        slope,
        intercept,
        r_squared,
        origin,
    })
}

/// Chronos 予測ライブラリのラッパー
///
/// 専用 rayon ThreadPool を持つ `predictor::Predictor` を内部で保持し、
/// モデル訓練の並列度を制御する。`tokio::spawn_blocking` スレッドは
/// rayon のワークスティーリングに参加しないため、同時モデル訓練数は
/// `max_model_threads` で制御される（prediction concurrency に無関係）。
pub struct ChronosPredictor {
    predictor: Arc<predictor::Predictor>,
}

impl ChronosPredictor {
    pub fn new(max_model_threads: usize) -> anyhow::Result<Self> {
        Ok(Self {
            predictor: Arc::new(
                predictor::Predictor::new(max_model_threads)
                    .context("Failed to create Predictor")?,
            ),
        })
    }

    /// 価格予測を実行
    ///
    /// `data` は履歴データ（タイムスタンプ → 価格）、`forecast_until` は予測終了時刻。
    /// 内部で同期関数 `predictor.predict()` を `spawn_blocking` でラップして呼び出す。
    /// モデル訓練は専用 ThreadPool 上で実行され、spawn_blocking スレッドは
    /// rayon のワークスティーリングに参加しない。
    pub async fn predict_price(
        &self,
        data: BTreeMap<DateTime<Utc>, BigDecimal>,
        forecast_until: DateTime<Utc>,
    ) -> anyhow::Result<ChronosPredictionResponse> {
        // 最後のタイムスタンプから horizon を計算
        let last_ts = data
            .keys()
            .last()
            .ok_or_else(|| anyhow::anyhow!("Empty data"))?;
        let horizon_duration = forecast_until.signed_duration_since(*last_ts);

        // DateTime<Utc> → NaiveDateTime に変換（chronos-rs の要求）
        let naive_data: BTreeMap<_, _> = data
            .into_iter()
            .map(|(ts, val)| (ts.naive_utc(), val))
            .collect();

        let input = predictor::PredictionInput {
            data: naive_data,
            horizon: TimeDelta::try_milliseconds(horizon_duration.num_milliseconds())
                .unwrap_or_else(|| TimeDelta::hours(1)),
        };

        let predictor = self.predictor.clone();
        let result = tokio::task::spawn_blocking(move || predictor.predict(&input))
            .await
            .map_err(|e| anyhow::anyhow!("spawn_blocking failed: {}", e))?
            .map_err(|e| anyhow::anyhow!("predictor::predict failed: {}", e))?;

        self.convert_result(result)
    }

    /// Detrend pre/post 処理付きで予測する。
    ///
    /// アルゴリズム:
    /// 1. 履歴 (t, price) に線形回帰して `slope`, `intercept`, `r²` を計算
    /// 2. R² が `R_SQUARED_THRESHOLD` (= 0.05) 未満、またはサンプル数 < `MIN_SAMPLES_FOR_DETREND`
    ///    なら detrend をスキップして通常の `predict_price` を呼ぶ
    /// 3. 履歴を detrend: `detrended[t] = price[t] - (intercept + slope × t)`
    /// 4. Chronos に detrended を渡して forecast
    /// 5. forecast に trend を戻す: `final[t] = chronos_pred[t] + (intercept + slope × t)`
    ///
    /// この wrapper の目的は Chronos の mean-reversion bias を補正すること。
    /// trending token (zec 等 +30%/19日) は raw Chronos では「mean に戻る」と
    /// 予測されて systematically 逆方向になるが、trend を分離して残差だけを
    /// Chronos に渡すことで forecast に trend が保存される。
    ///
    /// 詳細な動機・観測データは `../zcrc-chronos-rs/improve.md` を参照。
    pub async fn predict_price_with_detrend(
        &self,
        data: BTreeMap<DateTime<Utc>, BigDecimal>,
        forecast_until: DateTime<Utc>,
    ) -> anyhow::Result<ChronosPredictionResponse> {
        /// 最小サンプル数 — これ未満は線形回帰の信頼性が低いので detrend しない。
        const MIN_SAMPLES_FOR_DETREND: usize = 10;
        /// R² 閾値 — これ未満はトレンドが信頼できないので detrend しない。
        const R_SQUARED_THRESHOLD: f64 = 0.05;

        if data.len() < MIN_SAMPLES_FOR_DETREND {
            return self.predict_price(data, forecast_until).await;
        }

        let Some(trend) = compute_linear_trend(&data) else {
            return self.predict_price(data, forecast_until).await;
        };
        if trend.r_squared < R_SQUARED_THRESHOLD {
            return self.predict_price(data, forecast_until).await;
        }

        // Detrend: subtract `intercept + slope × t_hours_since_epoch_min` from each price.
        let detrended: BTreeMap<DateTime<Utc>, BigDecimal> = data
            .iter()
            .filter_map(|(ts, price)| {
                let dt_hours = trend.hours_since_origin(*ts);
                let trend_value = trend.intercept + trend.slope * dt_hours;
                let trend_bd = BigDecimal::from_f64(trend_value)?;
                Some((*ts, price - trend_bd))
            })
            .collect();
        if detrended.len() < MIN_SAMPLES_FOR_DETREND {
            return self.predict_price(data, forecast_until).await;
        }

        let mut response = self.predict_price(detrended, forecast_until).await?;

        // Re-trend: add `intercept + slope × t_hours_since_origin` back to each forecast value.
        response.forecast = response
            .forecast
            .into_iter()
            .filter_map(|(ts, val)| {
                let dt_hours = trend.hours_since_origin(ts);
                let trend_value = trend.intercept + trend.slope * dt_hours;
                let trend_bd = BigDecimal::from_f64(trend_value)?;
                Some((ts, val + trend_bd))
            })
            .collect();
        // 信頼区間も同じ trend を足し戻す (区間幅は変えない、中心が trend に乗る形)。
        if let Some(lower) = response.lower_bound.take() {
            response.lower_bound = Some(
                lower
                    .into_iter()
                    .filter_map(|(ts, val)| {
                        let dt_hours = trend.hours_since_origin(ts);
                        let trend_value = trend.intercept + trend.slope * dt_hours;
                        let trend_bd = BigDecimal::from_f64(trend_value)?;
                        Some((ts, val + trend_bd))
                    })
                    .collect(),
            );
        }
        if let Some(upper) = response.upper_bound.take() {
            response.upper_bound = Some(
                upper
                    .into_iter()
                    .filter_map(|(ts, val)| {
                        let dt_hours = trend.hours_since_origin(ts);
                        let trend_value = trend.intercept + trend.slope * dt_hours;
                        let trend_bd = BigDecimal::from_f64(trend_value)?;
                        Some((ts, val + trend_bd))
                    })
                    .collect(),
            );
        }

        Ok(response)
    }

    /// ForecastResult を ChronosPredictionResponse に変換
    fn convert_result(
        &self,
        result: predictor::ForecastResult,
    ) -> anyhow::Result<ChronosPredictionResponse> {
        // NaiveDateTime → DateTime<Utc> に変換
        let forecast = result
            .forecast_values
            .into_iter()
            .map(|(ts, val)| (DateTime::from_naive_utc_and_offset(ts, Utc), val))
            .collect();

        let lower_bound = result.lower_bound.map(|bound| {
            bound
                .into_iter()
                .map(|(ts, val)| (DateTime::from_naive_utc_and_offset(ts, Utc), val))
                .collect()
        });

        let upper_bound = result.upper_bound.map(|bound| {
            bound
                .into_iter()
                .map(|(ts, val)| (DateTime::from_naive_utc_and_offset(ts, Utc), val))
                .collect()
        });

        Ok(ChronosPredictionResponse {
            forecast,
            lower_bound,
            upper_bound,
            model_name: result.model_name,
            strategy_name: result.strategy_name,
            processing_time_secs: result.processing_time_secs,
            model_count: result.model_count,
        })
    }
}
