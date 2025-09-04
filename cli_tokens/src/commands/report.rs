use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::Args;
use std::path::PathBuf;

use crate::commands::simulate::{PortfolioValue, SimulationResult, TradeExecution};

#[cfg(test)]
mod tests;

// === Phase 4.1: Pure Data Structures ===

#[derive(Debug, Clone)]
pub struct ReportData {
    pub config: SimulationConfig,
    pub performance: PerformanceMetrics,
    pub trades: Vec<TradeExecution>,
    pub portfolio_values: Vec<PortfolioValue>,
}

#[derive(Debug, Clone)]
pub struct SimulationConfig {
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
    pub algorithm: String,
    pub initial_capital: f64,
    pub final_value: f64,
}

#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub total_return_pct: f64,
    pub max_drawdown_pct: f64,
    pub sharpe_ratio: f64,
    pub volatility: f64,
    pub total_trades: usize,
    pub win_rate: f64,
    pub active_trading_days: usize,
    pub simulation_days: usize,
    pub total_costs: f64,
}

#[derive(Debug, Clone)]
pub struct ChartData {
    pub labels: Vec<String>,
    pub values: Vec<f64>,
}

#[derive(Debug, Clone)]
pub struct ReportMetrics {
    pub performance_class: String, // "positive" or "negative"
    pub currency_symbol: String,
    pub chart_data: ChartData,
    pub trades_html: String,
    pub generation_timestamp: String,
}

// === Phase 4.1: Pure Functions ===

/// Convert SimulationResult to structured ReportData
pub fn extract_report_data(result: &SimulationResult) -> ReportData {
    ReportData {
        config: SimulationConfig {
            start_date: result.config.start_date,
            end_date: result.config.end_date,
            algorithm: format!("{:?}", result.config.algorithm),
            initial_capital: result.config.initial_capital,
            final_value: result.config.final_value,
        },
        performance: PerformanceMetrics {
            total_return_pct: result.performance.total_return_pct,
            max_drawdown_pct: result.performance.max_drawdown_pct,
            sharpe_ratio: result.performance.sharpe_ratio,
            volatility: result.performance.volatility,
            total_trades: result.performance.total_trades,
            win_rate: result.performance.win_rate,
            active_trading_days: result.performance.active_trading_days as usize,
            simulation_days: result.performance.simulation_days as usize,
            total_costs: result.performance.total_costs,
        },
        trades: result.trades.clone(),
        portfolio_values: result.portfolio_values.clone(),
    }
}

/// Calculate derived metrics for report generation
pub fn calculate_report_metrics(data: &ReportData) -> ReportMetrics {
    let performance_class = if data.performance.total_return_pct >= 0.0 {
        "positive".to_string()
    } else {
        "negative".to_string()
    };

    ReportMetrics {
        performance_class,
        currency_symbol: "wrap.near".to_string(), // TODO: Make configurable
        chart_data: generate_portfolio_chart_data(&data.portfolio_values),
        trades_html: generate_trades_table_html(&data.trades),
        generation_timestamp: Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string(),
    }
}

/// Generate chart data for portfolio value over time
pub fn generate_portfolio_chart_data(values: &[PortfolioValue]) -> ChartData {
    let labels: Vec<String> = values
        .iter()
        .map(|pv| format!("'{}'", pv.timestamp.format("%m/%d")))
        .collect();

    let chart_values: Vec<f64> = values.iter().map(|pv| pv.total_value).collect();

    ChartData {
        labels,
        values: chart_values,
    }
}

/// Generate HTML table for recent trades
pub fn generate_trades_table_html(trades: &[TradeExecution]) -> String {
    if trades.is_empty() {
        return r#"<tr><td colspan="6" style="text-align: center; color: #999;">No trades executed</td></tr>"#.to_string();
    }

    trades
        .iter()
        .rev()
        .take(10)
        .map(|trade| {
            format!(
                r#"<tr>
                    <td>{}</td>
                    <td>{} → {}</td>
                    <td>{:.4}</td>
                    <td>{:.6}</td>
                    <td>{:.4}</td>
                    <td>{}</td>
                </tr>"#,
                trade.timestamp.format("%Y-%m-%d %H:%M"),
                trade.from_token,
                trade.to_token,
                trade.amount,
                trade.executed_price,
                trade.cost.total,
                trade.reason
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Generate JavaScript object for chart data
pub fn generate_chart_data_js(chart_data: &ChartData) -> String {
    format!(
        "{{labels: [{}], values: [{}]}}",
        chart_data.labels.join(","),
        chart_data
            .values
            .iter()
            .map(|v| format!("{:.2}", v))
            .collect::<Vec<_>>()
            .join(",")
    )
}

#[derive(Debug, Args)]
pub struct ReportArgs {
    /// Input JSON file path (simulation result)
    pub input: PathBuf,

    /// Output format
    #[arg(short = 'f', long, default_value = "html")]
    pub format: String,

    /// Output file path (optional, defaults to same directory as input)
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

pub fn run_report(args: ReportArgs) -> Result<()> {
    // JSONファイルを読み込み
    let json_content = std::fs::read_to_string(&args.input)
        .with_context(|| format!("Failed to read input file: {}", args.input.display()))?;

    let simulation_result: SimulationResult = serde_json::from_str(&json_content)
        .with_context(|| format!("Failed to parse JSON file: {}", args.input.display()))?;

    // Phase 4.1: Use pure functions for data extraction and processing
    let report_data = extract_report_data(&simulation_result);
    let report_metrics = calculate_report_metrics(&report_data);

    // 出力先を決定
    let output_path = determine_output_path(&args)?;

    match args.format.as_str() {
        "html" => {
            let html_content = generate_html_report_v2(&report_data, &report_metrics)?;
            std::fs::write(&output_path, html_content)
                .with_context(|| format!("Failed to write HTML file: {}", output_path.display()))?;
            println!("📊 HTML report saved to: {}", output_path.display());
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Unsupported format: {}. Supported formats: html",
                args.format
            ))
        }
    }

    Ok(())
}

/// Determine output file path based on args
fn determine_output_path(args: &ReportArgs) -> Result<PathBuf> {
    if let Some(output) = &args.output {
        Ok(output.clone())
    } else {
        // 入力ファイルと同じディレクトリにreport.htmlを作成
        let parent = args
            .input
            .parent()
            .context("Failed to get parent directory")?;
        Ok(parent.join("report.html"))
    }
}

/// Generate HTML report using new structured approach (Phase 4.1)
fn generate_html_report_v2(data: &ReportData, metrics: &ReportMetrics) -> Result<String> {
    let html_content = format!(
        r#"<!DOCTYPE html>
<html lang="ja">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Trading Simulation Report</title>
    <style>
        * {{
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
            line-height: 1.6;
            color: #333;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            min-height: 100vh;
            padding: 20px;
        }}
        .container {{
            max-width: 1200px;
            margin: 0 auto;
            background: white;
            border-radius: 20px;
            box-shadow: 0 20px 60px rgba(0,0,0,0.3);
            overflow: hidden;
        }}
        .header {{
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            padding: 40px;
            text-align: center;
        }}
        .header h1 {{
            font-size: 2.5em;
            margin-bottom: 10px;
        }}
        .header p {{
            font-size: 1.1em;
            opacity: 0.9;
        }}
        .content {{
            padding: 40px;
        }}
        .section {{
            margin-bottom: 40px;
        }}
        .section h2 {{
            color: #667eea;
            margin-bottom: 20px;
            padding-bottom: 10px;
            border-bottom: 2px solid #f0f0f0;
        }}
        .metrics-grid {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
            gap: 20px;
            margin-top: 20px;
        }}
        .metric-card {{
            background: #f8f9fa;
            padding: 20px;
            border-radius: 10px;
            border-left: 4px solid #667eea;
        }}
        .metric-label {{
            color: #666;
            font-size: 0.9em;
            margin-bottom: 5px;
        }}
        .metric-value {{
            font-size: 1.5em;
            font-weight: bold;
            color: #333;
        }}
        .positive {{
            color: #28a745;
        }}
        .negative {{
            color: #dc3545;
        }}
        .chart-container {{
            margin-top: 20px;
            padding: 20px;
            background: #f8f9fa;
            border-radius: 10px;
        }}
        canvas {{
            max-width: 100%;
            height: auto;
        }}
        table {{
            width: 100%;
            border-collapse: collapse;
            margin-top: 20px;
        }}
        th, td {{
            padding: 12px;
            text-align: left;
            border-bottom: 1px solid #e0e0e0;
        }}
        th {{
            background: #f8f9fa;
            font-weight: 600;
            color: #666;
        }}
        tr:hover {{
            background: #f8f9fa;
        }}
        .footer {{
            background: #f8f9fa;
            padding: 20px;
            text-align: center;
            color: #666;
            font-size: 0.9em;
        }}
    </style>
    <script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>📊 Trading Simulation Report</h1>
            <p>{} - {} | {} Algorithm</p>
        </div>
        
        <div class="content">
            <div class="section">
                <h2>📈 Performance Summary</h2>
                <div class="metrics-grid">
                    <div class="metric-card">
                        <div class="metric-label">Initial Capital</div>
                        <div class="metric-value">{:.2} {}</div>
                    </div>
                    <div class="metric-card">
                        <div class="metric-label">Final Value</div>
                        <div class="metric-value">{:.2} {}</div>
                    </div>
                    <div class="metric-card">
                        <div class="metric-label">Total Return</div>
                        <div class="metric-value {}">{:+.2}%</div>
                    </div>
                    <div class="metric-card">
                        <div class="metric-label">Max Drawdown</div>
                        <div class="metric-value negative">{:.2}%</div>
                    </div>
                    <div class="metric-card">
                        <div class="metric-label">Sharpe Ratio</div>
                        <div class="metric-value">{:.2}</div>
                    </div>
                    <div class="metric-card">
                        <div class="metric-label">Volatility</div>
                        <div class="metric-value">{:.2}%</div>
                    </div>
                </div>
            </div>

            <div class="section">
                <h2>📊 Portfolio Value Over Time</h2>
                <div class="chart-container">
                    <canvas id="portfolioChart"></canvas>
                </div>
            </div>

            <div class="section">
                <h2>🔄 Trading Activity</h2>
                <div class="metrics-grid">
                    <div class="metric-card">
                        <div class="metric-label">Total Trades</div>
                        <div class="metric-value">{}</div>
                    </div>
                    <div class="metric-card">
                        <div class="metric-label">Win Rate</div>
                        <div class="metric-value">{:.1}%</div>
                    </div>
                    <div class="metric-card">
                        <div class="metric-label">Active Days</div>
                        <div class="metric-value">{} / {}</div>
                    </div>
                    <div class="metric-card">
                        <div class="metric-label">Total Costs</div>
                        <div class="metric-value">{:.2}</div>
                    </div>
                </div>
            </div>

            <div class="section">
                <h2>📝 Recent Trades</h2>
                <table>
                    <thead>
                        <tr>
                            <th>Timestamp</th>
                            <th>Trade</th>
                            <th>Amount</th>
                            <th>Price</th>
                            <th>Fee</th>
                            <th>Reason</th>
                        </tr>
                    </thead>
                    <tbody>
                        {}
                    </tbody>
                </table>
            </div>
        </div>

        <div class="footer">
            <p>Generated at {} | CLI Tokens Trading Simulator</p>
        </div>
    </div>

    <script>
        const ctx = document.getElementById('portfolioChart').getContext('2d');
        const portfolioData = {};
        
        new Chart(ctx, {{
            type: 'line',
            data: {{
                labels: portfolioData.labels,
                datasets: [{{
                    label: 'Portfolio Value',
                    data: portfolioData.values,
                    borderColor: '#667eea',
                    backgroundColor: 'rgba(102, 126, 234, 0.1)',
                    borderWidth: 2,
                    fill: true,
                    tension: 0.4
                }}]
            }},
            options: {{
                responsive: true,
                plugins: {{
                    legend: {{
                        display: false
                    }},
                    title: {{
                        display: false
                    }}
                }},
                scales: {{
                    y: {{
                        beginAtZero: false,
                        ticks: {{
                            callback: function(value) {{
                                return value.toLocaleString();
                            }}
                        }}
                    }}
                }}
            }}
        }});
    </script>
</body>
</html>"#,
        data.config.start_date.format("%Y-%m-%d"),
        data.config.end_date.format("%Y-%m-%d"),
        data.config.algorithm,
        data.config.initial_capital,
        metrics.currency_symbol,
        data.config.final_value,
        metrics.currency_symbol,
        metrics.performance_class,
        data.performance.total_return_pct,
        data.performance.max_drawdown_pct,
        data.performance.sharpe_ratio,
        data.performance.volatility * 100.0,
        data.performance.total_trades,
        data.performance.win_rate * 100.0,
        data.performance.active_trading_days,
        data.performance.simulation_days,
        data.performance.total_costs,
        metrics.trades_html,
        metrics.generation_timestamp,
        generate_chart_data_js(&metrics.chart_data),
    );

    Ok(html_content)
}
