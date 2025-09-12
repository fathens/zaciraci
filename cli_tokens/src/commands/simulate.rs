pub mod algorithms;
pub mod data;
pub mod metrics;
pub mod trading;
pub mod types;
pub mod utils;

// Re-export all types for backward compatibility
pub use types::*;
// Re-export utilities for backward compatibility
pub use utils::*;

use algorithms::{
    run_momentum_simulation, run_portfolio_simulation, run_trend_following_simulation,
};
use anyhow::{Context, Result};
use bigdecimal::{BigDecimal, FromPrimitive};
use chrono::{DateTime, Duration, Utc};
use serde_json;
use std::fs;
use std::path::{Path, PathBuf};

/// Main entry point for simulation execution
pub async fn run(args: SimulateArgs) -> Result<()> {
    println!("🚀 Starting simulation...");

    let verbose = args.verbose;

    if verbose {
        println!("📋 Configuration:");
        println!("  Algorithm: All algorithms (Momentum, Portfolio, TrendFollowing)");
        println!("  Capital: {} {}", args.capital, args.quote_token);
        println!("  Fee Model: {}", args.fee_model);
        println!("  Output: {}", args.output);
    }

    // outputを先に保存
    let output_dir = args.output.clone();

    // 1. 設定の検証と変換
    let config = validate_and_convert_args(args).await?;

    // topコマンドの出力ディレクトリからトークンを読み取り
    let mut final_config = config;

    if verbose {
        println!("🔍 Loading tokens from top command output directory...");
    }

    let tokens_dir = get_tokens_directory()?;
    let token_names = load_tokens_from_directory(&tokens_dir, &final_config.quote_token)?;

    if token_names.is_empty() {
        return Err(anyhow::anyhow!(
            "No tokens found in directory: {}. Please run 'cli_tokens top' first to generate token files.",
            tokens_dir.display()
        ));
    }

    println!("📈 Found {} tokens", token_names.len());
    if verbose {
        println!("  Tokens: {}", token_names.join(", "));
    }

    final_config.target_tokens = token_names;

    // 全アルゴリズムを実行
    println!("\n🔄 Running algorithms...");

    let algorithms = [
        AlgorithmType::Momentum,
        AlgorithmType::Portfolio,
        AlgorithmType::TrendFollowing,
    ];
    let mut results = Vec::new();

    for algorithm in &algorithms {
        let mut config_copy = final_config.clone();
        config_copy.algorithm = algorithm.clone();

        println!("Running {:?}...", algorithm);
        let result = run_single_algorithm(&config_copy, verbose).await?;
        results.push(result);
    }

    // 複数アルゴリズムの結果を保存
    save_simple_multi_algorithm_result(&results, &output_dir)?;

    Ok(())
}

/// Get the tokens directory from environment or default location
fn get_tokens_directory() -> Result<PathBuf> {
    let base_dir = std::env::var("CLI_TOKENS_BASE_DIR").unwrap_or_else(|_| ".".to_string());
    Ok(PathBuf::from(base_dir).join("tokens"))
}

/// Load token names from the tokens directory created by top command
fn load_tokens_from_directory(tokens_dir: &Path, quote_token: &str) -> Result<Vec<String>> {
    let quote_dir = tokens_dir.join(quote_token);

    if !quote_dir.exists() {
        return Err(anyhow::anyhow!(
            "Quote token directory '{}' not found in tokens directory. Please run 'cli_tokens top --quote-token {}' first.",
            quote_token,
            quote_token
        ));
    }

    let mut token_names = Vec::new();

    for entry in fs::read_dir(&quote_dir)
        .with_context(|| format!("Failed to read directory: {}", quote_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
            if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                // Verify the file is a valid token file by checking its content
                match fs::read_to_string(&path) {
                    Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                        Ok(json) => {
                            if json.get("token").is_some() && json.get("metadata").is_some() {
                                token_names.push(file_stem.to_string());
                            }
                        }
                        Err(_) => {
                            eprintln!("Warning: Skipping invalid JSON file: {}", path.display());
                        }
                    },
                    Err(_) => {
                        eprintln!("Warning: Could not read file: {}", path.display());
                    }
                }
            }
        }
    }

    token_names.sort(); // Sort for consistent ordering
    Ok(token_names)
}

/// Convert and validate simulation arguments
pub async fn validate_and_convert_args(args: SimulateArgs) -> Result<SimulationConfig> {
    // 日付の解析と検証
    let start_date = if let Some(start_str) = args.start {
        // Try multiple date formats
        if let Ok(date) = start_str.parse::<DateTime<Utc>>() {
            date
        } else {
            // Try parsing as date only (YYYY-MM-DD) and add time
            let date_str = if start_str.len() == 10 {
                format!("{}T00:00:00Z", start_str)
            } else {
                start_str.clone()
            };
            date_str.parse::<DateTime<Utc>>().map_err(|e| {
                anyhow::anyhow!(
                    "Invalid start date format '{}': {}. Use YYYY-MM-DD or ISO 8601 format",
                    start_str,
                    e
                )
            })?
        }
    } else {
        // デフォルト: 30日前から
        Utc::now() - Duration::days(30)
    };

    let end_date = if let Some(end_str) = args.end {
        // Try multiple date formats
        if let Ok(date) = end_str.parse::<DateTime<Utc>>() {
            date
        } else {
            // Try parsing as date only (YYYY-MM-DD) and add time
            let date_str = if end_str.len() == 10 {
                format!("{}T23:59:59Z", end_str)
            } else {
                end_str.clone()
            };
            date_str.parse::<DateTime<Utc>>().map_err(|e| {
                anyhow::anyhow!(
                    "Invalid end date format '{}': {}. Use YYYY-MM-DD or ISO 8601 format",
                    end_str,
                    e
                )
            })?
        }
    } else {
        // デフォルト: 現在時刻
        Utc::now()
    };

    // 期間の妥当性チェック
    if end_date <= start_date {
        return Err(anyhow::anyhow!(
            "End date must be after start date. Start: {}, End: {}",
            start_date,
            end_date
        ));
    }

    let duration = end_date - start_date;
    if duration < Duration::hours(1) {
        return Err(anyhow::anyhow!(
            "Simulation period too short. Minimum 1 hour required."
        ));
    }

    // アルゴリズムタイプはデフォルト値（後で各アルゴリズムごとに設定される）
    let algorithm = AlgorithmType::Momentum;

    // トークンリストは後で自動取得するため、ここでは空のベクターを設定
    let target_tokens = Vec::new();

    // 各種設定の変換
    let initial_capital = BigDecimal::from_f64(args.capital)
        .ok_or_else(|| anyhow::anyhow!("Invalid capital amount: {}", args.capital))?;

    let rebalance_interval = RebalanceInterval::parse(&args.rebalance_interval)?;
    let fee_model = FeeModel::from((args.fee_model.as_str(), args.custom_fee));
    let gas_cost = BigDecimal::from_f64(args.gas_cost)
        .ok_or_else(|| anyhow::anyhow!("Invalid gas cost: {}", args.gas_cost))?;
    let min_trade_amount = BigDecimal::from_f64(args.min_trade)
        .ok_or_else(|| anyhow::anyhow!("Invalid min trade amount: {}", args.min_trade))?;

    let prediction_horizon = Duration::hours(args.prediction_horizon as i64);

    Ok(SimulationConfig {
        start_date,
        end_date,
        algorithm,
        initial_capital,
        quote_token: args.quote_token,
        target_tokens,
        rebalance_interval,
        fee_model,
        slippage_rate: args.slippage,
        gas_cost,
        min_trade_amount,
        prediction_horizon,
        historical_days: args.historical_days as i64,
        model: args.model,
    })
}

/// Run a single algorithm simulation
async fn run_single_algorithm(
    config: &SimulationConfig,
    verbose: bool,
) -> Result<SimulationResult> {
    match config.algorithm {
        AlgorithmType::Momentum => run_momentum_simulation(config, verbose).await,
        AlgorithmType::Portfolio => run_portfolio_simulation(config, verbose).await,
        AlgorithmType::TrendFollowing => run_trend_following_simulation(config, verbose).await,
    }
}

/// Save simulation result to output directory
pub fn save_simulation_result(result: &SimulationResult, output_dir: &str) -> Result<()> {
    use std::fs;
    use std::path::Path;

    // 出力ディレクトリを作成
    fs::create_dir_all(output_dir)
        .with_context(|| format!("Failed to create output directory: {}", output_dir))?;

    // ファイル名を生成（アルゴリズム名 + タイムスタンプ）
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let filename = format!("{:?}_{}.json", result.config.algorithm, timestamp).to_lowercase();
    let filepath = Path::new(output_dir).join(&filename);

    // JSON形式でシミュレーション結果を保存
    let json_content = serde_json::to_string_pretty(result)
        .context("Failed to serialize simulation result to JSON")?;

    fs::write(&filepath, json_content).with_context(|| {
        format!(
            "Failed to write simulation result to {}",
            filepath.display()
        )
    })?;

    println!("💾 Simulation completed successfully!");
    println!("  Algorithm: {:?}", result.config.algorithm);
    println!("  Duration: {} days", result.config.duration_days);
    println!("  Initial Capital: ${:.2}", result.config.initial_capital);
    println!("  Final Value: ${:.2}", result.config.final_value);
    println!(
        "  Total Return: {:.2}%",
        result.performance.total_return_pct
    );

    if result.performance.total_trades > 0 {
        println!("  Trades: {}", result.performance.total_trades);
        println!("  Win Rate: {:.1}%", result.performance.win_rate);
    }

    println!("📁 Results saved to: {}", filepath.display());
    println!(
        "💡 Generate HTML report with: cli_tokens report {}",
        filepath.display()
    );

    Ok(())
}

/// Save multi-algorithm simulation results
pub fn save_simple_multi_algorithm_result(
    results: &[SimulationResult],
    output_dir: &str,
) -> Result<()> {
    use anyhow::Context;
    use std::fs;
    use std::path::Path;

    println!("\n🏆 Multi-Algorithm Comparison Results:");
    println!("┌─────────────────┬─────────────┬─────────────┬─────────────┬─────────────┐");
    println!("│ Algorithm       │ Return (%)  │ Sharpe      │ Drawdown(%) │ Trades      │");
    println!("├─────────────────┼─────────────┼─────────────┼─────────────┼─────────────┤");

    let mut best_return = (AlgorithmType::Momentum, f64::NEG_INFINITY);
    let mut best_sharpe = (AlgorithmType::Momentum, f64::NEG_INFINITY);
    let mut lowest_drawdown = (AlgorithmType::Momentum, f64::INFINITY);

    for result in results {
        let algo_name = format!("{:?}", result.config.algorithm);
        let return_pct = result.performance.total_return_pct;
        let sharpe = result.performance.sharpe_ratio;
        let drawdown = result.performance.max_drawdown_pct;
        let trades = result.performance.total_trades;

        println!(
            "│ {:<15} │ {:>11.2} │ {:>11.3} │ {:>11.2} │ {:>11} │",
            algo_name, return_pct, sharpe, drawdown, trades
        );

        // ベスト指標の更新
        if return_pct > best_return.1 {
            best_return = (result.config.algorithm.clone(), return_pct);
        }
        if sharpe > best_sharpe.1 {
            best_sharpe = (result.config.algorithm.clone(), sharpe);
        }
        if drawdown < lowest_drawdown.1 {
            lowest_drawdown = (result.config.algorithm.clone(), drawdown);
        }
    }

    println!("└─────────────────┴─────────────┴─────────────┴─────────────┴─────────────┘");

    println!("\n🥇 Best Performers:");
    println!(
        "  Highest Return: {:?} ({:.2}%)",
        best_return.0, best_return.1
    );
    println!(
        "  Best Sharpe Ratio: {:?} ({:.3})",
        best_sharpe.0, best_sharpe.1
    );
    println!(
        "  Lowest Drawdown: {:?} ({:.2}%)",
        lowest_drawdown.0, lowest_drawdown.1
    );

    // 出力ディレクトリを作成
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let final_output_dir = Path::new(output_dir).join(format!("multi_algorithm_{}", timestamp));

    fs::create_dir_all(&final_output_dir).with_context(|| {
        format!(
            "Failed to create output directory: {}",
            final_output_dir.display()
        )
    })?;

    // MultiAlgorithmSimulationResult 構造体を作成
    use crate::commands::simulate::{
        AlgorithmComparison, AlgorithmSummaryRow, MultiAlgorithmSimulationResult,
    };

    let comparison = AlgorithmComparison {
        best_return: (best_return.0, best_return.1),
        best_sharpe: (best_sharpe.0, best_sharpe.1),
        lowest_drawdown: (lowest_drawdown.0, lowest_drawdown.1),
        summary_table: results
            .iter()
            .map(|r| AlgorithmSummaryRow {
                algorithm: r.config.algorithm.clone(),
                total_return_pct: r.performance.total_return_pct,
                annualized_return: r.performance.annualized_return / 100.0,
                sharpe_ratio: r.performance.sharpe_ratio,
                max_drawdown_pct: r.performance.max_drawdown_pct,
                total_trades: r.performance.total_trades,
                win_rate: r.performance.win_rate,
            })
            .collect(),
    };

    let multi_result = MultiAlgorithmSimulationResult {
        results: results.to_vec(),
        comparison,
    };

    let summary_filepath = final_output_dir.join("multi_results.json");
    let summary_json = serde_json::to_string_pretty(&multi_result)
        .context("Failed to serialize multi-algorithm result to JSON")?;

    fs::write(&summary_filepath, summary_json).with_context(|| {
        format!(
            "Failed to write multi-algorithm result to {}",
            summary_filepath.display()
        )
    })?;

    println!("💾 Multi-algorithm comparison completed successfully!");
    println!("📁 Results saved to: {}", summary_filepath.display());
    println!(
        "💡 Generate HTML report with: cli_tokens report {}",
        summary_filepath.display()
    );

    Ok(())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod api_integration_tests;

#[cfg(test)]
mod algorithm_tests;
