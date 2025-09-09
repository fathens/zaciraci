use crate::api::backend::BackendClient;
use crate::models::history::{HistoryFileData, HistoryMetadata, PriceHistory};
use crate::models::token::TokenFileData;
use crate::utils::file::sanitize_filename;
use anyhow::{Context, Result};
use chrono::{NaiveDate, Utc};
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
    #[arg(short, long, default_value = "history")]
    pub output: PathBuf,

    /// 既存の履歴データを強制上書き
    #[arg(long)]
    pub force: bool,
}

pub async fn run_history(args: HistoryArgs) -> Result<()> {
    // トークンファイルを読み込み
    let token_data = load_token_file(&args.token_file).await?;

    // Get base directory from environment variable
    let base_dir = std::env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
    let output_dir = std::path::PathBuf::from(&base_dir).join(&args.output);

    // 出力ディレクトリを作成
    fs::create_dir_all(&output_dir)
        .await
        .context("Failed to create output directory")?;

    // quote_token サブディレクトリを作成
    let quote_dir = output_dir.join(sanitize_filename(&args.quote_token));
    fs::create_dir_all(&quote_dir)
        .await
        .context("Failed to create quote token subdirectory")?;

    // 出力ファイルパスを生成 (${quote_token}/${base_token}.json)
    let filename = format!("{}.json", sanitize_filename(&token_data.token));
    let output_file = quote_dir.join(filename);

    // 既存ファイルのチェック
    if output_file.exists() && !args.force {
        return Err(anyhow::anyhow!(
            "History file already exists: {}. Use --force to overwrite",
            output_file.display()
        ));
    }

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

    println!(
        "Fetching price history for {} from {} to {}",
        token_data.token, token_data.metadata.start_date, token_data.metadata.end_date
    );

    // 価格履歴を取得
    let values = match client
        .get_price_history(&args.quote_token, &token_data.token, start_date, end_date)
        .await
    {
        Ok(values) => values,
        Err(e) => {
            eprintln!("API Error details: {}", e);
            return Err(e.context(format!(
                "Failed to fetch price history for {} (quote: {}) from {} to {}",
                token_data.token, args.quote_token, start_date, end_date
            )));
        }
    };

    // 履歴データを作成
    let history_data = HistoryFileData {
        metadata: HistoryMetadata {
            generated_at: Utc::now(),
            start_date: token_data.metadata.start_date.clone(),
            end_date: token_data.metadata.end_date.clone(),
            base_token: token_data.token.clone(),
            quote_token: args.quote_token.clone(),
        },
        price_history: PriceHistory { values },
    };

    // ファイルに保存
    let json_content =
        serde_json::to_string_pretty(&history_data).context("Failed to serialize history data")?;

    fs::write(&output_file, json_content)
        .await
        .context("Failed to write history file")?;

    println!("History saved to: {}", output_file.display());
    println!("Data points: {}", history_data.price_history.values.len());

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
