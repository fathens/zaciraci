use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use tokio::fs;

use crate::api::{backend::BackendApiClient, chronos::ChronosApiClient};
use crate::models::{
    prediction::{AccuracyMetrics, PredictionResult, ZeroShotPredictionRequest},
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
}

pub async fn run(args: PredictArgs) -> Result<()> {
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
    let metrics_file = token_output_dir.join("metrics.json");

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
    let start_date =
        DateTime::parse_from_str(&token_data.metadata.start_date, "%Y-%m-%d")?.with_timezone(&Utc);
    let end_date =
        DateTime::parse_from_str(&token_data.metadata.end_date, "%Y-%m-%d")?.with_timezone(&Utc);

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
    let forecast_until = end_date + Duration::days(7); // Predict 7 days ahead
    let (timestamps, values): (Vec<DateTime<Utc>>, Vec<f64>) = history.into_iter().unzip();

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
        .poll_prediction_until_complete(&prediction_response.id, 150) // 5 minute timeout
        .await?;

    let forecast = completed_prediction
        .forecast
        .ok_or_else(|| anyhow::anyhow!("No forecast data in completed prediction"))?;

    // Create prediction result
    let prediction_result = PredictionResult {
        token: token_data.token_data.token.clone(),
        prediction_id: completed_prediction.id,
        predicted_values: forecast,
        accuracy_metrics: Some(AccuracyMetrics {
            mae: 0.0, // TODO: Calculate actual metrics
            rmse: 0.0,
            mape: 0.0,
        }),
        chart_svg: None, // TODO: Generate chart
    };

    // Save results
    pb.set_message("Saving prediction results...");
    write_json_file(&prediction_file, &prediction_result).await?;

    if let Some(metrics) = &prediction_result.accuracy_metrics {
        write_json_file(&metrics_file, metrics).await?;
    }

    pb.finish_with_message(format!(
        "Prediction completed for token: {} (saved to {:?})",
        token_data.token_data.token, token_output_dir
    ));

    Ok(())
}
