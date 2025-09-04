use anyhow::{Context, Result};
use chrono::Utc;
use clap::Args;
use std::path::PathBuf;

use crate::commands::simulate::{PortfolioValue, SimulationResult, TradeExecution};

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

    let result: SimulationResult = serde_json::from_str(&json_content)
        .with_context(|| format!("Failed to parse JSON file: {}", args.input.display()))?;

    // 出力先を決定
    let output_path = if let Some(output) = args.output {
        output
    } else {
        // 入力ファイルと同じディレクトリにreport.htmlを作成
        let parent = args
            .input
            .parent()
            .context("Failed to get parent directory")?;
        parent.join("report.html")
    };

    match args.format.as_str() {
        "html" => {
            let html_content = generate_html_report(&result)?;
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

/// HTMLレポートを生成
#[allow(clippy::format_in_format_args)]
fn generate_html_report(result: &SimulationResult) -> Result<String> {
    let config = &result.config;
    let perf = &result.performance;

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
        config.start_date.format("%Y-%m-%d"),
        config.end_date.format("%Y-%m-%d"),
        format!("{:?}", config.algorithm),
        config.initial_capital,
        "wrap.near",
        config.final_value,
        "wrap.near",
        if perf.total_return_pct >= 0.0 {
            "positive"
        } else {
            "negative"
        },
        perf.total_return_pct / 100.0,
        perf.max_drawdown_pct,
        perf.sharpe_ratio,
        perf.volatility,
        perf.total_trades,
        perf.win_rate,
        perf.active_trading_days,
        perf.simulation_days,
        perf.total_costs,
        generate_trades_html(&result.trades),
        Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
        generate_portfolio_data_js(&result.portfolio_values),
    );

    Ok(html_content)
}

fn generate_trades_html(trades: &[TradeExecution]) -> String {
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

fn generate_portfolio_data_js(values: &[PortfolioValue]) -> String {
    let labels: Vec<String> = values
        .iter()
        .map(|pv| format!("'{}'", pv.timestamp.format("%m/%d")))
        .collect();

    let data: Vec<String> = values
        .iter()
        .map(|pv| format!("{:.2}", pv.total_value))
        .collect();

    format!(
        "{{labels: [{}], values: [{}]}}",
        labels.join(","),
        data.join(",")
    )
}
