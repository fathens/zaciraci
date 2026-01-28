use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use tokio::fs;

use crate::models::{
    history::HistoryFileData,
    prediction::{
        PredictionFileData, PredictionMetadata, PredictionPoint as CachePredictionPoint,
        PredictionResults,
    },
    token::TokenFileData,
};
use crate::utils::{
    cache::{PredictionCacheParams, get_price_history_dir, save_prediction_result},
    file::{ensure_directory_exists, file_exists, sanitize_filename, write_json_file},
};
use common::api::chronos::{ChronosPredictor, calculate_horizon};
use common::cache::CacheOutput;
use common::prediction::{PredictionPoint, TokenPredictionResult};
use common::types::TokenPrice;

/// Find the latest history file in the given directory
async fn find_latest_history_file(dir: &Path) -> Result<Option<PathBuf>> {
    if !dir.exists() {
        return Ok(None);
    }

    let mut entries = fs::read_dir(dir).await?;
    let mut latest_file: Option<(PathBuf, std::time::SystemTime)> = None;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_file()
            && path.extension().and_then(|s| s.to_str()) == Some("json")
            && let Some(name) = path.file_name().and_then(|s| s.to_str())
            && name.starts_with("history-")
        {
            let metadata = entry.metadata().await?;
            let modified = metadata.modified()?;

            if latest_file.is_none() || latest_file.as_ref().unwrap().1 < modified {
                latest_file = Some((path, modified));
            }
        }
    }

    Ok(latest_file.map(|(path, _)| path))
}

#[derive(Parser)]
#[clap(about = "Execute prediction and save results")]
pub struct KickArgs {
    #[clap(help = "Token file path (e.g., tokens/wrap.near.json)")]
    pub token_file: PathBuf,

    #[clap(short, long, default_value = "predictions", help = "Output directory")]
    pub output: PathBuf,

    #[clap(
        long,
        default_value = "0.0",
        help = "Start percentage of time range (0.0-100.0)"
    )]
    pub start_pct: f64,

    #[clap(
        long,
        default_value = "100.0",
        help = "End percentage of time range (0.0-100.0)"
    )]
    pub end_pct: f64,

    #[clap(
        long,
        default_value = "10.0",
        help = "Forecast duration as percentage of input data period (0.0-500.0)"
    )]
    pub forecast_ratio: f64,
}

pub async fn run(args: KickArgs) -> Result<()> {
    // Validate percentage range
    if args.start_pct < 0.0 || args.start_pct > 100.0 {
        return Err(anyhow::anyhow!(
            "Invalid start percentage: {:.1}% (must be 0.0-100.0)",
            args.start_pct
        ));
    }
    if args.end_pct < 0.0 || args.end_pct > 100.0 {
        return Err(anyhow::anyhow!(
            "Invalid end percentage: {:.1}% (must be 0.0-100.0)",
            args.end_pct
        ));
    }
    if args.start_pct >= args.end_pct {
        return Err(anyhow::anyhow!(
            "Start percentage ({:.1}%) must be less than end percentage ({:.1}%)",
            args.start_pct,
            args.end_pct
        ));
    }

    // Validate forecast ratio
    if args.forecast_ratio <= 0.0 || args.forecast_ratio > 500.0 {
        return Err(anyhow::anyhow!(
            "Invalid forecast ratio: {:.1}% (must be 0.0-500.0)",
            args.forecast_ratio
        ));
    }

    let predictor = ChronosPredictor::new();

    // Read token file
    if !file_exists(&args.token_file).await {
        return Err(anyhow::anyhow!(
            "Token file not found: {:?}",
            args.token_file
        ));
    }

    let content = fs::read_to_string(&args.token_file).await?;
    let token_data: TokenFileData = serde_json::from_str(&content)?;

    // Get or extract quote token
    let quote_token = extract_quote_token_from_path(&args.token_file)
        .or_else(|| token_data.metadata.quote_token.clone())
        .unwrap_or_else(|| "wrap.near".to_string());

    // Prepare output directory
    let base_dir = std::env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());

    // Show progress
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );

    // Get historical data for prediction
    pb.set_message("Loading historical token data...");

    // Try to load from price_history directory using cache utility
    let history_dir = get_price_history_dir(&quote_token, &token_data.token);
    let history_file = find_latest_history_file(&history_dir).await?;
    let (mut timestamps, mut values) = if let Some(file_path) = history_file {
        pb.set_message("Loading data from history file...");
        load_history_data(&file_path).await?
    } else {
        return Err(anyhow::anyhow!(
            "No history data found for token: {} in directory: {}. Please run 'cli_tokens history {}' first to fetch price data",
            token_data.token,
            history_dir.display(),
            args.token_file.display()
        ));
    };

    // Apply time-based percentage range filtering
    let total_len = timestamps.len();
    if args.start_pct != 0.0 || args.end_pct != 100.0 {
        let earliest_time = timestamps
            .iter()
            .min()
            .ok_or_else(|| anyhow::anyhow!("No timestamps found"))?;
        let latest_time = timestamps
            .iter()
            .max()
            .ok_or_else(|| anyhow::anyhow!("No timestamps found"))?;

        // Calculate the total duration
        let total_duration = latest_time.signed_duration_since(*earliest_time);

        // Calculate start and end times based on percentages
        let start_offset = Duration::milliseconds(
            (total_duration.num_milliseconds() as f64 * (args.start_pct / 100.0)) as i64,
        );
        let end_offset = Duration::milliseconds(
            (total_duration.num_milliseconds() as f64 * (args.end_pct / 100.0)) as i64,
        );

        let start_time = *earliest_time + start_offset;
        let end_time = *earliest_time + end_offset;

        // Filter data points based on time range
        let mut filtered_timestamps = Vec::new();
        let mut filtered_values = Vec::new();

        for (i, timestamp) in timestamps.iter().enumerate() {
            if *timestamp >= start_time && *timestamp <= end_time {
                filtered_timestamps.push(*timestamp);
                filtered_values.push(values[i].clone());
            }
        }

        timestamps = filtered_timestamps;
        values = filtered_values;

        pb.set_message(format!(
            "Using {:.1}%-{:.1}% time range ({} of {} data points, from {} to {})",
            args.start_pct,
            args.end_pct,
            timestamps.len(),
            total_len,
            start_time.format("%Y-%m-%d %H:%M"),
            end_time.format("%Y-%m-%d %H:%M")
        ));
    } else {
        pb.set_message(format!("Using all {} data points", total_len));
    }

    // Check if we have enough data after filtering
    if timestamps.is_empty() {
        return Err(anyhow::anyhow!(
            "No data points available after filtering {:.1}%-{:.1}% range",
            args.start_pct,
            args.end_pct
        ));
    }

    // Find the latest timestamp in the data and predict from there
    let latest_timestamp = timestamps
        .iter()
        .max()
        .ok_or_else(|| anyhow::anyhow!("No timestamps found"))?;

    // Calculate forecast duration based on actual data period and ratio
    let earliest_timestamp = timestamps
        .iter()
        .min()
        .ok_or_else(|| anyhow::anyhow!("No timestamps found"))?;
    let input_duration = latest_timestamp.signed_duration_since(*earliest_timestamp);
    let forecast_duration_ms =
        (input_duration.num_milliseconds() as f64 * (args.forecast_ratio / 100.0)) as i64;
    let forecast_until = *latest_timestamp + Duration::milliseconds(forecast_duration_ms);

    // Calculate horizon from timestamps interval
    let horizon = calculate_horizon(&timestamps, forecast_until);

    pb.set_message(format!(
        "Input period: {:.1} days, forecast ratio: {:.1}%, horizon: {} steps",
        input_duration.num_hours() as f64 / 24.0,
        args.forecast_ratio,
        horizon,
    ));

    // Convert TokenPrice values to BigDecimal for the predictor
    let values_bd: Vec<bigdecimal::BigDecimal> =
        values.iter().map(|v| v.as_bigdecimal().clone()).collect();

    // Execute prediction directly
    pb.set_message("Executing prediction...");
    let chronos_response = predictor
        .predict_price(timestamps.clone(), values_bd, horizon)
        .await?;

    // Convert ChronosPredictionResponse to Vec<PredictionPoint>
    let forecast: Vec<PredictionPoint> = chronos_response
        .forecast_timestamp
        .iter()
        .zip(chronos_response.forecast_values.iter())
        .enumerate()
        .map(|(i, (timestamp, value))| {
            let confidence_interval =
                chronos_response
                    .confidence_intervals
                    .as_ref()
                    .and_then(|intervals| {
                        let lower_key = intervals
                            .keys()
                            .find(|k| k.contains("lower") || k.contains("0.025"));
                        let upper_key = intervals
                            .keys()
                            .find(|k| k.contains("upper") || k.contains("0.975"));

                        if let (Some(lower_key), Some(upper_key)) = (lower_key, upper_key) {
                            let lower_values = intervals.get(lower_key)?;
                            let upper_values = intervals.get(upper_key)?;

                            if i < lower_values.len() && i < upper_values.len() {
                                Some(common::prediction::ConfidenceInterval {
                                    lower: TokenPrice::from_near_per_token(lower_values[i].clone()),
                                    upper: TokenPrice::from_near_per_token(upper_values[i].clone()),
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    });

            PredictionPoint {
                timestamp: *timestamp,
                value: TokenPrice::from_near_per_token(value.clone()),
                confidence_interval,
            }
        })
        .collect();

    // Determine history and prediction periods
    let hist_start = *earliest_timestamp;
    let hist_end = *latest_timestamp;
    let pred_start = forecast
        .first()
        .ok_or_else(|| anyhow::anyhow!("No forecast data available"))?
        .timestamp;
    let pred_end = forecast
        .last()
        .ok_or_else(|| anyhow::anyhow!("No forecast data available"))?
        .timestamp;

    // Convert to cache prediction points
    let cache_predictions: Vec<CachePredictionPoint> = forecast
        .iter()
        .map(|point| CachePredictionPoint {
            timestamp: point.timestamp,
            price: point.value.clone(),
            confidence: point.confidence_interval.as_ref().map(|ci| {
                let range = &ci.upper - &ci.lower;
                range / bigdecimal::BigDecimal::from(2) / &point.value
            }),
        })
        .collect();

    // Create structured prediction file data
    let prediction_file_data = PredictionFileData {
        metadata: PredictionMetadata {
            generated_at: Utc::now(),
            model_name: chronos_response.model_name.clone(),
            base_token: token_data.token.clone(),
            quote_token: quote_token.clone(),
            history_start: hist_start.format("%Y-%m-%d").to_string(),
            history_end: hist_end.format("%Y-%m-%d").to_string(),
            prediction_start: pred_start.format("%Y-%m-%d").to_string(),
            prediction_end: pred_end.format("%Y-%m-%d").to_string(),
        },
        prediction_results: PredictionResults {
            predictions: cache_predictions,
            model_metrics: chronos_response
                .metrics
                .as_ref()
                .map(|metrics| serde_json::to_value(metrics).unwrap_or(serde_json::Value::Null)),
        },
    };

    // Create cache parameters
    let cache_params = PredictionCacheParams {
        model_name: &chronos_response.model_name,
        quote_token: &quote_token,
        base_token: &token_data.token,
        hist_start,
        hist_end,
        pred_start,
        pred_end,
    };

    // Save results using structured cache
    pb.set_message("Saving prediction results to structured cache...");
    save_prediction_result(&cache_params, &prediction_file_data).await?;
    CacheOutput::prediction_cached(
        &token_data.token,
        prediction_file_data.prediction_results.predictions.len(),
    );

    // Also save the legacy format for backward compatibility
    let output_dir = PathBuf::from(&base_dir).join(&args.output).join("temp");
    ensure_directory_exists(&output_dir)?;
    let prediction_file = output_dir.join(format!("{}.json", sanitize_filename(&token_data.token)));

    let prediction_result = TokenPredictionResult {
        token: token_data.token.clone(),
        prediction_id: format!("local-{}", Utc::now().timestamp()),
        predicted_values: forecast,
        accuracy_metrics: None,
        chart_svg: None,
    };
    write_json_file(&prediction_file, &prediction_result).await?;

    pb.finish_with_message(format!(
        "Prediction completed for token: {} (model: {}, {} forecast points)",
        token_data.token,
        chronos_response.model_name,
        prediction_result.predicted_values.len()
    ));

    Ok(())
}

/// Extract quote_token from token file path (e.g., tokens/wrap.near/usdc.tether-token.near.json -> wrap.near)
fn extract_quote_token_from_path(token_file: &Path) -> Option<String> {
    token_file
        .parent()?
        .file_name()?
        .to_str()
        .map(|s| s.to_string())
        .filter(|s| s != "tokens") // Skip if direct under tokens/ directory
}

async fn load_history_data(
    history_file: &PathBuf,
) -> Result<(Vec<DateTime<Utc>>, Vec<TokenPrice>)> {
    let content = fs::read_to_string(history_file).await?;
    let history_data: HistoryFileData = serde_json::from_str(&content)?;

    if history_data.price_history.values.is_empty() {
        return Err(anyhow::anyhow!("No price data found in history file"));
    }

    let timestamps: Vec<DateTime<Utc>> = history_data
        .price_history
        .values
        .iter()
        .map(|v| DateTime::from_naive_utc_and_offset(v.time, Utc))
        .collect();
    let values: Vec<TokenPrice> = history_data
        .price_history
        .values
        .iter()
        .map(|v| v.value.clone())
        .collect();

    if timestamps.len() != values.len() {
        return Err(anyhow::anyhow!(
            "Timestamp and value arrays have different lengths"
        ));
    }

    Ok((timestamps, values))
}
