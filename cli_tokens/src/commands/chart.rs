use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::{Args, ValueEnum};
use plotters::coord::types::RangedCoordf64;
use plotters::prelude::*;
use std::path::{Path, PathBuf};
use tokio::fs;

use crate::models::history::HistoryFileData;
use crate::models::prediction::{PredictionPoint, TokenPredictionResult};
use crate::models::token::TokenFileData;
use crate::utils::file::sanitize_filename;

#[derive(Debug, Args)]
pub struct ChartArgs {
    /// トークンファイルパス（起点）
    pub token_file: PathBuf,

    /// 出力ディレクトリ
    #[arg(short, long, default_value = "charts")]
    pub output: PathBuf,

    /// ベースディレクトリ（CLI_TOKENS_BASE_DIRをオーバーライド）
    #[arg(long)]
    pub base_dir: Option<PathBuf>,

    /// 画像サイズ (WIDTHxHEIGHT)
    #[arg(long, default_value = "1200x800")]
    pub size: String,

    /// チャートタイプ
    #[arg(long, value_enum, default_value = "auto")]
    pub chart_type: ChartType,

    /// 予測データの検索と描画を無効化
    #[arg(long)]
    pub history_only: bool,

    /// 信頼区間を表示（予測データがある場合）
    #[arg(long)]
    pub show_confidence: bool,

    /// 出力ファイル名を明示指定
    #[arg(long)]
    pub output_name: Option<String>,

    /// 既存ファイルを強制上書き
    #[arg(long)]
    pub force: bool,

    /// 詳細ログ出力
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Clone, ValueEnum, Debug)]
pub enum ChartType {
    /// 自動検出（履歴+予測があれば両方、なければ履歴のみ）
    Auto,
    /// 履歴データのみ
    History,
    /// 予測データのみ（履歴は背景として薄く表示）
    Prediction,
    /// 履歴と予測を重ねて表示
    Combined,
}

#[derive(Debug)]
pub struct DetectedFiles {
    pub history: Option<PathBuf>,
    pub prediction: Option<PathBuf>,
    pub token_name: String,
    pub quote_token: String,
}

#[derive(Debug)]
pub struct ChartData {
    pub history: Option<Vec<(DateTime<Utc>, f64)>>,
    pub predictions: Option<Vec<PredictionPoint>>,
    pub token_name: String,
    pub quote_token: String,
    pub time_range: Option<(DateTime<Utc>, DateTime<Utc>)>,
}

#[derive(Debug, thiserror::Error)]
pub enum ChartError {
    #[error("No data found for token: {0}")]
    NoDataFound(String),
    #[error("History file not found: {0}")]
    HistoryNotFound(PathBuf),
    #[error("Prediction file not found: {0}")]
    PredictionNotFound(PathBuf),
    #[error("Invalid token file: {0}")]
    InvalidTokenFile(PathBuf),
    #[error("Chart generation error: {0}")]
    ChartGeneration(String),
    #[error("Output error: {0}")]
    OutputError(String),
    #[error("Invalid size format: {0}. Expected format: WIDTHxHEIGHT")]
    InvalidSizeFormat(String),
}

pub async fn run_chart(args: ChartArgs) -> Result<()> {
    if args.verbose {
        println!(
            "🔍 Starting chart generation for: {}",
            args.token_file.display()
        );
    }

    // Validate token file
    if !args.token_file.exists() {
        return Err(ChartError::InvalidTokenFile(args.token_file.clone()).into());
    }

    // Parse size
    let (width, height) = parse_size(&args.size)?;

    // Detect data files
    let detected = detect_data_files(&args.token_file, args.base_dir.as_deref()).await?;

    if args.verbose {
        println!("📁 Detected files:");
        println!("   History: {:?}", detected.history);
        println!("   Prediction: {:?}", detected.prediction);
    }

    // Validate based on chart type
    validate_chart_args(&args, &detected)?;

    // Load data
    let chart_data = load_chart_data(&detected, &args).await?;

    if args.verbose {
        println!("📊 Loaded data:");
        if let Some(ref history) = chart_data.history {
            println!("   History points: {}", history.len());
        }
        if let Some(ref predictions) = chart_data.predictions {
            println!("   Prediction points: {}", predictions.len());
        }
    }

    // Generate output path
    let output_path = generate_output_path(&args, &detected)?;

    if args.verbose {
        println!("📤 Output path: {}", output_path.display());
    }

    // Check if file exists and handle force flag
    if output_path.exists() && !args.force {
        return Err(ChartError::OutputError(format!(
            "Chart file already exists: {}. Use --force to overwrite",
            output_path.display()
        ))
        .into());
    }

    // Create output directory
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .await
            .context("Failed to create output directory")?;
    }

    // Generate chart
    generate_chart(&chart_data, &output_path, width, height, &args)?;

    println!("✅ Chart generated: {}", output_path.display());

    Ok(())
}

async fn detect_data_files(token_file: &Path, base_dir: Option<&Path>) -> Result<DetectedFiles> {
    // Load token file to extract information
    let token_data = load_token_file(token_file).await?;

    // Extract quote token from path or token data
    let quote_token = extract_quote_token_from_path(token_file)
        .or_else(|| token_data.metadata.quote_token.clone())
        .unwrap_or_else(|| "wrap.near".to_string());

    let base_dir = base_dir
        .map(|p| p.to_path_buf())
        .or_else(|| std::env::var("CLI_TOKENS_BASE_DIR").ok().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));

    let token_name = sanitize_filename(&token_data.token_data.token);
    let quote_dir = sanitize_filename(&quote_token);

    // Search for history file
    let history_file = base_dir
        .join("history")
        .join(&quote_dir)
        .join(format!("{}.json", token_name));

    // Search for prediction file
    let prediction_file = base_dir
        .join("predictions")
        .join(&quote_dir)
        .join(format!("{}.json", token_name));

    Ok(DetectedFiles {
        history: if history_file.exists() {
            Some(history_file)
        } else {
            None
        },
        prediction: if prediction_file.exists() {
            Some(prediction_file)
        } else {
            None
        },
        token_name,
        quote_token,
    })
}

fn extract_quote_token_from_path(token_file: &Path) -> Option<String> {
    token_file
        .parent()?
        .file_name()?
        .to_str()
        .map(|s| s.to_string())
        .filter(|s| s != "tokens")
}

async fn load_token_file(path: &Path) -> Result<TokenFileData> {
    let content = fs::read_to_string(path)
        .await
        .with_context(|| format!("Failed to read token file: {}", path.display()))?;

    let token_data: TokenFileData = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse token file: {}", path.display()))?;

    Ok(token_data)
}

fn validate_chart_args(args: &ChartArgs, detected: &DetectedFiles) -> Result<()> {
    match args.chart_type {
        ChartType::History => {
            if detected.history.is_none() {
                return Err(ChartError::HistoryNotFound(PathBuf::from(format!(
                    "Expected history file for token: {}",
                    detected.token_name
                )))
                .into());
            }
        }
        ChartType::Prediction => {
            if detected.prediction.is_none() {
                return Err(ChartError::PredictionNotFound(PathBuf::from(format!(
                    "Expected prediction file for token: {}",
                    detected.token_name
                )))
                .into());
            }
        }
        ChartType::Auto | ChartType::Combined => {
            if detected.history.is_none() && detected.prediction.is_none() {
                return Err(ChartError::NoDataFound(detected.token_name.clone()).into());
            }
        }
    }

    // If history_only is set, we must have history data
    if args.history_only && detected.history.is_none() {
        return Err(ChartError::HistoryNotFound(PathBuf::from(format!(
            "History data required but not found for token: {}",
            detected.token_name
        )))
        .into());
    }

    Ok(())
}

async fn load_chart_data(detected: &DetectedFiles, args: &ChartArgs) -> Result<ChartData> {
    let mut chart_data = ChartData {
        history: None,
        predictions: None,
        token_name: detected.token_name.clone(),
        quote_token: detected.quote_token.clone(),
        time_range: None,
    };

    // Load history data if available and needed
    if let Some(ref history_file) = detected.history {
        if !args.history_only
            || matches!(
                args.chart_type,
                ChartType::History | ChartType::Auto | ChartType::Combined
            )
        {
            chart_data.history = Some(load_history_data(history_file).await?);
        }
    }

    // Load prediction data if available and needed
    if let Some(ref prediction_file) = detected.prediction {
        if !args.history_only
            && matches!(
                args.chart_type,
                ChartType::Prediction | ChartType::Auto | ChartType::Combined
            )
        {
            chart_data.predictions = Some(load_prediction_data(prediction_file).await?);
        }
    }

    // Calculate time range
    chart_data.time_range = calculate_time_range(&chart_data);

    Ok(chart_data)
}

async fn load_history_data(history_file: &Path) -> Result<Vec<(DateTime<Utc>, f64)>> {
    let content = fs::read_to_string(history_file)
        .await
        .with_context(|| format!("Failed to read history file: {}", history_file.display()))?;

    let history_data: HistoryFileData = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse history file: {}", history_file.display()))?;

    if history_data.price_history.values.is_empty() {
        return Err(anyhow::anyhow!("No price data found in history file"));
    }

    let data: Vec<(DateTime<Utc>, f64)> = history_data
        .price_history
        .values
        .iter()
        .map(|v| (DateTime::from_naive_utc_and_offset(v.time, Utc), v.value))
        .collect();

    Ok(data)
}

async fn load_prediction_data(prediction_file: &Path) -> Result<Vec<PredictionPoint>> {
    let content = fs::read_to_string(prediction_file).await.with_context(|| {
        format!(
            "Failed to read prediction file: {}",
            prediction_file.display()
        )
    })?;

    let prediction_data: TokenPredictionResult =
        serde_json::from_str(&content).with_context(|| {
            format!(
                "Failed to parse prediction file: {}",
                prediction_file.display()
            )
        })?;

    if prediction_data.predicted_values.is_empty() {
        return Err(anyhow::anyhow!(
            "No prediction data found in prediction file"
        ));
    }

    Ok(prediction_data.predicted_values)
}

fn calculate_time_range(chart_data: &ChartData) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let mut min_time: Option<DateTime<Utc>> = None;
    let mut max_time: Option<DateTime<Utc>> = None;

    // Check history data
    if let Some(ref history) = chart_data.history {
        for (timestamp, _) in history {
            min_time = Some(min_time.map_or(*timestamp, |t| t.min(*timestamp)));
            max_time = Some(max_time.map_or(*timestamp, |t| t.max(*timestamp)));
        }
    }

    // Check prediction data
    if let Some(ref predictions) = chart_data.predictions {
        for point in predictions {
            min_time = Some(min_time.map_or(point.timestamp, |t| t.min(point.timestamp)));
            max_time = Some(max_time.map_or(point.timestamp, |t| t.max(point.timestamp)));
        }
    }

    min_time.and_then(|min| max_time.map(|max| (min, max)))
}

fn generate_output_path(args: &ChartArgs, detected: &DetectedFiles) -> Result<PathBuf> {
    let base_dir = args
        .base_dir
        .as_ref()
        .cloned()
        .or_else(|| std::env::var("CLI_TOKENS_BASE_DIR").ok().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));

    let output_dir = base_dir
        .join(&args.output)
        .join(sanitize_filename(&detected.quote_token));

    let filename = if let Some(ref custom_name) = args.output_name {
        format!("{}.png", custom_name)
    } else {
        generate_output_filename(detected, &args.chart_type, args.show_confidence)
    };

    Ok(output_dir.join(filename))
}

fn generate_output_filename(
    detected: &DetectedFiles,
    chart_type: &ChartType,
    show_confidence: bool,
) -> String {
    let base_name = &detected.token_name;

    match chart_type {
        ChartType::History => format!("{}_history.png", base_name),
        ChartType::Prediction => {
            if show_confidence {
                format!("{}_prediction_with_confidence.png", base_name)
            } else {
                format!("{}_prediction.png", base_name)
            }
        }
        ChartType::Combined => {
            if show_confidence {
                format!("{}_combined_with_confidence.png", base_name)
            } else {
                format!("{}_combined.png", base_name)
            }
        }
        ChartType::Auto => {
            let has_history = detected.history.is_some();
            let has_prediction = detected.prediction.is_some();

            match (has_history, has_prediction) {
                (true, true) => {
                    if show_confidence {
                        format!("{}_combined_with_confidence.png", base_name)
                    } else {
                        format!("{}_combined.png", base_name)
                    }
                }
                (true, false) => format!("{}_history.png", base_name),
                (false, true) => format!("{}_prediction.png", base_name),
                (false, false) => unreachable!(), // Should be caught by validation
            }
        }
    }
}

fn parse_size(size_str: &str) -> Result<(u32, u32)> {
    let parts: Vec<&str> = size_str.split('x').collect();
    if parts.len() != 2 {
        return Err(ChartError::InvalidSizeFormat(size_str.to_string()).into());
    }

    let width = parts[0]
        .parse::<u32>()
        .map_err(|_| ChartError::InvalidSizeFormat(size_str.to_string()))?;
    let height = parts[1]
        .parse::<u32>()
        .map_err(|_| ChartError::InvalidSizeFormat(size_str.to_string()))?;

    if width == 0 || height == 0 {
        return Err(ChartError::InvalidSizeFormat(size_str.to_string()).into());
    }

    Ok((width, height))
}

fn generate_chart(
    chart_data: &ChartData,
    output_path: &Path,
    width: u32,
    height: u32,
    args: &ChartArgs,
) -> Result<()> {
    let root = BitMapBackend::new(output_path, (width, height)).into_drawing_area();

    root.fill(&WHITE)
        .map_err(|e| ChartError::ChartGeneration(format!("Failed to fill background: {}", e)))?;

    // Calculate value range
    let (min_value, max_value) = calculate_value_range(chart_data)?;
    let (start_time, end_time) = chart_data
        .time_range
        .ok_or_else(|| ChartError::ChartGeneration("No time range available".to_string()))?;

    // Create chart title
    let title = format!(
        "{} / {} Price Chart",
        chart_data.token_name, chart_data.quote_token
    );

    let mut chart = ChartBuilder::on(&root)
        .caption(&title, ("Arial", 30).into_font())
        .margin(20)
        .x_label_area_size(60)
        .y_label_area_size(80)
        .build_cartesian_2d(start_time..end_time, min_value..max_value)
        .map_err(|e| ChartError::ChartGeneration(format!("Failed to build chart: {}", e)))?;

    chart
        .configure_mesh()
        .x_desc("Time")
        .y_desc(format!("Price ({})", chart_data.quote_token))
        .x_label_formatter(&|x| x.format("%m-%d").to_string())
        .draw()
        .map_err(|e| ChartError::ChartGeneration(format!("Failed to configure mesh: {}", e)))?;

    // Draw history data
    if let Some(ref history) = chart_data.history {
        chart
            .draw_series(LineSeries::new(
                history.iter().map(|(time, value)| (*time, *value)),
                &BLUE,
            ))
            .map_err(|e| {
                ChartError::ChartGeneration(format!("Failed to draw history series: {}", e))
            })?
            .label("History")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 10, y)], BLUE));
    }

    // Draw prediction data
    if let Some(ref predictions) = chart_data.predictions {
        chart
            .draw_series(LineSeries::new(
                predictions.iter().map(|p| (p.timestamp, p.value)),
                &RED,
            ))
            .map_err(|e| {
                ChartError::ChartGeneration(format!("Failed to draw prediction series: {}", e))
            })?
            .label("Prediction")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 10, y)], RED));

        // Draw confidence intervals if requested
        if args.show_confidence {
            draw_confidence_intervals(&mut chart, predictions)?;
        }
    }

    chart
        .configure_series_labels()
        .position(SeriesLabelPosition::UpperLeft)
        .margin(10)
        .background_style(WHITE.mix(0.8))
        .border_style(BLACK)
        .draw()
        .map_err(|e| ChartError::ChartGeneration(format!("Failed to draw legend: {}", e)))?;

    root.present()
        .map_err(|e| ChartError::ChartGeneration(format!("Failed to present chart: {}", e)))?;

    Ok(())
}

fn calculate_value_range(chart_data: &ChartData) -> Result<(f64, f64)> {
    let mut min_value = f64::INFINITY;
    let mut max_value = f64::NEG_INFINITY;

    // Check history data
    if let Some(ref history) = chart_data.history {
        for (_, value) in history {
            min_value = min_value.min(*value);
            max_value = max_value.max(*value);
        }
    }

    // Check prediction data
    if let Some(ref predictions) = chart_data.predictions {
        for point in predictions {
            min_value = min_value.min(point.value);
            max_value = max_value.max(point.value);

            // Include confidence intervals in range calculation
            if let Some(ref ci) = point.confidence_interval {
                min_value = min_value.min(ci.lower);
                max_value = max_value.max(ci.upper);
            }
        }
    }

    if min_value == f64::INFINITY || max_value == f64::NEG_INFINITY {
        return Err(ChartError::ChartGeneration("No valid data points found".to_string()).into());
    }

    // Add some padding
    let range = max_value - min_value;
    let padding = range * 0.1;

    Ok((min_value - padding, max_value + padding))
}

fn draw_confidence_intervals<DB: DrawingBackend>(
    chart: &mut ChartContext<DB, Cartesian2d<RangedDateTime<DateTime<Utc>>, RangedCoordf64>>,
    predictions: &[PredictionPoint],
) -> Result<()> {
    for point in predictions {
        if let Some(ref ci) = point.confidence_interval {
            // Draw confidence interval as a vertical line
            chart
                .draw_series(std::iter::once(Rectangle::new(
                    [(point.timestamp, ci.lower), (point.timestamp, ci.upper)],
                    RGBColor(128, 128, 128).mix(0.3).filled(),
                )))
                .map_err(|e| {
                    ChartError::ChartGeneration(format!(
                        "Failed to draw confidence interval: {}",
                        e
                    ))
                })?;
        }
    }
    Ok(())
}
