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

    // Get or extract quote token
    let quote_token = extract_quote_token_from_path(&args.token_file)
        .or_else(|| token_data.metadata.quote_token.clone())
        .unwrap_or_else(|| "wrap.near".to_string());

    // Prepare paths
    let base_dir = std::env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
    let output_dir = PathBuf::from(base_dir)
        .join(&args.output)
        .join(sanitize_filename(&quote_token));

    let task_file = output_dir.join(format!(
        "{}.task.json",
        sanitize_filename(&token_data.token_data.token)
    ));

    let prediction_file = output_dir.join(format!(
        "{}.json",
        sanitize_filename(&token_data.token_data.token)
    ));

    // Read task info
    if !file_exists(&task_file).await {
        return Err(anyhow::anyhow!(
            "Task file not found: {:?}. Please run 'cli_tokens predict kick' first",
            task_file
        ));
    }

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

    // Update task info to completed
    task_info.update_status("completed".to_string());
    write_json_file(&task_file, &task_info).await?;

    pb.finish_with_message(format!(
        "✅ Prediction completed for token: {} (saved to {:?})",
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
