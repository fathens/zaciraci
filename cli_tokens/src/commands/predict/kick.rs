use anyhow::Result;
use bigdecimal::BigDecimal;
use chrono::{DateTime, Duration, Utc};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use tokio::fs;

use crate::models::{
    history::HistoryFileData,
    task::{PredictionParams, TaskInfo},
    token::TokenFileData,
};
use crate::utils::{
    cache::get_price_history_dir,
    config::Config,
    file::{ensure_directory_exists, file_exists, sanitize_filename, write_json_file},
    scaling::scale_values,
};
use common::api::chronos::ChronosApiClient;
use common::prediction::ZeroShotPredictionRequest;

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
#[clap(about = "Start an async prediction task and exit")]
pub struct KickArgs {
    #[clap(help = "Token file path (e.g., tokens/wrap.near.json)")]
    pub token_file: PathBuf,

    #[clap(short, long, default_value = "predictions", help = "Output directory")]
    pub output: PathBuf,

    #[clap(
        short,
        long,
        help = "Prediction model (defaults to server's default model if not specified)"
    )]
    pub model: Option<String>,

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

    let config = Config::from_env();
    let chronos_client = ChronosApiClient::new(config.chronos_url);

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

    // Prepare output directory with model name
    let base_dir = std::env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
    let model_name = args
        .model
        .as_ref()
        .unwrap_or(&"chronos_default".to_string())
        .clone();

    // For now, we'll save task files in a temporary location
    // The actual prediction results will go to the structured directory
    let task_dir = PathBuf::from(&base_dir).join(".tasks").join(&args.output);
    ensure_directory_exists(&task_dir)?;

    let task_file = task_dir.join(format!(
        "{}_{}.task.json",
        sanitize_filename(&token_data.token),
        sanitize_filename(&model_name)
    ));

    // Check if task file already exists
    if file_exists(&task_file).await {
        return Err(anyhow::anyhow!(
            "Task file already exists: {:?}. Please remove it first or use a different model name",
            task_file
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

    // Scale values to 0-1,000,000 range using min-max normalization
    let scale_result = scale_values(&values);
    let scaled_values = scale_result.values;
    let scale_params = scale_result.params;

    pb.set_message(format!(
        "📊 Values scaled to 0-1,000,000 range (original: {} - {})",
        scale_params.original_min, scale_params.original_max
    ));

    pb.set_message(format!(
        "📊 Input period: {:.1} days, forecast ratio: {:.1}%, forecast duration: {:.1} hours",
        input_duration.num_hours() as f64 / 24.0,
        args.forecast_ratio,
        Duration::milliseconds(forecast_duration_ms).num_hours() as f64
    ));

    let prediction_request = ZeroShotPredictionRequest {
        timestamp: timestamps,
        values: scaled_values,
        forecast_until,
        model_name: args.model.clone(),
        model_params: None,
    };

    // Execute prediction (start async task)
    pb.set_message("Starting prediction task...");
    let prediction_response = chronos_client.predict_zero_shot(prediction_request).await?;

    // Create and save task info
    let task_info = TaskInfo::new(
        prediction_response.task_id.clone(),
        args.token_file.clone(),
        args.model.clone(),
        PredictionParams {
            start_pct: args.start_pct,
            end_pct: args.end_pct,
            forecast_ratio: args.forecast_ratio,
            scale_params,
        },
    );

    write_json_file(&task_file, &task_info).await?;

    pb.finish_with_message(format!(
        "✅ Prediction task started for token: {} (task_id: {}, saved to {:?})",
        token_data.token, prediction_response.task_id, task_file
    ));

    println!("\nTo retrieve results, run:");
    println!(
        "  cli_tokens predict pull {} --output {}",
        args.token_file.display(),
        args.output.display()
    );

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
) -> Result<(Vec<DateTime<Utc>>, Vec<BigDecimal>)> {
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
    let values: Vec<BigDecimal> = history_data
        .price_history
        .values
        .iter()
        .map(|v| v.value.clone().into_bigdecimal())
        .collect();

    if timestamps.len() != values.len() {
        return Err(anyhow::anyhow!(
            "Timestamp and value arrays have different lengths"
        ));
    }

    Ok((timestamps, values))
}
