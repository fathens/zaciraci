use anyhow::Result;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use tokio::fs;

use crate::models::{task::TaskInfo, token::TokenFileData};
use crate::utils::{
    config::Config,
    file::{file_exists, sanitize_filename, write_json_file},
};
use common::api::chronos::ChronosApiClient;
use common::prediction::{PredictionPoint, TokenPredictionResult};

/// Find the task file for a given token
async fn find_task_file(dir: &Path, token_name: &str) -> Result<Option<PathBuf>> {
    if !dir.exists() {
        return Ok(None);
    }

    let sanitized_token = sanitize_filename(token_name);
    let mut entries = fs::read_dir(dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if name.starts_with(&sanitized_token) && name.ends_with(".task.json") {
                    return Ok(Some(path));
                }
            }
        }
    }

    Ok(None)
}

#[derive(Parser)]
#[clap(about = "Poll for prediction results")]
pub struct PullArgs {
    #[clap(help = "Token file path (e.g., tokens/wrap.near.json)")]
    pub token_file: PathBuf,

    #[clap(short, long, default_value = "predictions", help = "Output directory")]
    pub output: PathBuf,

    #[clap(long, default_value = "30", help = "Maximum number of poll attempts")]
    pub max_polls: u32,

    #[clap(long, default_value = "2", help = "Poll interval in seconds")]
    pub poll_interval: u64,
}

pub async fn run(args: PullArgs) -> Result<()> {
    let config = Config::from_env();
    let chronos_client = ChronosApiClient::new(config.chronos_url);

    // Read token file to get token info
    if !file_exists(&args.token_file).await {
        return Err(anyhow::anyhow!(
            "Token file not found: {:?}",
            args.token_file
        ));
    }

    let content = fs::read_to_string(&args.token_file).await?;
    let token_data: TokenFileData = serde_json::from_str(&content)?;

    // Prepare paths
    let base_dir = std::env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());

    // Look for task file in the .tasks directory
    let task_dir = PathBuf::from(&base_dir).join(".tasks").join(&args.output);

    // Find the task file - it should have the token name in it
    let task_file = find_task_file(&task_dir, &token_data.token).await?;

    // We'll determine the actual output location from the task info
    let prediction_file = PathBuf::from(&base_dir)
        .join(&args.output)
        .join("temp")
        .join(format!("{}.json", sanitize_filename(&token_data.token)));

    // Read task info
    if task_file.is_none() {
        return Err(anyhow::anyhow!(
            "Task file not found for token: {}. Please run 'cli_tokens predict kick' first",
            token_data.token
        ));
    }

    let task_file = task_file.unwrap();
    let task_content = fs::read_to_string(&task_file).await?;
    let mut task_info: TaskInfo = serde_json::from_str(&task_content)?;

    // Show progress
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );

    pb.set_message(format!(
        "Polling prediction task {} (current status: {})",
        task_info.task_id, task_info.last_status
    ));

    // Poll for completion with limited attempts
    let mut poll_count = 0u32;
    let completed_prediction = loop {
        if poll_count >= args.max_polls {
            return Err(anyhow::anyhow!(
                "Polling timeout: Maximum poll attempts ({}) reached. Task is still {}",
                args.max_polls,
                task_info.last_status
            ));
        }

        poll_count += 1;
        let response = chronos_client
            .get_prediction_status(&task_info.task_id)
            .await?;

        // Update task info
        task_info.update_status(response.status.clone());

        pb.set_message(format!(
            "Poll attempt {}/{}: Status = {}, Progress = {:?}",
            poll_count, args.max_polls, response.status, response.progress
        ));

        if let Some(message) = &response.message {
            pb.set_message(format!("Status: {} - {}", response.status, message));
        }

        match response.status.as_str() {
            "completed" => {
                pb.set_message("Prediction completed successfully!");
                break response;
            }
            "failed" => {
                let error_msg = response.error.unwrap_or("Unknown error".to_string());
                return Err(anyhow::anyhow!("Prediction failed: {}", error_msg));
            }
            "running" | "pending" => {
                // Save updated task info
                write_json_file(&task_file, &task_info).await?;

                // Wait before next poll
                tokio::time::sleep(std::time::Duration::from_secs(args.poll_interval)).await;
            }
            _ => {
                return Err(anyhow::anyhow!(
                    "Unknown prediction status: {}",
                    response.status
                ))
            }
        }
    };

    let prediction_result = completed_prediction
        .result
        .ok_or_else(|| anyhow::anyhow!("No prediction result data"))?;

    // Convert ChronosPredictionResponse to Vec<PredictionPoint>
    let mut forecast: Vec<PredictionPoint> = prediction_result
        .forecast_timestamp
        .into_iter()
        .zip(prediction_result.forecast_values.into_iter())
        .enumerate()
        .map(|(i, (timestamp, value))| {
            // Extract confidence intervals if available
            let confidence_interval =
                prediction_result
                    .confidence_intervals
                    .as_ref()
                    .and_then(|intervals| {
                        // Common patterns for confidence interval keys
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
                                    lower: lower_values[i],
                                    upper: upper_values[i],
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    });

            PredictionPoint {
                timestamp,
                value,
                confidence_interval,
            }
        })
        .collect();

    // Restore original scale if values were scaled down
    if let Some(scale_factor) = task_info.params.scale_factor {
        pb.set_message(format!(
            "📊 Restoring values to original scale (factor: {:.2e})",
            scale_factor
        ));

        for point in &mut forecast {
            point.value *= scale_factor;
            // Also scale confidence intervals if present
            if let Some(ref mut ci) = point.confidence_interval {
                ci.lower *= scale_factor;
                ci.upper *= scale_factor;
            }
        }
    }

    // Create prediction result
    let prediction_result = TokenPredictionResult {
        token: token_data.token.clone(),
        prediction_id: completed_prediction.task_id,
        predicted_values: forecast,
        accuracy_metrics: None,
        chart_svg: None,
    };

    // Save results
    pb.set_message("Saving prediction results...");
    write_json_file(&prediction_file, &prediction_result).await?;

    // Update task info to completed
    task_info.update_status("completed".to_string());
    write_json_file(&task_file, &task_info).await?;

    pb.finish_with_message(format!(
        "✅ Prediction completed for token: {} (saved to {:?})",
        token_data.token, prediction_file
    ));

    Ok(())
}

#[cfg(test)]
mod tests;
