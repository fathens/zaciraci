use crate::prediction::ChronosPredictionResponse;
use bigdecimal::BigDecimal;
use chrono::{DateTime, TimeDelta, Utc};
use std::collections::BTreeMap;

/// Chronos 予測ライブラリのラッパー
pub struct ChronosPredictor;

impl ChronosPredictor {
    pub fn new() -> Self {
        Self
    }

    /// 価格予測を実行
    ///
    /// `timestamps` と `values` は履歴データ、`forecast_until` は予測終了時刻。
    /// 内部で同期関数 `predictor::predict()` を `spawn_blocking` でラップして呼び出す。
    pub async fn predict_price(
        &self,
        timestamps: Vec<DateTime<Utc>>,
        values: Vec<BigDecimal>,
        forecast_until: DateTime<Utc>,
    ) -> anyhow::Result<ChronosPredictionResponse> {
        // timestamps と values から BTreeMap を構築
        let data: BTreeMap<_, _> = timestamps
            .iter()
            .zip(values.iter())
            .map(|(ts, val)| (ts.naive_utc(), val.clone()))
            .collect();

        // 最後のタイムスタンプから horizon を計算
        let last_ts = timestamps
            .last()
            .ok_or_else(|| anyhow::anyhow!("Empty timestamps"))?;
        let horizon_duration = forecast_until.signed_duration_since(*last_ts);

        let input = predictor::PredictionInput {
            data,
            horizon: TimeDelta::try_milliseconds(horizon_duration.num_milliseconds())
                .unwrap_or_else(|| TimeDelta::hours(1)),
        };

        let result = tokio::task::spawn_blocking(move || predictor::predict(&input))
            .await
            .map_err(|e| anyhow::anyhow!("spawn_blocking failed: {}", e))?
            .map_err(|e| anyhow::anyhow!("predictor::predict failed: {}", e))?;

        self.convert_result(result)
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

impl Default for ChronosPredictor {
    fn default() -> Self {
        Self::new()
    }
}
