use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};
use tokio::fs;

use crate::api::backend::BackendClient;
use crate::models::history::{HistoryFileData, HistoryMetadata, PriceHistory};
use crate::utils::file::sanitize_filename;
use common::stats::ValueAtTime;

/// Get the base directory for cache operations
pub fn get_base_directory() -> String {
    std::env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string())
}

/// Format a datetime for use in filenames
pub fn format_datetime(dt: &DateTime<Utc>) -> String {
    dt.format("%Y%m%d_%H%M").to_string()
}

/// Parse datetime from filename format
pub fn parse_datetime(s: &str) -> Result<DateTime<Utc>> {
    chrono::DateTime::parse_from_str(&format!("{}+00:00", s), "%Y%m%d_%H%M%z")
        .map(|dt| dt.with_timezone(&Utc))
        .context("Failed to parse datetime from filename")
}

/// Find the latest history file in a directory
pub async fn find_latest_history_file(dir: &Path) -> Result<Option<PathBuf>> {
    if !dir.exists() {
        return Ok(None);
    }

    let mut entries = fs::read_dir(dir).await?;
    let mut latest_file: Option<(PathBuf, std::time::SystemTime)> = None;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if name.starts_with("history-") {
                    let metadata = entry.metadata().await?;
                    let modified = metadata.modified()?;

                    if latest_file.is_none() || latest_file.as_ref().unwrap().1 < modified {
                        latest_file = Some((path, modified));
                    }
                }
            }
        }
    }

    Ok(latest_file.map(|(path, _)| path))
}

/// Find history files that overlap with a given time range
pub async fn find_overlapping_history_files(
    dir: &Path,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut matching_files = Vec::new();
    let mut entries = fs::read_dir(dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if name.starts_with("history-") && name.ends_with(".json") {
                    // Extract date range from filename: history-start-end.json
                    let date_part = &name[8..name.len() - 5]; // Remove "history-" prefix and ".json" suffix
                    if let Some((start_str, end_str)) = date_part.split_once('-') {
                        if let (Ok(file_start), Ok(file_end)) =
                            (parse_datetime(start_str), parse_datetime(end_str))
                        {
                            // Check if time ranges overlap
                            if file_start <= end_time && file_end >= start_time {
                                matching_files.push(path);
                            }
                        }
                    }
                }
            }
        }
    }

    // Sort by start time (implied by filename)
    matching_files.sort();
    Ok(matching_files)
}

/// Load history data from a file
pub async fn load_history_data(file_path: &Path) -> Result<HistoryFileData> {
    let content = fs::read_to_string(file_path)
        .await
        .with_context(|| format!("Failed to read history file: {}", file_path.display()))?;

    let history_data: HistoryFileData = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse history file: {}", file_path.display()))?;

    Ok(history_data)
}

/// Load and merge history data from multiple files, filtering by time range
pub async fn load_merged_history_data(
    files: &[PathBuf],
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
) -> Result<Vec<ValueAtTime>> {
    let mut all_values = Vec::new();

    for file_path in files {
        let history_data = load_history_data(file_path).await?;

        // Filter values by time range
        let filtered_values: Vec<ValueAtTime> = history_data
            .price_history
            .values
            .into_iter()
            .filter(|v| {
                let value_time: DateTime<Utc> = DateTime::from_naive_utc_and_offset(v.time, Utc);
                value_time >= start_time && value_time <= end_time
            })
            .collect();

        all_values.extend(filtered_values);
    }

    // Sort by timestamp
    all_values.sort_by_key(|v| v.time);

    // Remove duplicates (keep the latest value for each timestamp)
    all_values.dedup_by_key(|v| v.time);

    Ok(all_values)
}

/// Get the path for price history directory
pub fn get_price_history_dir(quote_token: &str, base_token: &str) -> PathBuf {
    let base_dir = get_base_directory();
    PathBuf::from(base_dir)
        .join("price_history")
        .join(sanitize_filename(quote_token))
        .join(sanitize_filename(base_token))
}

/// Create a history file name for the given time range
pub fn create_history_filename(start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> String {
    format!(
        "history-{}-{}.json",
        format_datetime(&start_time),
        format_datetime(&end_time)
    )
}

/// Save history data to cache
pub async fn save_history_data(
    quote_token: &str,
    base_token: &str,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    values: &[ValueAtTime],
) -> Result<PathBuf> {
    let history_dir = get_price_history_dir(quote_token, base_token);
    fs::create_dir_all(&history_dir)
        .await
        .context("Failed to create history directory")?;

    let filename = create_history_filename(start_time, end_time);
    let file_path = history_dir.join(filename);

    let history_data = HistoryFileData {
        metadata: HistoryMetadata {
            generated_at: Utc::now(),
            start_date: start_time.format("%Y-%m-%d").to_string(),
            end_date: end_time.format("%Y-%m-%d").to_string(),
            base_token: base_token.to_string(),
            quote_token: quote_token.to_string(),
        },
        price_history: PriceHistory {
            values: values.to_vec(),
        },
    };

    let json_content =
        serde_json::to_string_pretty(&history_data).context("Failed to serialize history data")?;

    fs::write(&file_path, json_content)
        .await
        .with_context(|| format!("Failed to write history file: {}", file_path.display()))?;

    Ok(file_path)
}

/// Check if cached history data exists and is sufficient for the requested time range
pub async fn check_history_cache(
    quote_token: &str,
    base_token: &str,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
) -> Result<Option<Vec<ValueAtTime>>> {
    let history_dir = get_price_history_dir(quote_token, base_token);

    let overlapping_files =
        find_overlapping_history_files(&history_dir, start_time, end_time).await?;

    if overlapping_files.is_empty() {
        return Ok(None);
    }

    // Check if we have complete coverage
    let merged_data = load_merged_history_data(&overlapping_files, start_time, end_time).await?;

    if merged_data.is_empty() {
        return Ok(None);
    }

    // Check if we have data covering the requested time range
    let first_time: DateTime<Utc> =
        DateTime::from_naive_utc_and_offset(merged_data.first().unwrap().time, Utc);
    let last_time: DateTime<Utc> =
        DateTime::from_naive_utc_and_offset(merged_data.last().unwrap().time, Utc);

    // We need data that starts before or at start_time and ends after or at end_time
    if first_time <= start_time && last_time >= end_time {
        Ok(Some(merged_data))
    } else {
        // Partial data available, but not complete coverage
        Ok(None)
    }
}

/// Fetch price history with cache support
/// First checks cache, then fetches from API if needed
pub async fn fetch_price_history_with_cache(
    backend_client: &BackendClient,
    quote_token: &str,
    base_token: &str,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
) -> Result<Vec<ValueAtTime>> {
    // Check cache first
    if let Some(cached_data) =
        check_history_cache(quote_token, base_token, start_time, end_time).await?
    {
        println!(
            "  ✅ Using cached data for {} ({} data points)",
            base_token,
            cached_data.len()
        );
        return Ok(cached_data);
    }

    // Fetch from API
    println!("  🌐 Fetching data from API for {}", base_token);
    let values = backend_client
        .get_price_history(
            quote_token,
            base_token,
            start_time.naive_utc(),
            end_time.naive_utc(),
        )
        .await
        .with_context(|| {
            format!(
                "Failed to fetch price history for {} (quote: {}) from {} to {}",
                base_token, quote_token, start_time, end_time
            )
        })?;

    if values.is_empty() {
        return Err(anyhow::anyhow!(
            "No price data found for token: {}",
            base_token
        ));
    }

    // Save to cache
    save_history_data(quote_token, base_token, start_time, end_time, &values).await?;
    println!(
        "  💾 Cached data for {} ({} data points)",
        base_token,
        values.len()
    );

    Ok(values)
}

/// Fetch multiple tokens' price history with cache support
pub async fn fetch_multiple_price_history_with_cache(
    backend_client: &BackendClient,
    quote_token: &str,
    base_tokens: &[String],
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
) -> Result<std::collections::HashMap<String, Vec<ValueAtTime>>> {
    use std::collections::HashMap;

    let mut price_data = HashMap::new();

    println!(
        "📈 Fetching price data from {} to {}",
        start_time.format("%Y-%m-%d %H:%M"),
        end_time.format("%Y-%m-%d %H:%M")
    );

    for base_token in base_tokens {
        match fetch_price_history_with_cache(
            backend_client,
            quote_token,
            base_token,
            start_time,
            end_time,
        )
        .await
        {
            Ok(values) => {
                if !values.is_empty() {
                    price_data.insert(base_token.clone(), values);
                } else {
                    println!("  ⚠️ No price data found for {}", base_token);
                }
            }
            Err(e) => {
                println!("  ❌ Failed to fetch data for {}: {}", base_token, e);
                // Continue with other tokens instead of failing completely
            }
        }
    }

    if price_data.is_empty() {
        return Err(anyhow::anyhow!(
            "No price data available for any of the requested tokens: {:?}",
            base_tokens
        ));
    }

    println!(
        "✅ Successfully loaded data for {} tokens",
        price_data.len()
    );
    Ok(price_data)
}
