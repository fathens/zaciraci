use anyhow::Result;
use chrono::{DateTime, Utc};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;

use crate::api::backend::BackendApiClient;
use crate::models::{
    prediction::{PredictionPoint, TokenPredictionResult},
    token::TokenFileData,
    verification::{ComparisonPoint, VerificationMetrics, VerificationPeriod, VerificationReport},
};
use crate::utils::{
    config::Config,
    file::{ensure_directory_exists, file_exists, sanitize_filename, write_json_file},
};

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
    let config = Config::from_env();
    let backend_client = BackendApiClient::new(config.backend_url);

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

    // Auto-infer actual data file if not provided
    let actual_data_file = if let Some(file) = args.actual_data_file {
        file
    } else {
        infer_actual_data_file(&prediction_data.token)?
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

    // Create output directory structure
    let token_output_dir = args.output.join(&prediction_data.token);
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

    pb.set_message("Fetching actual data for verification period...");

    // Get actual data for the prediction period
    let actual_history = backend_client
        .get_token_history(&prediction_data.token, prediction_start, prediction_end)
        .await?;

    if actual_history.is_empty() {
        return Err(anyhow::anyhow!(
            "No actual data found for verification period: {} to {}",
            prediction_start.format("%Y-%m-%d %H:%M:%S"),
            prediction_end.format("%Y-%m-%d %H:%M:%S")
        ));
    }

    pb.set_message("Matching prediction and actual data points...");

    // Match prediction and actual data points
    let comparison_points = match_data_points(&prediction_data.predicted_values, &actual_history)?;

    if comparison_points.is_empty() {
        return Err(anyhow::anyhow!(
            "No matching data points found between prediction and actual data"
        ));
    }

    pb.set_message("Calculating verification metrics...");

    // Calculate verification metrics
    let metrics = calculate_verification_metrics(&comparison_points)?;

    // Create verification report
    let verification_report = VerificationReport {
        token: prediction_data.token.clone(),
        prediction_id: prediction_data.prediction_id.clone(),
        verification_date: Utc::now(),
        period: VerificationPeriod {
            start: prediction_start,
            end: prediction_end,
            predicted_points_count: prediction_data.predicted_values.len(),
            actual_points_count: actual_history.len(),
            matched_points_count: comparison_points.len(),
        },
        metrics,
        data_points: comparison_points,
    };

    // Save verification report
    pb.set_message("Saving verification report...");
    write_json_file(&verification_file, &verification_report).await?;

    pb.finish_with_message(format!(
        "Verification completed for token: {} (saved to {:?})",
        prediction_data.token, token_output_dir
    ));

    // Display summary
    println!("\n=== Verification Summary ===");
    println!("Token: {}", verification_report.token);
    println!("Prediction ID: {}", verification_report.prediction_id);
    println!(
        "Period: {} to {}",
        verification_report.period.start.format("%Y-%m-%d %H:%M:%S"),
        verification_report.period.end.format("%Y-%m-%d %H:%M:%S")
    );
    println!(
        "Data Points: {} predicted, {} actual, {} matched",
        verification_report.period.predicted_points_count,
        verification_report.period.actual_points_count,
        verification_report.period.matched_points_count
    );
    println!("Metrics:");
    println!("  MAE: {:.4}", verification_report.metrics.mae);
    println!("  RMSE: {:.4}", verification_report.metrics.rmse);
    println!("  MAPE: {:.2}%", verification_report.metrics.mape);
    println!(
        "  Direction Accuracy: {:.2}%",
        verification_report.metrics.direction_accuracy * 100.0
    );
    println!(
        "  Correlation: {:.4}",
        verification_report.metrics.correlation
    );

    Ok(())
}

pub fn infer_actual_data_file(token_name: &str) -> Result<PathBuf> {
    let sanitized_name = sanitize_filename(token_name);
    let filename = format!("{}.json", sanitized_name);
    Ok(PathBuf::from("tokens").join(filename))
}

fn match_data_points(
    predicted_values: &[PredictionPoint],
    actual_history: &[(DateTime<Utc>, f64)],
) -> Result<Vec<ComparisonPoint>> {
    let mut comparison_points = Vec::new();

    for prediction_point in predicted_values {
        // Find the closest actual data point by timestamp
        if let Some((_actual_timestamp, actual_value)) =
            actual_history.iter().min_by_key(|(timestamp, _)| {
                (timestamp.timestamp() - prediction_point.timestamp.timestamp()).abs()
            })
        {
            let error = prediction_point.value - actual_value;
            let percentage_error = if *actual_value != 0.0 {
                (error / actual_value) * 100.0
            } else {
                0.0
            };

            comparison_points.push(ComparisonPoint {
                timestamp: prediction_point.timestamp,
                predicted_value: prediction_point.value,
                actual_value: *actual_value,
                error,
                percentage_error,
            });
        }
    }

    Ok(comparison_points)
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
