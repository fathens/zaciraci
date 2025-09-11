use super::types::*;
use crate::api::backend::BackendClient;
use crate::utils::cache::fetch_multiple_price_history_with_cache;
use anyhow::Result;
use chrono::{DateTime, Utc};
use common::stats::ValueAtTime;
use std::collections::HashMap;

/// Fetch price data from backend with cache support
pub async fn fetch_price_data(
    backend_client: &BackendClient,
    config: &SimulationConfig,
) -> Result<HashMap<String, Vec<ValueAtTime>>> {
    // 必要なデータ期間を計算
    let data_start_date = config.start_date - chrono::Duration::days(config.historical_days);
    let data_end_date = config.end_date + config.prediction_horizon;

    // Use cache-enabled fetch function
    fetch_multiple_price_history_with_cache(
        backend_client,
        &config.quote_token,
        &config.target_tokens,
        data_start_date,
        data_end_date,
    )
    .await
}

/// Get prices at a specific time point
pub fn get_prices_at_time(
    price_data: &HashMap<String, Vec<ValueAtTime>>,
    target_time: DateTime<Utc>,
) -> Result<HashMap<String, f64>> {
    let mut prices = HashMap::new();
    let one_hour = chrono::Duration::hours(1);
    let time_window_start = target_time - one_hour;
    let time_window_end = target_time + one_hour;

    for (token, values) in price_data {
        // target_time の前後1時間以内のデータを検索
        let nearby_values: Vec<&ValueAtTime> = values
            .iter()
            .filter(|v| {
                let value_time: DateTime<Utc> = DateTime::from_naive_utc_and_offset(v.time, Utc);
                value_time >= time_window_start && value_time <= time_window_end
            })
            .collect();

        if nearby_values.is_empty() {
            return Err(anyhow::anyhow!(
                "No price data found for token '{}' within 1 hour of target time {}. \
                 This indicates insufficient data quality for reliable simulation. \
                 Please ensure continuous price data is available for the simulation period.",
                token,
                target_time.format("%Y-%m-%d %H:%M:%S UTC")
            ));
        }

        // 最も近い価格データを選択
        let closest_value = nearby_values
            .iter()
            .min_by_key(|v| {
                let value_time: DateTime<Utc> = DateTime::from_naive_utc_and_offset(v.time, Utc);
                (value_time - target_time).num_seconds().abs()
            })
            .unwrap();

        prices.insert(token.clone(), closest_value.value);
    }

    Ok(prices)
}

/// Calculate returns from a series of values
pub fn calculate_returns(values: &[f64]) -> Vec<f64> {
    values
        .windows(2)
        .map(|window| {
            let prev = window[0];
            let curr = window[1];
            if prev != 0.0 {
                (curr - prev) / prev
            } else {
                0.0
            }
        })
        .collect()
}

/// Calculate volatility from returns
pub fn calculate_volatility(returns: &[f64]) -> f64 {
    if returns.len() <= 1 {
        return 0.0;
    }

    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance =
        returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (returns.len() - 1) as f64;

    variance.sqrt()
}

/// Extract price values from ValueAtTime series
pub fn extract_prices(data: &[ValueAtTime]) -> Vec<f64> {
    data.iter().map(|v| v.value).collect()
}

/// Validate data quality for simulation (placeholder implementation)
pub fn validate_data_quality(
    _price_data: &HashMap<String, Vec<ValueAtTime>>,
    _config: &SimulationConfig,
) -> Result<DataQuality> {
    Ok(DataQuality::High)
}
