use anyhow::Result;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;

use crate::api::backend::BackendClient;
use crate::models::token::{FileMetadata, PriceData, TokenFileData, TokenVolatilityData};
use crate::utils::{
    config::Config,
    file::{ensure_directory_exists, sanitize_filename, write_json_file},
};

#[derive(Parser)]
#[clap(about = "Get high volatility tokens and save to individual files")]
pub struct TopArgs {
    #[clap(short, long, help = "Start date (YYYY-MM-DD format)")]
    pub start: Option<String>,

    #[clap(short, long, help = "End date (YYYY-MM-DD format)")]
    pub end: Option<String>,

    #[clap(short, long, default_value = "10", help = "Number of tokens to fetch")]
    pub limit: u32,

    #[clap(short, long, default_value = "tokens", help = "Output directory")]
    pub output: PathBuf,

    #[clap(short, long, default_value = "json", help = "Output format (json|csv)")]
    pub format: String,

    #[clap(
        long,
        help = "Quote token for volatility calculation [default: wrap.near]"
    )]
    pub quote_token: Option<String>,
}

pub async fn run(args: TopArgs) -> Result<()> {
    let config = Config::from_env();
    let backend_client = BackendClient::new_with_url(config.backend_url);

    // Parse dates
    let end_date = if let Some(end_str) = args.end {
        parse_date(&end_str)?
    } else {
        Utc::now()
    };

    let start_date = if let Some(start_str) = args.start {
        parse_date(&start_str)?
    } else {
        end_date - Duration::days(30)
    };

    println!(
        "Fetching volatility tokens from {} to {}",
        start_date.format("%Y-%m-%d"),
        end_date.format("%Y-%m-%d")
    );

    // Ensure output directory exists
    ensure_directory_exists(&args.output)?;

    // Show progress
    let pb = ProgressBar::new(args.limit as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("#>-"),
    );

    // Fetch tokens
    pb.set_message("Fetching volatility tokens...");
    let tokens = backend_client
        .get_volatility_tokens(start_date, end_date, args.limit, args.quote_token.clone())
        .await?;

    // Save each token to individual file
    let quote_token = args.quote_token.as_deref().unwrap_or("wrap.near");
    for (i, token) in tokens.iter().enumerate() {
        pb.set_position((i + 1) as u64);
        pb.set_message(format!("Saving {}", token.0));

        let file_data = TokenFileData {
            metadata: FileMetadata {
                generated_at: Utc::now(),
                start_date: start_date.format("%Y-%m-%d").to_string(),
                end_date: end_date.format("%Y-%m-%d").to_string(),
                token: token.0.to_string(),
            },
            token_data: TokenVolatilityData {
                token: token.0.to_string(),
                volatility_score: 0.85, // TODO: Calculate actual volatility
                price_data: PriceData {
                    current_price: 0.0, // TODO: Get actual price data
                    price_change_24h: 0.0,
                    volume_24h: 0.0,
                },
            },
        };

        // Create quote_token subdirectory
        let quote_dir = args.output.join(sanitize_filename(quote_token));
        ensure_directory_exists(&quote_dir)?;

        let filename = format!("{}.json", sanitize_filename(&token.0));
        let file_path = quote_dir.join(filename);

        write_json_file(&file_path, &file_data).await?;
    }

    pb.finish_with_message(format!(
        "Successfully saved {} tokens to {:?}",
        tokens.len(),
        args.output
    ));
    Ok(())
}

pub fn parse_date(date_str: &str) -> Result<DateTime<Utc>> {
    let naive_date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")?;
    Ok(naive_date.and_hms_opt(0, 0, 0).unwrap().and_utc())
}
