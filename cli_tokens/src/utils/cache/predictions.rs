use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};
use tokio::fs;

use crate::models::prediction::PredictionFileData;
use crate::utils::file::sanitize_filename;

// Import common utilities from parent module
use super::{format_datetime, get_base_directory};

/// Parameters for prediction cache operations
#[derive(Debug, Clone)]
pub struct PredictionCacheParams<'a> {
    pub model_name: &'a str,
    pub quote_token: &'a str,
    pub base_token: &'a str,
    pub hist_start: DateTime<Utc>,
    pub hist_end: DateTime<Utc>,
    pub pred_start: DateTime<Utc>,
    pub pred_end: DateTime<Utc>,
}

/// Get the path for prediction directory
/// Structure: predictions/{model_name}/{quote_token}/{base_token}/history-{hist_start}-{hist_end}/
pub fn get_prediction_dir(
    model_name: &str,
    quote_token: &str,
    base_token: &str,
    hist_start: DateTime<Utc>,
    hist_end: DateTime<Utc>,
) -> PathBuf {
    let base_dir = get_base_directory();
    let history_dir_name = format!(
        "history-{}-{}",
        format_datetime(&hist_start),
        format_datetime(&hist_end)
    );

    PathBuf::from(base_dir)
        .join("predictions")
        .join(sanitize_filename(model_name))
        .join(sanitize_filename(quote_token))
        .join(sanitize_filename(base_token))
        .join(history_dir_name)
}

/// Create a prediction file name for the given prediction time range
/// Format: predict-{pred_start}-{pred_end}.json
pub fn create_prediction_filename(pred_start: DateTime<Utc>, pred_end: DateTime<Utc>) -> String {
    format!(
        "predict-{}-{}.json",
        format_datetime(&pred_start),
        format_datetime(&pred_end)
    )
}

/// Find the latest prediction file in a directory structure
/// Searches through all model directories for the most recent prediction file
pub async fn find_latest_prediction_file(
    predictions_dir: &Path,
    quote_token: &str,
    token_name: &str,
) -> Result<Option<PathBuf>> {
    if !predictions_dir.exists() {
        return Ok(None);
    }

    let mut latest_file: Option<(PathBuf, std::time::SystemTime)> = None;

    // Walk through model directories
    let mut entries = fs::read_dir(predictions_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let model_dir = entry.path();
        if !model_dir.is_dir() {
            continue;
        }

        // Check each model directory for token files
        let token_dir = model_dir
            .join(sanitize_filename(quote_token))
            .join(token_name);

        if !token_dir.exists() {
            continue;
        }

        // Look for prediction files in history subdirectories
        let mut history_entries = fs::read_dir(&token_dir).await?;
        while let Some(history_entry) = history_entries.next_entry().await? {
            let history_dir = history_entry.path();
            if !history_dir.is_dir() {
                continue;
            }

            // Look for prediction files in this history directory
            let mut pred_entries = fs::read_dir(&history_dir).await?;
            while let Some(pred_entry) = pred_entries.next_entry().await? {
                let pred_path = pred_entry.path();
                if pred_path.is_file()
                    && pred_path.extension().and_then(|s| s.to_str()) == Some("json")
                {
                    if let Some(name) = pred_path.file_name().and_then(|s| s.to_str()) {
                        if name.starts_with("predict-") {
                            let metadata = pred_entry.metadata().await?;
                            let modified = metadata.modified()?;

                            if latest_file.is_none() || latest_file.as_ref().unwrap().1 < modified {
                                latest_file = Some((pred_path, modified));
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(latest_file.map(|(path, _)| path))
}

/// Save prediction result to structured cache
/// Creates the full directory structure and saves the prediction file
pub async fn save_prediction_result(
    params: &PredictionCacheParams<'_>,
    prediction_data: &PredictionFileData,
) -> Result<PathBuf> {
    let prediction_dir = get_prediction_dir(
        params.model_name,
        params.quote_token,
        params.base_token,
        params.hist_start,
        params.hist_end,
    );
    fs::create_dir_all(&prediction_dir)
        .await
        .context("Failed to create prediction directory")?;

    let filename = create_prediction_filename(params.pred_start, params.pred_end);
    let file_path = prediction_dir.join(filename);

    let json_content = serde_json::to_string_pretty(prediction_data)
        .context("Failed to serialize prediction data")?;

    fs::write(&file_path, json_content)
        .await
        .with_context(|| format!("Failed to write prediction file: {}", file_path.display()))?;

    Ok(file_path)
}

/// Load prediction data from a file
pub async fn load_prediction_data(file_path: &Path) -> Result<PredictionFileData> {
    let content = fs::read_to_string(file_path)
        .await
        .with_context(|| format!("Failed to read prediction file: {}", file_path.display()))?;

    let prediction_data: PredictionFileData = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse prediction file: {}", file_path.display()))?;

    Ok(prediction_data)
}

/// Check if a prediction file exists for the given parameters
pub async fn check_prediction_cache(params: &PredictionCacheParams<'_>) -> Result<Option<PathBuf>> {
    let prediction_dir = get_prediction_dir(
        params.model_name,
        params.quote_token,
        params.base_token,
        params.hist_start,
        params.hist_end,
    );

    // Use async fs::metadata to check directory existence more reliably
    if fs::metadata(&prediction_dir).await.is_err() {
        return Ok(None);
    }

    let filename = create_prediction_filename(params.pred_start, params.pred_end);
    let file_path = prediction_dir.join(filename);

    // Use async fs::metadata to check file existence
    match fs::metadata(&file_path).await {
        Ok(_) => Ok(Some(file_path)),
        Err(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    include!("predictions/tests.rs");
}
