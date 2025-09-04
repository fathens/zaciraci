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

// === Phase 4.2: Extended Data Structures ===

#[derive(Debug, Clone)]
pub struct ReportConfiguration {
    pub theme: ReportTheme,
    pub currency: CurrencyConfig,
    pub chart_settings: ChartSettings,
    pub display_options: DisplayOptions,
}

#[derive(Debug, Clone)]
pub enum ReportTheme {
    Default,
    Dark,
    Light,
    Minimal,
}

#[derive(Debug, Clone)]
pub struct CurrencyConfig {
    pub symbol: String,
    pub decimal_places: u8,
    pub position: CurrencyPosition,
}

#[derive(Debug, Clone)]
pub enum CurrencyPosition {
    Before,
    After,
}

#[derive(Debug, Clone)]
pub struct ChartSettings {
    pub chart_type: ChartType,
    pub show_volume: bool,
    pub show_trades: bool,
    pub color_scheme: ColorScheme,
}

#[derive(Debug, Clone)]
pub enum ChartType {
    Line,
    Area,
    Candlestick,
}

#[derive(Debug, Clone)]
pub struct ColorScheme {
    pub primary: String,
    pub secondary: String,
    pub positive: String,
    pub negative: String,
}

#[derive(Debug, Clone)]
pub struct DisplayOptions {
    pub show_detailed_trades: bool,
    pub max_trades_displayed: usize,
    pub include_performance_comparison: bool,
    pub show_risk_metrics: bool,
}

// === Phase 4.3: Template System ===

#[derive(Debug, Clone)]
pub struct HtmlTemplate {
    pub template_name: String,
    pub title: String,
    pub css_style: String,
    pub javascript_libs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TemplateContext {
    pub config: ReportConfiguration,
    pub data: ReportData,
    pub metrics: ReportMetrics,
    pub extended_metrics: Option<ExtendedMetrics>,
}

#[derive(Debug, Clone)]
pub enum TemplateSection {
    Header,
    PerformanceSummary,
    PortfolioChart,
    TradingActivity,
    TradeHistory,
    RiskAnalysis,
    Footer,
}

#[derive(Debug, Clone)]
pub struct ExtendedMetrics {
    pub risk_metrics: RiskMetrics,
    pub performance_comparison: Option<PerformanceComparison>,
    pub trade_analysis: TradeAnalysis,
}

#[derive(Debug, Clone)]
pub struct RiskMetrics {
    pub value_at_risk_95: f64,
    pub value_at_risk_99: f64,
    pub expected_shortfall: f64,
    pub beta: f64,
    pub alpha: f64,
    pub information_ratio: f64,
}

#[derive(Debug, Clone)]
pub struct PerformanceComparison {
    pub benchmark_return: f64,
    pub excess_return: f64,
    pub tracking_error: f64,
    pub upside_capture: f64,
    pub downside_capture: f64,
}

#[derive(Debug, Clone)]
pub struct TradeAnalysis {
    pub average_trade_duration: f64, // in hours
    pub largest_win: f64,
    pub largest_loss: f64,
    pub consecutive_wins: u32,
    pub consecutive_losses: u32,
    pub trade_frequency_per_day: f64,
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

// === Phase 4.2: Extended Functions ===

/// Create default report configuration
pub fn create_default_report_config() -> ReportConfiguration {
    ReportConfiguration {
        theme: ReportTheme::Default,
        currency: CurrencyConfig {
            symbol: "wrap.near".to_string(),
            decimal_places: 2,
            position: CurrencyPosition::After,
        },
        chart_settings: ChartSettings {
            chart_type: ChartType::Line,
            show_volume: false,
            show_trades: true,
            color_scheme: ColorScheme {
                primary: "#667eea".to_string(),
                secondary: "#764ba2".to_string(),
                positive: "#28a745".to_string(),
                negative: "#dc3545".to_string(),
            },
        },
        display_options: DisplayOptions {
            show_detailed_trades: true,
            max_trades_displayed: 10,
            include_performance_comparison: false,
            show_risk_metrics: true,
        },
    }
}

/// Calculate extended metrics for comprehensive analysis
pub fn calculate_extended_metrics(data: &ReportData) -> ExtendedMetrics {
    ExtendedMetrics {
        risk_metrics: calculate_risk_metrics(data),
        performance_comparison: None, // Would require benchmark data
        trade_analysis: analyze_trades(&data.trades),
    }
}

/// Calculate risk-related metrics
pub fn calculate_risk_metrics(data: &ReportData) -> RiskMetrics {
    let returns = calculate_daily_returns(&data.portfolio_values);

    RiskMetrics {
        value_at_risk_95: calculate_var(&returns, 0.95),
        value_at_risk_99: calculate_var(&returns, 0.99),
        expected_shortfall: calculate_expected_shortfall(&returns, 0.95),
        beta: 1.0,  // Would require benchmark data for proper calculation
        alpha: 0.0, // Would require benchmark data
        information_ratio: calculate_information_ratio(
            data.performance.total_return_pct,
            data.performance.volatility,
        ),
    }
}

/// Analyze trading patterns and statistics
pub fn analyze_trades(trades: &[TradeExecution]) -> TradeAnalysis {
    if trades.is_empty() {
        return TradeAnalysis {
            average_trade_duration: 0.0,
            largest_win: 0.0,
            largest_loss: 0.0,
            consecutive_wins: 0,
            consecutive_losses: 0,
            trade_frequency_per_day: 0.0,
        };
    }

    let trade_pnls: Vec<f64> = trades
        .iter()
        .map(|t| t.portfolio_value_after - t.portfolio_value_before)
        .collect();

    TradeAnalysis {
        average_trade_duration: 24.0, // Simplified: assume daily trades
        largest_win: trade_pnls.iter().fold(0.0_f64, |acc, &x| acc.max(x)),
        largest_loss: trade_pnls.iter().fold(0.0_f64, |acc, &x| acc.min(x)),
        consecutive_wins: calculate_max_consecutive_wins(&trade_pnls),
        consecutive_losses: calculate_max_consecutive_losses(&trade_pnls),
        trade_frequency_per_day: trades.len() as f64 / 30.0, // Assume 30-day period
    }
}

/// Format currency value according to configuration
pub fn format_currency_value(value: f64, config: &CurrencyConfig) -> String {
    let formatted_value = format!("{:.prec$}", value, prec = config.decimal_places as usize);

    match config.position {
        CurrencyPosition::Before => format!("{} {}", config.symbol, formatted_value),
        CurrencyPosition::After => format!("{} {}", formatted_value, config.symbol),
    }
}

// === Helper Functions ===

fn calculate_daily_returns(portfolio_values: &[PortfolioValue]) -> Vec<f64> {
    if portfolio_values.len() < 2 {
        return vec![];
    }

    portfolio_values
        .windows(2)
        .map(|window| {
            let prev_value = window[0].total_value;
            let curr_value = window[1].total_value;
            if prev_value != 0.0 {
                (curr_value - prev_value) / prev_value
            } else {
                0.0
            }
        })
        .collect()
}

fn calculate_var(returns: &[f64], confidence_level: f64) -> f64 {
    if returns.is_empty() {
        return 0.0;
    }

    let mut sorted_returns = returns.to_vec();
    sorted_returns.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let index = ((1.0 - confidence_level) * sorted_returns.len() as f64) as usize;
    sorted_returns.get(index).copied().unwrap_or(0.0)
}

fn calculate_expected_shortfall(returns: &[f64], confidence_level: f64) -> f64 {
    let var = calculate_var(returns, confidence_level);
    let tail_returns: Vec<f64> = returns.iter().filter(|&&r| r <= var).copied().collect();

    if tail_returns.is_empty() {
        0.0
    } else {
        tail_returns.iter().sum::<f64>() / tail_returns.len() as f64
    }
}

fn calculate_information_ratio(excess_return: f64, tracking_error: f64) -> f64 {
    if tracking_error != 0.0 {
        excess_return / tracking_error
    } else {
        0.0
    }
}

fn calculate_max_consecutive_wins(trade_pnls: &[f64]) -> u32 {
    let mut max_consecutive = 0;
    let mut current_consecutive = 0;

    for &pnl in trade_pnls {
        if pnl > 0.0 {
            current_consecutive += 1;
            max_consecutive = max_consecutive.max(current_consecutive);
        } else {
            current_consecutive = 0;
        }
    }

    max_consecutive
}

fn calculate_max_consecutive_losses(trade_pnls: &[f64]) -> u32 {
    let mut max_consecutive = 0;
    let mut current_consecutive = 0;

    for &pnl in trade_pnls {
        if pnl < 0.0 {
            current_consecutive += 1;
            max_consecutive = max_consecutive.max(current_consecutive);
        } else {
            current_consecutive = 0;
        }
    }

    max_consecutive
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
            // Use the new template system (Phase 4.3)
            let html_content = generate_html_report_v3(&report_data, &report_metrics, None, None)?;
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

// === Phase 4.3: Template System Functions ===

/// Create default HTML template configuration
pub fn create_default_html_template() -> HtmlTemplate {
    HtmlTemplate {
        template_name: "default".to_string(),
        title: "Trading Simulation Report".to_string(),
        css_style: get_default_css_style(),
        javascript_libs: vec!["https://cdn.jsdelivr.net/npm/chart.js".to_string()],
    }
}

/// Create template context from report data
pub fn create_template_context(
    data: &ReportData,
    metrics: &ReportMetrics,
    config: Option<ReportConfiguration>,
    extended_metrics: Option<ExtendedMetrics>,
) -> TemplateContext {
    TemplateContext {
        config: config.unwrap_or_else(create_default_report_config),
        data: data.clone(),
        metrics: metrics.clone(),
        extended_metrics,
    }
}

/// Render HTML template section
pub fn render_template_section(section: &TemplateSection, context: &TemplateContext) -> String {
    match section {
        TemplateSection::Header => render_header_section(context),
        TemplateSection::PerformanceSummary => render_performance_summary_section(context),
        TemplateSection::PortfolioChart => render_portfolio_chart_section(context),
        TemplateSection::TradingActivity => render_trading_activity_section(context),
        TemplateSection::TradeHistory => render_trade_history_section(context),
        TemplateSection::RiskAnalysis => render_risk_analysis_section(context),
        TemplateSection::Footer => render_footer_section(context),
    }
}

/// Generate full HTML report using template system (Phase 4.3)
pub fn generate_html_report_v3(
    data: &ReportData,
    metrics: &ReportMetrics,
    template: Option<HtmlTemplate>,
    config: Option<ReportConfiguration>,
) -> Result<String> {
    let template = template.unwrap_or_else(create_default_html_template);
    let context = create_template_context(data, metrics, config, None);

    let sections = [
        TemplateSection::Header,
        TemplateSection::PerformanceSummary,
        TemplateSection::PortfolioChart,
        TemplateSection::TradingActivity,
        TemplateSection::TradeHistory,
        TemplateSection::Footer,
    ];

    let content_sections: Vec<String> = sections
        .iter()
        .map(|section| render_template_section(section, &context))
        .collect();

    let html_content = format!(
        r#"<!DOCTYPE html>
<html lang="ja">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{}</title>
    <style>{}</style>
    {}
</head>
<body>
    <div class="container">
        {}
    </div>
    <script>
        {}
    </script>
</body>
</html>"#,
        template.title,
        template.css_style,
        template
            .javascript_libs
            .iter()
            .map(|lib| format!("<script src=\"{}\"></script>", lib))
            .collect::<Vec<_>>()
            .join("\n    "),
        content_sections.join("\n\n        "),
        generate_chart_data_js(&context.metrics.chart_data)
    );

    Ok(html_content)
}

// Legacy function removed in favor of generate_html_report_v3 (Phase 4.3 Template System)

// === Phase 4.3: Template Section Renderers ===

/// Render header section
fn render_header_section(context: &TemplateContext) -> String {
    format!(
        r#"<div class="header">
            <h1>📊 Trading Report</h1>
            <p>{} - {} | {} Algorithm</p>
        </div>"#,
        context.data.config.start_date.format("%Y-%m-%d"),
        context.data.config.end_date.format("%Y-%m-%d"),
        context.data.config.algorithm
    )
}

/// Render performance summary section
fn render_performance_summary_section(context: &TemplateContext) -> String {
    let performance_class = if context.data.performance.total_return_pct >= 0.0 {
        "positive"
    } else {
        "negative"
    };

    format!(
        r#"<div class="content">
            <div class="section">
                <h2>📈 Performance Summary</h2>
                <div class="metrics-grid">
                    <div class="metric-card">
                        <div class="metric-label">Initial Capital</div>
                        <div class="metric-value">{}</div>
                    </div>
                    <div class="metric-card">
                        <div class="metric-label">Final Value</div>
                        <div class="metric-value">{}</div>
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
            </div>"#,
        format_currency_value(
            context.data.config.initial_capital,
            &context.config.currency
        ),
        format_currency_value(context.data.config.final_value, &context.config.currency),
        performance_class,
        context.data.performance.total_return_pct,
        context.data.performance.max_drawdown_pct,
        context.data.performance.sharpe_ratio,
        context.data.performance.volatility * 100.0
    )
}

/// Render portfolio chart section
fn render_portfolio_chart_section(_context: &TemplateContext) -> String {
    r#"<div class="section">
                <h2>📊 Portfolio Value Over Time</h2>
                <div class="chart-container">
                    <canvas id="portfolioChart"></canvas>
                </div>
            </div>"#
        .to_string()
}

/// Render trading activity section
fn render_trading_activity_section(context: &TemplateContext) -> String {
    format!(
        r#"<div class="section">
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
                        <div class="metric-value">{}</div>
                    </div>
                </div>
            </div>"#,
        context.data.performance.total_trades,
        context.data.performance.win_rate * 100.0,
        context.data.performance.active_trading_days,
        context.data.performance.simulation_days,
        format_currency_value(
            context.data.performance.total_costs,
            &context.config.currency
        )
    )
}

/// Render trade history section
fn render_trade_history_section(context: &TemplateContext) -> String {
    format!(
        r#"<div class="section">
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
        </div>"#,
        context.metrics.trades_html
    )
}

/// Render risk analysis section (optional, for extended metrics)
fn render_risk_analysis_section(context: &TemplateContext) -> String {
    if let Some(extended) = &context.extended_metrics {
        format!(
            r#"<div class="section">
                <h2>⚠️ Risk Analysis</h2>
                <div class="metrics-grid">
                    <div class="metric-card">
                        <div class="metric-label">Value at Risk (95%)</div>
                        <div class="metric-value">{:.2}%</div>
                    </div>
                    <div class="metric-card">
                        <div class="metric-label">Expected Shortfall</div>
                        <div class="metric-value">{:.2}%</div>
                    </div>
                    <div class="metric-card">
                        <div class="metric-label">Max Consecutive Losses</div>
                        <div class="metric-value">{}</div>
                    </div>
                    <div class="metric-card">
                        <div class="metric-label">Largest Win</div>
                        <div class="metric-value">{}</div>
                    </div>
                </div>
            </div>"#,
            extended.risk_metrics.value_at_risk_95 * 100.0,
            extended.risk_metrics.expected_shortfall * 100.0,
            extended.trade_analysis.consecutive_losses,
            format_currency_value(
                extended.trade_analysis.largest_win,
                &context.config.currency
            )
        )
    } else {
        String::new()
    }
}

/// Render footer section
fn render_footer_section(context: &TemplateContext) -> String {
    format!(
        r#"<div class="footer">
            <p>Generated at {} | CLI Tokens Trading Simulator</p>
        </div>"#,
        context.metrics.generation_timestamp
    )
}

/// Get default CSS style for HTML template
fn get_default_css_style() -> String {
    r#"
        * {
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
            line-height: 1.6;
            color: #333;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            min-height: 100vh;
            padding: 20px;
        }
        .container {
            max-width: 1200px;
            margin: 0 auto;
            background: white;
            border-radius: 20px;
            box-shadow: 0 20px 60px rgba(0,0,0,0.3);
            overflow: hidden;
        }
        .header {
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            padding: 40px;
            text-align: center;
        }
        .header h1 {
            font-size: 2.5em;
            margin-bottom: 10px;
        }
        .header p {
            font-size: 1.1em;
            opacity: 0.9;
        }
        .content {
            padding: 40px;
        }
        .section {
            margin-bottom: 40px;
        }
        .section h2 {
            color: #667eea;
            margin-bottom: 20px;
            padding-bottom: 10px;
            border-bottom: 2px solid #f0f0f0;
        }
        .metrics-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
            gap: 20px;
            margin-top: 20px;
        }
        .metric-card {
            background: #f8f9fa;
            padding: 20px;
            border-radius: 10px;
            border-left: 4px solid #667eea;
        }
        .metric-label {
            color: #666;
            font-size: 0.9em;
            margin-bottom: 5px;
        }
        .metric-value {
            font-size: 1.5em;
            font-weight: bold;
            color: #333;
        }
        .positive {
            color: #28a745;
        }
        .negative {
            color: #dc3545;
        }
        .chart-container {
            margin-top: 20px;
            padding: 20px;
            background: #f8f9fa;
            border-radius: 10px;
        }
        canvas {
            max-width: 100%;
            height: auto;
        }
        table {
            width: 100%;
            border-collapse: collapse;
            margin-top: 20px;
        }
        th, td {
            padding: 12px;
            text-align: left;
            border-bottom: 1px solid #e0e0e0;
        }
        th {
            background: #f8f9fa;
            font-weight: 600;
            color: #666;
        }
        tr:hover {
            background: #f8f9fa;
        }
        .footer {
            background: #f8f9fa;
            padding: 20px;
            text-align: center;
            color: #666;
            font-size: 0.9em;
        }
    "#.to_string()
}
