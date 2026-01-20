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
///
/// # Returns
/// 価格マップ（無次元比率: yoctoNEAR/smallest_unit = NEAR/token）
pub fn get_prices_at_time(
    price_data: &HashMap<String, Vec<ValueAtTime>>,
    target_time: DateTime<Utc>,
) -> Result<HashMap<String, TokenPriceF64>> {
    let mut prices: HashMap<String, TokenPriceF64> = HashMap::new();
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

        // 価格は無次元比率（yoctoNEAR/smallest_unit = NEAR/token）
        prices.insert(token.clone(), closest_value.value.to_f64());
    }

    Ok(prices)
}

/// Get prices at a specific time point, returning None if data is insufficient
///
/// # Returns
/// 価格マップ（無次元比率）
pub fn get_prices_at_time_optional(
    price_data: &HashMap<String, Vec<ValueAtTime>>,
    target_time: DateTime<Utc>,
) -> Option<HashMap<String, TokenPriceF64>> {
    get_prices_at_time(price_data, target_time).ok()
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

/// Validate data quality for simulation (placeholder implementation)
pub fn validate_data_quality(
    _price_data: &HashMap<String, Vec<ValueAtTime>>,
    _config: &SimulationConfig,
) -> Result<DataQuality> {
    Ok(DataQuality::High)
}

/// 統一されたログ出力関数
pub fn log_data_gap_event(event: &DataGapEvent) {
    let duration_str = if event.impact.duration_hours >= 24 {
        format!("{:.1} days", event.impact.duration_hours as f64 / 24.0)
    } else {
        format!("{} hours", event.impact.duration_hours)
    };

    println!(
        "⚠️  {} at {} - Duration: {} - Tokens: [{}]",
        match event.event_type {
            DataGapEventType::TradingSkipped => "Trading skipped",
            DataGapEventType::RebalanceSkipped => "Rebalance skipped",
            DataGapEventType::PriceDataMissing => "Price data missing",
        },
        event.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
        duration_str,
        event.affected_tokens.join(", ")
    );

    if event.impact.duration_hours >= 24 {
        println!("   📊 Long data gap detected - consider reviewing data quality");
    }
}

/// ギャップの影響を計算
pub fn calculate_gap_impact(
    last_success: Option<DateTime<Utc>>,
    current_time: DateTime<Utc>,
    price_data: &HashMap<String, Vec<ValueAtTime>>,
    tokens: &[String],
) -> DataGapImpact {
    let last_known_timestamp = last_success.unwrap_or(current_time);

    // 次の有効データを探す
    let next_known_timestamp = find_next_valid_data_time(price_data, current_time, tokens);

    let duration_hours = if let Some(next_time) = next_known_timestamp {
        (next_time - last_known_timestamp).num_hours()
    } else {
        (current_time - last_known_timestamp).num_hours()
    };

    DataGapImpact {
        duration_hours,
        last_known_timestamp,
        next_known_timestamp,
    }
}

/// 次の有効なデータ時刻を探す
fn find_next_valid_data_time(
    price_data: &HashMap<String, Vec<ValueAtTime>>,
    current_time: DateTime<Utc>,
    tokens: &[String],
) -> Option<DateTime<Utc>> {
    let mut earliest_next = None;

    for token in tokens {
        if let Some(values) = price_data.get(token) {
            // current_time以降で最初に有効なデータを探す
            for value in values {
                let value_time = DateTime::from_naive_utc_and_offset(value.time, Utc);
                if value_time > current_time {
                    match earliest_next {
                        None => earliest_next = Some(value_time),
                        Some(earliest) if value_time < earliest => earliest_next = Some(value_time),
                        _ => {}
                    }
                    break; // 昇順ソート済みと仮定
                }
            }
        }
    }

    earliest_next
}

/// ポートフォリオ評価用：より柔軟な価格取得（最大7日前まで遡る）
///
/// # Returns
/// 価格マップ（無次元比率）
pub fn get_last_known_prices_for_evaluation(
    price_data: &HashMap<String, Vec<ValueAtTime>>,
    target_time: DateTime<Utc>,
) -> Option<HashMap<String, TokenPriceF64>> {
    let mut prices: HashMap<String, TokenPriceF64> = HashMap::new();
    let max_lookback = chrono::Duration::days(7); // 最大7日前まで遡る

    for (token, values) in price_data {
        if let Some(price) = find_price_within(values, target_time, max_lookback) {
            prices.insert(token.clone(), price);
        }
    }

    if prices.is_empty() {
        None
    } else {
        Some(prices)
    }
}

/// 指定期間内で最も近い価格を探す（無次元比率）
fn find_price_within(
    values: &[ValueAtTime],
    target_time: DateTime<Utc>,
    max_lookback: chrono::Duration,
) -> Option<TokenPriceF64> {
    let earliest_allowed = target_time - max_lookback;

    values
        .iter()
        .filter(|v| {
            let value_time: DateTime<Utc> = DateTime::from_naive_utc_and_offset(v.time, Utc);
            value_time <= target_time && value_time >= earliest_allowed
        })
        .max_by_key(|v| v.time)
        .map(|v| TokenPriceF64::from_near_per_token(v.value.to_f64().as_f64()))
}
