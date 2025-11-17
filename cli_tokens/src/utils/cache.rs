use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

pub mod predictions;
pub mod price_history;

// Re-export commonly used functions for backward compatibility
pub use price_history::{
    fetch_multiple_price_history_with_cache, fetch_price_history_with_cache,
    find_latest_history_file, get_price_history_dir,
};

// Re-export prediction cache functions
pub use predictions::{
    PredictionCacheParams, check_prediction_cache, create_prediction_filename,
    find_latest_prediction_file, get_prediction_dir, load_prediction_data, save_prediction_result,
};

// Common cache utilities

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
