use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use tokio::fs;

use crate::api::chronos::ChronosApiClient;
use crate::models::{
    history::HistoryFileData,
    prediction::{PredictionPoint, TokenPredictionResult, ZeroShotPredictionRequest},
    token::TokenFileData,
};
use crate::utils::{
    config::Config,
    file::{ensure_directory_exists, file_exists, sanitize_filename, write_json_file},
};

#[derive(Parser)]
#[clap(about = "Execute zeroshot prediction for specified token file")]
pub struct PredictArgs {
    #[clap(help = "Token file path (e.g., tokens/wrap.near.json)")]
    pub token_file: PathBuf,

    #[clap(short, long, default_value = "predictions", help = "Output directory")]
    pub output: PathBuf,

    #[clap(
        short,
        long,
        default_value = "server_default",
        help = "Prediction model"
    )]
    pub model: String,

    #[clap(long, help = "Force overwrite existing prediction results")]
    pub force: bool,

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

pub async fn run(args: PredictArgs) -> Result<()> {
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

    let config = Config::from_env();
    let chronos_client = ChronosApiClient::new(config.chronos_url);

    // Read token file
    if !file_exists(&args.token_file).await {
        return Err(anyhow::anyhow!(
            "Token file not found: {:?}",
            args.token_file
        ));
    }

    let file_content = fs::read_to_string(&args.token_file).await?;
    let token_data: TokenFileData = serde_json::from_str(&file_content)?;

    println!(
        "Processing prediction for token: {}",
        token_data.token_data.token
    );

    // Get base directory from environment variable
    let base_dir = std::env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
    let output_dir = PathBuf::from(&base_dir).join(&args.output);

    // Ensure output directory exists
    ensure_directory_exists(&output_dir)?;

    // Extract quote_token from token file path or use default
    let quote_token =
        extract_quote_token_from_path(&args.token_file).unwrap_or("wrap.near".to_string());

    // Create quote_token subdirectory
    let quote_dir = output_dir.join(sanitize_filename(&quote_token));
    ensure_directory_exists(&quote_dir)?;

    // Create prediction file path (${quote_token}/${base_token}.json)
    let filename = format!("{}.json", sanitize_filename(&token_data.token_data.token));
    let prediction_file = quote_dir.join(filename);

    // Check if prediction already exists
    if !args.force && file_exists(&prediction_file).await {
        return Err(anyhow::anyhow!(
            "Prediction already exists: {:?}. Use --force to overwrite",
            prediction_file
        ));
    }

    // Show progress
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );

    // Get historical data for prediction
    pb.set_message("Loading historical token data...");

    // Try to load from history file first (${quote_token}/${base_token}.json)
    let base_dir = std::env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
    let history_file = PathBuf::from(base_dir)
        .join("history")
        .join(sanitize_filename(&quote_token))
        .join(format!(
            "{}.json",
            sanitize_filename(&token_data.token_data.token)
        ));
    let (mut timestamps, mut values) = if history_file.exists() {
        pb.set_message("Loading data from history file...");
        load_history_data(&history_file).await?
    } else {
        // Fallback: return error instead of generating mock data
        return Err(anyhow::anyhow!(
            "No history data found for token: {}. Please run 'cli_tokens history {}' first to fetch price data",
            token_data.token_data.token,
            args.token_file.display()
        ));
    };

    // Apply time-based percentage range filtering
    let total_len = timestamps.len();
    if args.start_pct != 0.0 || args.end_pct != 100.0 {
        // Get the time range of the data
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
                filtered_values.push(values[i]);
            }
        }

        timestamps = filtered_timestamps;
        values = filtered_values;

        pb.set_message(format!(
            "📊 Using {:.1}%-{:.1}% time range ({} of {} data points, from {} to {})",
            args.start_pct,
            args.end_pct,
            timestamps.len(),
            total_len,
            start_time.format("%Y-%m-%d %H:%M"),
            end_time.format("%Y-%m-%d %H:%M")
        ));
    } else {
        pb.set_message(format!("📊 Using all {} data points", total_len));
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

    pb.set_message(format!(
        "📊 Input period: {:.1} days, forecast ratio: {:.1}%, forecast duration: {:.1} hours",
        input_duration.num_hours() as f64 / 24.0,
        args.forecast_ratio,
        Duration::milliseconds(forecast_duration_ms).num_hours() as f64
    ));

    let prediction_request = ZeroShotPredictionRequest {
        timestamp: timestamps,
        values,
        forecast_until,
        model_name: if args.model == "server_default" {
            None
        } else {
            Some(args.model.clone())
        },
        model_params: None,
    };

    // Execute prediction
    pb.set_message("Executing zero-shot prediction...");
    let prediction_response = chronos_client.predict_zero_shot(prediction_request).await?;

    // Poll for completion
    pb.set_message("Waiting for prediction to complete...");
    let completed_prediction = chronos_client
        .poll_prediction_until_complete(&prediction_response.task_id)
        .await?;

    let prediction_result = completed_prediction
        .result
        .ok_or_else(|| anyhow::anyhow!("No prediction result data"))?;

    // Convert ChronosPredictionResponse to Vec<PredictionPoint>
    let forecast: Vec<PredictionPoint> = prediction_result
        .forecast_timestamp
        .into_iter()
        .zip(prediction_result.forecast_values.into_iter())
        .map(|(timestamp, value)| PredictionPoint {
            timestamp,
            value,
            confidence_interval: None, // TODO: Add confidence intervals if available
        })
        .collect();

    // Create prediction result
    let prediction_result = TokenPredictionResult {
        token: token_data.token_data.token.clone(),
        prediction_id: completed_prediction.task_id,
        predicted_values: forecast,
        accuracy_metrics: None,
        chart_svg: None,
    };

    // Save results
    pb.set_message("Saving prediction results...");
    write_json_file(&prediction_file, &prediction_result).await?;

    pb.finish_with_message(format!(
        "Prediction completed for token: {} (saved to {:?})",
        token_data.token_data.token, prediction_file
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

async fn load_history_data(history_file: &PathBuf) -> Result<(Vec<DateTime<Utc>>, Vec<f64>)> {
    let content = fs::read_to_string(history_file).await?;
    let history_data: HistoryFileData = serde_json::from_str(&content)?;

    if history_data.price_history.values.is_empty() {
        return Err(anyhow::anyhow!("No price data found in history file"));
    }

    let mut timestamps = Vec::new();
    let mut values = Vec::new();

    for value_at_time in history_data.price_history.values {
        timestamps.push(value_at_time.time.and_utc());
        values.push(value_at_time.value);
    }

    Ok((timestamps, values))
}
