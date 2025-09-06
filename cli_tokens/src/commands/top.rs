use anyhow::Result;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use clap::Parser;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::time;
use tokio::time::Duration as TokioDuration;

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

    #[clap(long, help = "Minimum depth for filtering tokens [default: 1000000]")]
    pub min_depth: Option<u64>,
}

async fn start_timer(start_time: Instant, running: Arc<AtomicBool>) {
    let mut interval = time::interval(TokioDuration::from_secs(1));

    // 最初の tick をスキップして即座に0秒を表示しないようにする
    interval.tick().await;

    while running.load(Ordering::Relaxed) {
        interval.tick().await;
        if running.load(Ordering::Relaxed) {
            let elapsed = start_time.elapsed();
            let elapsed_mins = elapsed.as_secs() / 60;
            let elapsed_secs = elapsed.as_secs() % 60;
            eprint!("\r経過時間: {}:{:02}", elapsed_mins, elapsed_secs);
            use std::io::{self, Write};
            io::stderr().flush().unwrap();
        }
    }
}

pub async fn run(args: TopArgs) -> Result<()> {
    let start_time = Instant::now();
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    // バックグラウンドでタイマーを開始
    let timer_handle = tokio::spawn(start_timer(start_time, running_clone));

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

    // Get base directory from environment variable
    let base_dir = std::env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
    let output_dir = PathBuf::from(&base_dir).join(&args.output);

    // Ensure output directory exists
    ensure_directory_exists(&output_dir)?;

    // Fetch tokens
    let tokens = backend_client
        .get_volatility_tokens(
            start_date,
            end_date,
            args.limit,
            args.quote_token.clone(),
            args.min_depth,
        )
        .await?;

    // Save each token to individual file
    let quote_token = args.quote_token.as_deref().unwrap_or("wrap.near");
    for (i, token) in tokens.iter().enumerate() {
        println!("Saving {} ({}/{})", token.0, i + 1, tokens.len());

        // Get recent price data to calculate current price and 24h change
        let (current_price, price_change_24h, volume_24h, volatility_score) =
            get_current_price_data_with_volatility(
                &backend_client,
                &token.0,
                quote_token,
                start_date,
                end_date,
            )
            .await;

        let file_data = TokenFileData {
            metadata: FileMetadata {
                generated_at: Utc::now(),
                start_date: start_date.format("%Y-%m-%d").to_string(),
                end_date: end_date.format("%Y-%m-%d").to_string(),
                token: token.0.to_string(),
                quote_token: Some(quote_token.to_string()),
            },
            token_data: TokenVolatilityData {
                token: token.0.to_string(),
                volatility_score,
                price_data: PriceData {
                    current_price,
                    price_change_24h,
                    volume_24h,
                },
            },
        };

        // Create quote_token subdirectory
        let quote_dir = output_dir.join(sanitize_filename(quote_token));
        ensure_directory_exists(&quote_dir)?;

        let filename = format!("{}.json", sanitize_filename(&token.0));
        let file_path = quote_dir.join(filename);

        write_json_file(&file_path, &file_data).await?;
    }

    // タイマーを停止
    running.store(false, Ordering::Relaxed);
    timer_handle.abort();

    // 最終的な経過時間を標準エラー出力に表示してから改行
    let final_elapsed = start_time.elapsed();
    let final_mins = final_elapsed.as_secs() / 60;
    let final_secs = final_elapsed.as_secs() % 60;
    eprintln!("\r経過時間: {}:{:02}", final_mins, final_secs);

    println!(
        "Successfully saved {} tokens to {:?} - 総経過時間: {}:{:02}",
        tokens.len(),
        output_dir,
        final_mins,
        final_secs
    );
    Ok(())
}

pub fn parse_date(date_str: &str) -> Result<DateTime<Utc>> {
    let naive_date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")?;
    Ok(naive_date.and_hms_opt(0, 0, 0).unwrap().and_utc())
}

/// Get current price data for a token including 24h change calculation and volatility
async fn get_current_price_data_with_volatility(
    backend_client: &BackendClient,
    token: &str,
    quote_token: &str,
    start_date: DateTime<Utc>,
    end_date: DateTime<Utc>,
) -> (f64, f64, f64, f64) {
    match backend_client
        .get_price_history(
            token,
            quote_token,
            start_date.naive_utc(),
            end_date.naive_utc(),
        )
        .await
    {
        Ok(values) if !values.is_empty() => {
            let current_price = values.last().map(|v| v.value).unwrap_or(0.0);

            // Calculate 24h price change
            let price_24h_ago = if values.len() > 24 {
                values[values.len() - 24].value
            } else {
                values.first().map(|v| v.value).unwrap_or(current_price)
            };

            let price_change_24h = if price_24h_ago > 0.0 {
                ((current_price - price_24h_ago) / price_24h_ago) * 100.0
            } else {
                0.0
            };

            // Calculate volatility score from price data
            let volatility_score = calculate_volatility_score(&values);

            // Volume is not available from current API, set to 0
            let volume_24h = 0.0;

            (
                current_price,
                price_change_24h,
                volume_24h,
                volatility_score,
            )
        }
        _ => {
            // Fallback to defaults if no data available
            (0.0, 0.0, 0.0, 0.0)
        }
    }
}

/// Calculate volatility score from price data using standard deviation of returns
pub fn calculate_volatility_score(values: &[common::stats::ValueAtTime]) -> f64 {
    common::algorithm::calculate_volatility_score(values, true)
}

#[cfg(test)]
mod tests;
