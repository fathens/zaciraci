use crate::prediction::ChronosPredictionResponse;
use bigdecimal::BigDecimal;
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;

/// Chronos 予測ライブラリのラッパー
pub struct ChronosPredictor {
    time_budget_secs: Option<f64>,
}

impl ChronosPredictor {
    pub fn new() -> Self {
        Self {
            time_budget_secs: None,
        }
    }

    pub fn with_time_budget(mut self, secs: f64) -> Self {
        self.time_budget_secs = Some(secs);
        self
    }

    /// 価格予測を実行
    ///
    /// `timestamps` と `values` は履歴データ、`horizon` は予測ステップ数。
    /// 内部で同期関数 `chronos_predictor::predict()` を `spawn_blocking` でラップして呼び出す。
    pub async fn predict_price(
        &self,
        timestamps: Vec<DateTime<Utc>>,
        values: Vec<BigDecimal>,
        horizon: usize,
    ) -> anyhow::Result<ChronosPredictionResponse> {
        let input = chronos_predictor::PredictionInput {
            timestamps: timestamps.iter().map(|t| t.naive_utc()).collect(),
            values: values.clone(),
            horizon,
            time_budget_secs: self.time_budget_secs,
        };

        let result = tokio::task::spawn_blocking(move || chronos_predictor::predict(&input))
            .await
            .map_err(|e| anyhow::anyhow!("spawn_blocking failed: {}", e))?
            .map_err(|e| anyhow::anyhow!("chronos_predictor::predict failed: {}", e))?;

        self.convert_result(result, &timestamps, horizon)
    }

    /// ForecastResult を ChronosPredictionResponse に変換
    fn convert_result(
        &self,
        result: chronos_predictor::ForecastResult,
        input_timestamps: &[DateTime<Utc>],
        horizon: usize,
    ) -> anyhow::Result<ChronosPredictionResponse> {
        // 入力タイムスタンプの間隔を推定
        let interval = estimate_interval(input_timestamps);

        // 最後のタイムスタンプから forecast_timestamp を生成
        let last_ts = input_timestamps
            .last()
            .ok_or_else(|| anyhow::anyhow!("Empty input timestamps"))?;

        let forecast_timestamps: Vec<DateTime<Utc>> = (1..=horizon)
            .map(|i| *last_ts + interval * i as i32)
            .collect();

        // confidence_intervals を構築
        let confidence_intervals = match (result.lower_bound, result.upper_bound) {
            (Some(lower), Some(upper)) => {
                let mut map = HashMap::new();
                map.insert("lower_10".to_string(), lower);
                map.insert("upper_90".to_string(), upper);
                Some(map)
            }
            _ => None,
        };

        Ok(ChronosPredictionResponse {
            forecast_timestamp: forecast_timestamps,
            forecast_values: result.forecast_values,
            model_name: result.model_name,
            confidence_intervals,
            metrics: None,
        })
    }
}

impl Default for ChronosPredictor {
    fn default() -> Self {
        Self::new()
    }
}

/// 入力タイムスタンプの平均間隔を推定
fn estimate_interval(timestamps: &[DateTime<Utc>]) -> Duration {
    if timestamps.len() < 2 {
        return Duration::hours(1); // デフォルト1時間
    }

    let total_duration = timestamps
        .last()
        .unwrap()
        .signed_duration_since(*timestamps.first().unwrap());
    let num_intervals = (timestamps.len() - 1) as i64;

    Duration::milliseconds(total_duration.num_milliseconds() / num_intervals)
}

/// forecast_until と入力データの間隔から horizon を計算するユーティリティ
pub fn calculate_horizon(timestamps: &[DateTime<Utc>], forecast_until: DateTime<Utc>) -> usize {
    if timestamps.is_empty() {
        return 1;
    }

    let interval = estimate_interval(timestamps);
    let last_ts = timestamps.last().unwrap();
    let remaining = forecast_until.signed_duration_since(*last_ts);

    if interval.num_milliseconds() <= 0 {
        return 1;
    }

    let horizon = remaining.num_milliseconds() / interval.num_milliseconds();
    std::cmp::max(1, horizon as usize)
}
