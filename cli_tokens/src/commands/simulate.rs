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
    println!("🚀 Starting trading simulation...");

    let run_all_algorithms = args.algorithm.is_none();

    if args.verbose {
        println!("📋 Configuration:");
        if run_all_algorithms {
            println!("  Algorithm: All algorithms (Momentum, Portfolio, TrendFollowing)");
        } else {
            println!("  Algorithm: {:?}", args.algorithm);
        }
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
    println!("🔍 Loading tokens from top command output directory...");

    let tokens_dir = get_tokens_directory()?;
    let token_names = load_tokens_from_directory(&tokens_dir, &final_config.quote_token)?;

    if token_names.is_empty() {
        return Err(anyhow::anyhow!(
            "No tokens found in directory: {}. Please run 'cli_tokens top' first to generate token files.",
            tokens_dir.display()
        ));
    }

    println!(
        "📈 Found {} tokens: {}",
        token_names.len(),
        token_names.join(", ")
    );

    final_config.target_tokens = token_names;

    if run_all_algorithms {
        // 全アルゴリズムを実行
        println!("\n🔄 Running all algorithms for comparison...");

        let algorithms = [
            AlgorithmType::Momentum,
            AlgorithmType::Portfolio,
            AlgorithmType::TrendFollowing,
        ];
        let mut results = Vec::new();

        for algorithm in &algorithms {
            let mut config_copy = final_config.clone();
            config_copy.algorithm = algorithm.clone();

            println!("\n--- Running {:?} Algorithm ---", algorithm);
            let result = run_single_algorithm(&config_copy).await?;
            results.push(result);
        }

        // 複数アルゴリズムの結果を保存
        save_simple_multi_algorithm_result(&results, &output_dir)?;
    } else {
        // 単一アルゴリズムを実行
        let result = run_single_algorithm(&final_config).await?;

        // 単一アルゴリズムの結果を保存
        save_simulation_result(&result, &output_dir)?;

        // 単一アルゴリズムの場合のみサマリーを表示
        println!("\n📊 Simulation Summary:");
        println!(
            "  Total Return: {:.2}%",
            result.performance.total_return_pct
        );
        println!("  Sharpe Ratio: {:.3}", result.performance.sharpe_ratio);
        println!(
            "  Max Drawdown: {:.2}%",
            result.performance.max_drawdown_pct
        );
        println!("  Total Trades: {}", result.performance.total_trades);
        println!("  Win Rate: {:.1}%", result.performance.win_rate);
        println!("  Final Value: ${:.2}", result.config.final_value);
    }

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

    // アルゴリズムタイプの決定
    let algorithm = if let Some(algo_str) = args.algorithm {
        AlgorithmType::from(algo_str.as_str())
    } else {
        AlgorithmType::Momentum // 全アルゴリズム実行時の一時的な値
    };

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
async fn run_single_algorithm(config: &SimulationConfig) -> Result<SimulationResult> {
    match config.algorithm {
        AlgorithmType::Momentum => run_momentum_simulation(config).await,
        AlgorithmType::Portfolio => run_portfolio_simulation(config).await,
        AlgorithmType::TrendFollowing => run_trend_following_simulation(config).await,
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

    let mut best_return = ("None".to_string(), f64::NEG_INFINITY);
    let mut best_sharpe = ("None".to_string(), f64::NEG_INFINITY);
    let mut lowest_drawdown = ("None".to_string(), f64::INFINITY);

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
            best_return = (algo_name.clone(), return_pct);
        }
        if sharpe > best_sharpe.1 {
            best_sharpe = (algo_name.clone(), sharpe);
        }
        if drawdown < lowest_drawdown.1 {
            lowest_drawdown = (algo_name, drawdown);
        }
    }

    println!("└─────────────────┴─────────────┴─────────────┴─────────────┴─────────────┘");

    println!("\n🥇 Best Performers:");
    println!(
        "  Highest Return: {} ({:.2}%)",
        best_return.0, best_return.1
    );
    println!(
        "  Best Sharpe Ratio: {} ({:.3})",
        best_sharpe.0, best_sharpe.1
    );
    println!(
        "  Lowest Drawdown: {} ({:.2}%)",
        lowest_drawdown.0, lowest_drawdown.1
    );

    // 出力ディレクトリを作成
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let final_output_dir = Path::new(output_dir)
        .join("simulation_results")
        .join(format!("multi_algorithm_{}", timestamp));

    fs::create_dir_all(&final_output_dir).with_context(|| {
        format!(
            "Failed to create output directory: {}",
            final_output_dir.display()
        )
    })?;

    // 各アルゴリズムの結果を個別に保存
    for result in results {
        let filename = format!("{:?}_result.json", result.config.algorithm).to_lowercase();
        let filepath = final_output_dir.join(&filename);

        let json_content = serde_json::to_string_pretty(result)
            .context("Failed to serialize simulation result to JSON")?;

        fs::write(&filepath, json_content).with_context(|| {
            format!(
                "Failed to write simulation result to {}",
                filepath.display()
            )
        })?;
    }

    // 比較サマリーをJSONとして保存
    let summary = serde_json::json!({
        "comparison_type": "multi_algorithm",
        "algorithms": results.iter().map(|r| format!("{:?}", r.config.algorithm)).collect::<Vec<_>>(),
        "best_performers": {
            "highest_return": {
                "algorithm": best_return.0,
                "value": best_return.1
            },
            "best_sharpe": {
                "algorithm": best_sharpe.0,
                "value": best_sharpe.1
            },
            "lowest_drawdown": {
                "algorithm": lowest_drawdown.0,
                "value": lowest_drawdown.1
            }
        },
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "results": results
    });

    let summary_filepath = final_output_dir.join("multi_results.json");
    let summary_json = serde_json::to_string_pretty(&summary)
        .context("Failed to serialize comparison summary to JSON")?;

    fs::write(&summary_filepath, summary_json).with_context(|| {
        format!(
            "Failed to write comparison summary to {}",
            summary_filepath.display()
        )
    })?;

    println!("💾 Multi-algorithm comparison completed successfully!");
    println!("📁 Results saved to: {}", final_output_dir.display());
    println!(
        "📄 Individual results: {:?}_result.json for each algorithm",
        results
            .iter()
            .map(|r| format!("{:?}", r.config.algorithm))
            .collect::<Vec<_>>()
    );
    println!("📊 Comparison summary: multi_results.json");
    println!(
        "💡 Generate HTML report with: cli_tokens report {}",
        summary_filepath.display()
    );

    Ok(())
}

#[cfg(test)]
mod tests;
