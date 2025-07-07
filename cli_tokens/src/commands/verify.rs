use anyhow::Result;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};

use crate::models::{
    prediction::TokenPredictionResult,
    token::TokenFileData,
    verification::{ComparisonPoint, VerificationMetrics},
};
use crate::utils::file::{ensure_directory_exists, file_exists, sanitize_filename};

#[derive(Parser)]
#[clap(about = "Verify prediction accuracy against actual data")]
pub struct VerifyArgs {
    #[clap(help = "Prediction file path (e.g., predictions/wrap.near.json)")]
    pub prediction_file: PathBuf,

    #[clap(
        long,
        help = "Actual data file path (defaults to auto-inferred: tokens/{token}.json)"
    )]
    pub actual_data_file: Option<PathBuf>,

    #[clap(short, long, default_value = "verification", help = "Output directory")]
    pub output: PathBuf,

    #[clap(long, help = "Force overwrite existing verification results")]
    pub force: bool,
}

pub async fn run(args: VerifyArgs) -> Result<()> {
    // Read prediction file
    if !file_exists(&args.prediction_file).await {
        return Err(anyhow::anyhow!(
            "Prediction file not found: {:?}",
            args.prediction_file
        ));
    }

    let prediction_content = tokio::fs::read_to_string(&args.prediction_file).await?;
    let prediction_data: TokenPredictionResult = serde_json::from_str(&prediction_content)?;

    println!("Verifying prediction for token: {}", prediction_data.token);

    // Extract quote_token from prediction file path
    let quote_token =
        extract_quote_token_from_path(&args.prediction_file).unwrap_or("wrap.near".to_string());

    // Auto-infer actual data file if not provided
    let actual_data_file = if let Some(file) = args.actual_data_file {
        file
    } else {
        infer_actual_data_file(&prediction_data.token, &quote_token)?
    };

    // Verify actual data file exists
    if !file_exists(&actual_data_file).await {
        return Err(anyhow::anyhow!(
            "Actual data file not found: {:?}. Use --actual-data-file to specify manually.",
            actual_data_file
        ));
    }

    // Read actual data file
    let actual_content = tokio::fs::read_to_string(&actual_data_file).await?;
    let actual_data: TokenFileData = serde_json::from_str(&actual_content)?;

    // Verify token names match
    if prediction_data.token != actual_data.token_data.token {
        return Err(anyhow::anyhow!(
            "Token mismatch: prediction has '{}', actual data has '{}'",
            prediction_data.token,
            actual_data.token_data.token
        ));
    }

    // Create output directory structure (${quote_token}/${base_token}/)
    let quote_dir = args.output.join(sanitize_filename(&quote_token));
    let token_output_dir = quote_dir.join(sanitize_filename(&prediction_data.token));
    ensure_directory_exists(&token_output_dir)?;

    let verification_file = token_output_dir.join("verification_report.json");

    // Check if verification already exists
    if !args.force && file_exists(&verification_file).await {
        return Err(anyhow::anyhow!(
            "Verification already exists: {:?}. Use --force to overwrite",
            verification_file
        ));
    }

    // Show progress
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );

    // Get prediction period
    let prediction_start = prediction_data
        .predicted_values
        .first()
        .ok_or_else(|| anyhow::anyhow!("No prediction values found"))?
        .timestamp;
    let prediction_end = prediction_data
        .predicted_values
        .last()
        .ok_or_else(|| anyhow::anyhow!("No prediction values found"))?
        .timestamp;

    pb.set_message("Checking verification requirements...");

    // Check if verification is possible with current data
    Err(anyhow::anyhow!(
        "Verification requires real-time data for the prediction period ({} to {}). \
        The actual data file ({}) only contains historical data up to the prediction start. \
        For verification, you would need to wait for the prediction period to pass and \
        then fetch the actual price data for that period using the history command.",
        prediction_start.format("%Y-%m-%d %H:%M:%S"),
        prediction_end.format("%Y-%m-%d %H:%M:%S"),
        actual_data_file.display()
    ))
}

/// Extract quote_token from prediction file path (e.g., predictions/wrap.near/usdc.tether-token.near.json -> wrap.near)
fn extract_quote_token_from_path(prediction_file: &Path) -> Option<String> {
    prediction_file
        .parent()?
        .file_name()?
        .to_str()
        .map(|s| s.to_string())
        .filter(|s| s != "predictions") // Skip if direct under predictions/ directory
}

pub fn infer_actual_data_file(token_name: &str, quote_token: &str) -> Result<PathBuf> {
    let sanitized_token = sanitize_filename(token_name);
    let sanitized_quote = sanitize_filename(quote_token);
    let filename = format!("{}.json", sanitized_token);
    Ok(PathBuf::from("tokens").join(sanitized_quote).join(filename))
}

pub fn calculate_verification_metrics(
    comparison_points: &[ComparisonPoint],
) -> Result<VerificationMetrics> {
    if comparison_points.is_empty() {
        return Err(anyhow::anyhow!(
            "No comparison points available for metrics calculation"
        ));
    }

    let n = comparison_points.len() as f64;

    // Calculate MAE (Mean Absolute Error)
    let mae = comparison_points
        .iter()
        .map(|point| point.error.abs())
        .sum::<f64>()
        / n;

    // Calculate RMSE (Root Mean Square Error)
    let mse = comparison_points
        .iter()
        .map(|point| point.error.powi(2))
        .sum::<f64>()
        / n;
    let rmse = mse.sqrt();

    // Calculate MAPE (Mean Absolute Percentage Error)
    let mape = comparison_points
        .iter()
        .map(|point| point.percentage_error.abs())
        .sum::<f64>()
        / n;

    // Calculate Direction Accuracy
    let correct_directions = comparison_points
        .windows(2)
        .filter(|window| {
            let predicted_direction = window[1].predicted_value > window[0].predicted_value;
            let actual_direction = window[1].actual_value > window[0].actual_value;
            predicted_direction == actual_direction
        })
        .count();

    let direction_accuracy = if comparison_points.len() > 1 {
        correct_directions as f64 / (comparison_points.len() - 1) as f64
    } else {
        0.0
    };

    // Calculate Correlation
    let predicted_mean = comparison_points
        .iter()
        .map(|point| point.predicted_value)
        .sum::<f64>()
        / n;
    let actual_mean = comparison_points
        .iter()
        .map(|point| point.actual_value)
        .sum::<f64>()
        / n;

    let numerator = comparison_points
        .iter()
        .map(|point| (point.predicted_value - predicted_mean) * (point.actual_value - actual_mean))
        .sum::<f64>();

    let predicted_variance = comparison_points
        .iter()
        .map(|point| (point.predicted_value - predicted_mean).powi(2))
        .sum::<f64>();

    let actual_variance = comparison_points
        .iter()
        .map(|point| (point.actual_value - actual_mean).powi(2))
        .sum::<f64>();

    let correlation = if predicted_variance > 0.0 && actual_variance > 0.0 {
        numerator / (predicted_variance.sqrt() * actual_variance.sqrt())
    } else {
        0.0
    };

    Ok(VerificationMetrics {
        mae,
        rmse,
        mape,
        direction_accuracy,
        correlation,
    })
}
