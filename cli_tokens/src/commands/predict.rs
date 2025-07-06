use anyhow::Result;
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use tokio::fs;

use crate::api::{backend::BackendApiClient, chronos::ChronosApiClient};
use crate::models::{
    prediction::{PredictionPoint, TokenPredictionResult, ZeroShotPredictionRequest},
    token::TokenFileData,
};
use crate::utils::{
    config::Config,
    file::{ensure_directory_exists, file_exists, write_json_file},
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
        help = "Start percentage of data range (0.0-100.0)"
    )]
    pub start_pct: f64,

    #[clap(
        long,
        default_value = "100.0",
        help = "End percentage of data range (0.0-100.0)"
    )]
    pub end_pct: f64,
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

    let config = Config::from_env();
    let backend_client = BackendApiClient::new(config.backend_url);
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

    // Create output directory structure
    let token_output_dir = args.output.join(&token_data.token_data.token);
    ensure_directory_exists(&token_output_dir)?;

    let prediction_file = token_output_dir.join("prediction.json");

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
    pb.set_message("Fetching historical token data...");
    let start_date = NaiveDate::parse_from_str(&token_data.metadata.start_date, "%Y-%m-%d")?
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| anyhow::anyhow!("Invalid start date"))?;
    let start_date = Utc.from_utc_datetime(&start_date);

    let end_date = NaiveDate::parse_from_str(&token_data.metadata.end_date, "%Y-%m-%d")?
        .and_hms_opt(23, 59, 59)
        .ok_or_else(|| anyhow::anyhow!("Invalid end date"))?;
    let end_date = Utc.from_utc_datetime(&end_date);

    let history = backend_client
        .get_token_history(&token_data.token_data.token, start_date, end_date)
        .await?;

    if history.is_empty() {
        return Err(anyhow::anyhow!(
            "No historical data found for token: {}",
            token_data.token_data.token
        ));
    }

    // Prepare prediction request
    let (mut timestamps, mut values): (Vec<DateTime<Utc>>, Vec<f64>) = history.into_iter().unzip();

    // Apply percentage range filtering
    let total_len = timestamps.len();
    if args.start_pct != 0.0 || args.end_pct != 100.0 {
        let start_idx = ((total_len as f64) * (args.start_pct / 100.0)).round() as usize;
        let end_idx = ((total_len as f64) * (args.end_pct / 100.0)).round() as usize;

        let start_idx = start_idx.min(total_len);
        let end_idx = end_idx.min(total_len).max(start_idx);

        timestamps = timestamps[start_idx..end_idx].to_vec();
        values = values[start_idx..end_idx].to_vec();

        pb.set_message(format!(
            "📊 Using {:.1}%-{:.1}% range ({} of {} data points)",
            args.start_pct,
            args.end_pct,
            timestamps.len(),
            total_len
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
    let forecast_until = *latest_timestamp + Duration::hours(12); // Predict 12 hours ahead for faster processing

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
        .poll_prediction_until_complete(&prediction_response.task_id, 5) // 5 polls for quick test
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
        token_data.token_data.token, token_output_dir
    ));

    Ok(())
}
