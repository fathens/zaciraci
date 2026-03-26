use crate::cli::VerifyArgs;
use crate::portfolio_state::to_f64_or_warn;
use anyhow::Result;
use bigdecimal::BigDecimal;
use chrono::NaiveTime;
use logging::*;
use num_traits::Zero;
use persistence::trade_transaction::TradeTransaction;
use serde::Serialize;
use std::collections::BTreeMap;

/// Per-token-pair accuracy statistics
#[derive(Debug, Serialize)]
pub struct TokenPairStats {
    pub count: usize,
    pub mean_error_pct: f64,
    pub max_error_pct: f64,
}

/// Overall estimate_return accuracy analysis
#[derive(Debug, Serialize)]
pub struct SlippageAnalysis {
    pub total_trades: usize,
    pub trades_with_actual: usize,
    pub trades_without_actual: usize,
    pub mean_error_pct: f64,
    pub median_error_pct: f64,
    pub std_dev_pct: f64,
    pub p95_error_pct: f64,
    pub max_error_pct: f64,
    pub by_token_pair: BTreeMap<String, TokenPairStats>,
}

/// Estimated gas cost per swap transaction (NEAR)
const ESTIMATED_GAS_COST_NEAR: f64 = 0.01;

/// Calculate divergence percentage: (actual - estimated) / estimated * 100
fn divergence_pct(estimated: &BigDecimal, actual: &BigDecimal) -> Option<f64> {
    if estimated.is_zero() {
        return None;
    }
    let diff = actual - estimated;
    let pct = &diff / estimated * BigDecimal::from(100);
    Some(to_f64_or_warn(&pct, "divergence_pct"))
}

/// Compute the analysis from trade transactions
pub fn analyze(transactions: &[TradeTransaction]) -> SlippageAnalysis {
    let total_trades = transactions.len();

    let mut errors: Vec<f64> = Vec::new();
    let mut pair_errors: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut trades_without_actual = 0usize;

    for tx in transactions {
        let Some(ref actual) = tx.actual_to_amount else {
            trades_without_actual += 1;
            continue;
        };

        let estimated = tx.to_amount.as_bigdecimal();
        let Some(pct) = divergence_pct(estimated, actual) else {
            continue;
        };

        errors.push(pct);
        let pair_key = format!("{} -> {}", tx.from_token, tx.to_token);
        pair_errors.entry(pair_key).or_default().push(pct);
    }

    let trades_with_actual = errors.len();

    let (mean_error_pct, median_error_pct, std_dev_pct, p95_error_pct, max_error_pct) = if errors
        .is_empty()
    {
        (0.0, 0.0, 0.0, 0.0, 0.0)
    } else {
        let mean = errors.iter().sum::<f64>() / errors.len() as f64;
        let variance = errors.iter().map(|e| (e - mean).powi(2)).sum::<f64>() / errors.len() as f64;
        let std_dev = variance.sqrt();

        errors.sort_by(f64::total_cmp);
        let median = if errors.len().is_multiple_of(2) {
            (errors[errors.len() / 2 - 1] + errors[errors.len() / 2]) / 2.0
        } else {
            errors[errors.len() / 2]
        };

        // p95: 95th percentile of absolute errors (worst 5%)
        let mut abs_sorted: Vec<f64> = errors.iter().map(|e| e.abs()).collect();
        abs_sorted.sort_by(f64::total_cmp);
        let p95_idx = ((abs_sorted.len() as f64) * 0.95).ceil() as usize;
        let p95 = abs_sorted[p95_idx.min(abs_sorted.len()).saturating_sub(1)];

        let max_abs = abs_sorted.last().copied().unwrap_or(0.0);

        (mean, median, std_dev, p95, max_abs)
    };

    let by_token_pair = pair_errors
        .into_iter()
        .map(|(pair, errs)| {
            let count = errs.len();
            let mean = errs.iter().sum::<f64>() / count as f64;
            let max = errs.iter().map(|e| e.abs()).fold(0.0f64, f64::max);
            (
                pair,
                TokenPairStats {
                    count,
                    mean_error_pct: mean,
                    max_error_pct: max,
                },
            )
        })
        .collect();

    SlippageAnalysis {
        total_trades,
        trades_with_actual,
        trades_without_actual,
        mean_error_pct,
        median_error_pct,
        std_dev_pct,
        p95_error_pct,
        max_error_pct,
        by_token_pair,
    }
}

fn print_text_report(analysis: &SlippageAnalysis, start: &str, end: &str) {
    println!("\n=== Simulation Accuracy Report ===");
    println!("Period: {} to {}", start, end);
    println!("Total real trades: {}", analysis.total_trades);
    println!(
        "Trades with actual output: {} ({:.1}%)",
        analysis.trades_with_actual,
        if analysis.total_trades > 0 {
            analysis.trades_with_actual as f64 / analysis.total_trades as f64 * 100.0
        } else {
            0.0
        }
    );
    println!(
        "Trades without actual output: {}",
        analysis.trades_without_actual
    );

    if analysis.trades_with_actual == 0 {
        println!("\nNo trades with actual output data to analyze.");
        return;
    }

    let bias = if analysis.mean_error_pct < 0.0 {
        "estimate overestimates"
    } else if analysis.mean_error_pct > 0.0 {
        "estimate underestimates"
    } else {
        "no systematic bias"
    };

    println!("\nestimate_return accuracy:");
    println!(
        "  Mean error:   {:+.4}% ({})",
        analysis.mean_error_pct, bias
    );
    println!("  Median error: {:+.4}%", analysis.median_error_pct);
    println!("  Std dev:       {:.4}%", analysis.std_dev_pct);
    println!("  95th pct:      {:.4}%", analysis.p95_error_pct);
    println!("  Max error:     {:.4}%", analysis.max_error_pct);

    if !analysis.by_token_pair.is_empty() {
        println!("\nBy token pair:");
        let mut pairs: Vec<_> = analysis.by_token_pair.iter().collect();
        pairs.sort_by(|a, b| b.1.count.cmp(&a.1.count));
        for (pair, stats) in pairs {
            println!(
                "  {:50} avg {:+.4}%, max {:.4}%, n={}",
                pair, stats.mean_error_pct, stats.max_error_pct, stats.count
            );
        }
    }

    // Gas cost estimate
    println!("\nGas cost omission (estimated):");
    println!("  Total trades: {}", analysis.total_trades);
    let gas_cost = analysis.total_trades as f64 * ESTIMATED_GAS_COST_NEAR;
    println!("  Est. gas cost: ~{:.2} NEAR", gas_cost);
}

/// Parse and validate date range from VerifyArgs
fn parse_date_range(args: &VerifyArgs) -> Result<(chrono::NaiveDate, chrono::NaiveDate)> {
    let start_date = args.parse_start_date()?;
    let end_date = args.parse_end_date()?;

    if start_date >= end_date {
        return Err(anyhow::anyhow!(
            "start-date must be before end-date: {} >= {}",
            start_date,
            end_date
        ));
    }

    Ok((start_date, end_date))
}

pub async fn run_verify(args: &VerifyArgs) -> Result<()> {
    let log = DEFAULT.new(o!("function" => "run_verify"));

    let (start_date, end_date) = parse_date_range(args)?;

    info!(log, "verifying simulation accuracy";
        "start_date" => %start_date, "end_date" => %end_date
    );

    let start_dt =
        start_date.and_time(NaiveTime::from_hms_opt(0, 0, 0).expect("valid HMS constant"));
    let end_dt =
        end_date.and_time(NaiveTime::from_hms_opt(23, 59, 59).expect("valid HMS constant"));

    let transactions = TradeTransaction::find_by_date_range_async(start_dt, end_dt).await?;

    info!(log, "loaded transactions"; "count" => transactions.len());

    let analysis = analyze(&transactions);

    match args.format {
        crate::cli::OutputFormat::Text => {
            print_text_report(&analysis, &args.start_date, &args.end_date);
        }
        crate::cli::OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&analysis)?;
            println!("{}", json);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests;
