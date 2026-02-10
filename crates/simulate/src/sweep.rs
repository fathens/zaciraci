use crate::cli::Cli;
use crate::engine::run_simulation;
use anyhow::Result;
use logging::*;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct SweepConfig {
    #[serde(default = "default_top_tokens")]
    pub top_tokens: Vec<usize>,
    #[serde(default = "default_volatility_days")]
    pub volatility_days: Vec<i64>,
    #[serde(default = "default_price_history_days")]
    pub price_history_days: Vec<i64>,
    #[serde(default = "default_rebalance_threshold")]
    pub rebalance_threshold: Vec<f64>,
    #[serde(default = "default_rebalance_interval_days")]
    pub rebalance_interval_days: Vec<i64>,
}

fn default_top_tokens() -> Vec<usize> {
    vec![10]
}
fn default_volatility_days() -> Vec<i64> {
    vec![7]
}
fn default_price_history_days() -> Vec<i64> {
    vec![30]
}
fn default_rebalance_threshold() -> Vec<f64> {
    vec![0.1]
}
fn default_rebalance_interval_days() -> Vec<i64> {
    vec![1]
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SweepResult {
    pub results: Vec<SweepEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SweepEntry {
    pub parameters: SweepParameters,
    pub total_return: f64,
    pub annualized_return: f64,
    pub sharpe_ratio: f64,
    pub sortino_ratio: f64,
    pub max_drawdown: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SweepParameters {
    pub top_tokens: usize,
    pub volatility_days: i64,
    pub price_history_days: i64,
    pub rebalance_threshold: f64,
    pub rebalance_interval_days: i64,
}

pub async fn run_sweep(base_cli: &Cli, sweep_config_path: &Path) -> Result<()> {
    let log = DEFAULT.new(o!("function" => "run_sweep"));

    let config_str = std::fs::read_to_string(sweep_config_path)?;
    let sweep_config: SweepConfig = serde_json::from_str(&config_str)?;

    let combinations = generate_combinations(&sweep_config);
    info!(log, "starting parameter sweep"; "combinations" => combinations.len());

    let mut results = Vec::new();

    for (i, params) in combinations.iter().enumerate() {
        info!(log, "running combination"; "index" => i + 1, "total" => combinations.len());

        let mut cli = base_cli.clone();
        cli.top_tokens = params.top_tokens;
        cli.volatility_days = params.volatility_days;
        cli.price_history_days = params.price_history_days;
        cli.rebalance_threshold = params.rebalance_threshold;
        cli.rebalance_interval_days = params.rebalance_interval_days;

        match run_simulation(&cli).await {
            Ok(result) => {
                results.push(SweepEntry {
                    parameters: SweepParameters {
                        top_tokens: params.top_tokens,
                        volatility_days: params.volatility_days,
                        price_history_days: params.price_history_days,
                        rebalance_threshold: params.rebalance_threshold,
                        rebalance_interval_days: params.rebalance_interval_days,
                    },
                    total_return: result.performance.total_return,
                    annualized_return: result.performance.annualized_return,
                    sharpe_ratio: result.performance.sharpe_ratio,
                    sortino_ratio: result.performance.sortino_ratio,
                    max_drawdown: result.performance.max_drawdown,
                });

                // Write individual result
                let individual_path = base_cli.output.join(format!("result_{:03}.json", i + 1));
                if let Some(parent) = individual_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                result.write_to_file(&individual_path)?;
            }
            Err(e) => {
                warn!(log, "combination failed"; "index" => i + 1, "error" => ?e);
            }
        }
    }

    // Sort by Sharpe ratio descending
    results.sort_by(|a, b| {
        b.sharpe_ratio
            .partial_cmp(&a.sharpe_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Write sweep summary
    let sweep_result = SweepResult { results };
    let summary_path = base_cli.output.join("sweep_summary.json");
    if let Some(parent) = summary_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&sweep_result)?;
    std::fs::write(&summary_path, json)?;

    info!(log, "sweep completed"; "results" => sweep_result.results.len());

    // Print summary table
    print_summary_table(&sweep_result);

    Ok(())
}

fn generate_combinations(config: &SweepConfig) -> Vec<SweepParameters> {
    let mut combinations = Vec::new();

    for &top_tokens in &config.top_tokens {
        for &volatility_days in &config.volatility_days {
            for &price_history_days in &config.price_history_days {
                for &rebalance_threshold in &config.rebalance_threshold {
                    for &rebalance_interval_days in &config.rebalance_interval_days {
                        combinations.push(SweepParameters {
                            top_tokens,
                            volatility_days,
                            price_history_days,
                            rebalance_threshold,
                            rebalance_interval_days,
                        });
                    }
                }
            }
        }
    }

    combinations
}

fn print_summary_table(result: &SweepResult) {
    println!(
        "\n{:<8} {:<8} {:<8} {:<10} {:<10} {:>10} {:>12} {:>10} {:>10}",
        "TopTok",
        "VolDays",
        "HistDays",
        "RebThresh",
        "RebIntv",
        "Return%",
        "Ann.Return%",
        "Sharpe",
        "MaxDD%"
    );
    println!("{}", "-".repeat(96));

    for entry in &result.results {
        println!(
            "{:<8} {:<8} {:<8} {:<10.2} {:<10} {:>10.2} {:>12.2} {:>10.3} {:>10.2}",
            entry.parameters.top_tokens,
            entry.parameters.volatility_days,
            entry.parameters.price_history_days,
            entry.parameters.rebalance_threshold,
            entry.parameters.rebalance_interval_days,
            entry.total_return * 100.0,
            entry.annualized_return * 100.0,
            entry.sharpe_ratio,
            entry.max_drawdown * 100.0,
        );
    }
}
