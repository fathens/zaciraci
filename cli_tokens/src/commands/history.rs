use crate::api::backend::BackendClient;
use crate::models::token::TokenFileData;
use crate::utils::cache::fetch_price_history_with_cache;
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use clap::Args;
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug, Args)]
pub struct HistoryArgs {
    /// トークンファイルパス
    pub token_file: PathBuf,

    /// 見積りトークン（価格表示の基準）
    #[arg(long, default_value = "wrap.near")]
    pub quote_token: String,

    /// 出力ディレクトリ
    #[arg(short, long, default_value = "price_history")]
    pub output: PathBuf,
}

pub async fn run_history(args: HistoryArgs) -> Result<()> {
    // トークンファイルを読み込み
    let token_data = load_token_file(&args.token_file).await?;

    // APIクライアントを作成
    let client = BackendClient::new();

    // トークンメタデータから期間を取得
    let start_date = NaiveDate::parse_from_str(&token_data.metadata.start_date, "%Y-%m-%d")
        .context("Invalid start_date format")?
        .and_hms_opt(0, 0, 0)
        .context("Failed to create start datetime")?;

    let end_date = NaiveDate::parse_from_str(&token_data.metadata.end_date, "%Y-%m-%d")
        .context("Invalid end_date format")?
        .and_hms_opt(23, 59, 59)
        .context("Failed to create end datetime")?;

    let start_datetime = DateTime::<Utc>::from_naive_utc_and_offset(start_date, Utc);
    let end_datetime = DateTime::<Utc>::from_naive_utc_and_offset(end_date, Utc);

    println!(
        "Fetching price history for {} from {} to {}",
        token_data.token, token_data.metadata.start_date, token_data.metadata.end_date
    );

    // キャッシュを使用して価格履歴を取得
    let values = fetch_price_history_with_cache(
        &client,
        &args.quote_token,
        &token_data.token,
        start_datetime,
        end_datetime,
    )
    .await
    .context(format!(
        "Failed to fetch price history for {} (quote: {})",
        token_data.token, args.quote_token
    ))?;

    println!("History retrieved successfully!");
    println!("Data points: {}", values.len());

    Ok(())
}

async fn load_token_file(path: &PathBuf) -> Result<TokenFileData> {
    let content = fs::read_to_string(path)
        .await
        .with_context(|| format!("Failed to read token file: {}", path.display()))?;

    let token_data: TokenFileData = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse token file: {}", path.display()))?;

    Ok(token_data)
}
