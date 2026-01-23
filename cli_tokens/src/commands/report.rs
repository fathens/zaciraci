use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::Args;
use std::path::PathBuf;

use crate::commands::simulate::{
    MultiAlgorithmSimulationResult, PortfolioValue, SimulationResult, TradeExecution,
};

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
            // NearValueF64 から f64 への変換（レポート表示用）
            initial_capital: result.config.initial_capital.as_f64(),
            final_value: result.config.final_value.as_f64(),
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
            // NearValueF64 から f64 への変換（レポート表示用）
            total_costs: result.performance.total_costs.as_f64(),
        },
        trades: result.trades.clone(),
        portfolio_values: result.portfolio_values.clone(),
    }
}

/// Calculate derived metrics for report generation
pub fn calculate_report_metrics(data: &ReportData) -> ReportMetrics {
    let default_config = create_default_report_config();
    let performance_class = if data.performance.total_return_pct >= 0.0 {
        "positive".to_string()
    } else {
        "negative".to_string()
    };

    ReportMetrics {
        performance_class,
        currency_symbol: default_config.currency.symbol.clone(),
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

    let chart_values: Vec<f64> = values.iter().map(|pv| pv.total_value.as_f64()).collect();

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
            // コストを NEAR 単位で表示（yoctoNEAR から変換）
            let cost_near = trade.cost.total.to_near().as_f64();
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
                cost_near,
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
        .map(|t| t.portfolio_value_after.as_f64() - t.portfolio_value_before.as_f64())
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
            let prev_value = window[0].total_value.as_f64();
            let curr_value = window[1].total_value.as_f64();
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

    /// Output file path (optional, defaults to same directory as input)
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

pub fn run_report(args: ReportArgs) -> Result<()> {
    // JSONファイルを読み込み
    let json_content = std::fs::read_to_string(&args.input)
        .with_context(|| format!("Failed to read input file: {}", args.input.display()))?;

    // 複数アルゴリズムの結果かどうかを判定して処理
    if let Ok(multi_result) = serde_json::from_str::<MultiAlgorithmSimulationResult>(&json_content)
    {
        // 複数アルゴリズムの場合
        run_multi_algorithm_report(args, &multi_result)
    } else if let Ok(simulation_result) = serde_json::from_str::<SimulationResult>(&json_content) {
        // 単一アルゴリズムの場合
        run_single_algorithm_report(args, &simulation_result)
    } else {
        Err(anyhow::anyhow!(
            "Failed to parse JSON file: {}. Expected SimulationResult or MultiAlgorithmSimulationResult",
            args.input.display()
        ))
    }
}

/// 単一アルゴリズムレポートを生成
fn run_single_algorithm_report(
    args: ReportArgs,
    simulation_result: &SimulationResult,
) -> Result<()> {
    // Phase 4.1: Use pure functions for data extraction and processing
    let report_data = extract_report_data(simulation_result);
    let report_metrics = calculate_report_metrics(&report_data);

    // 出力先を決定
    let output_path = determine_output_path(&args)?;

    // Generate HTML report
    let html_content = generate_html_report_v3(&report_data, &report_metrics, None, None)?;
    std::fs::write(&output_path, html_content)
        .with_context(|| format!("Failed to write HTML file: {}", output_path.display()))?;
    println!("📊 HTML report saved to: {}", output_path.display());
    Ok(())
}

/// 複数アルゴリズム比較レポートを生成
fn run_multi_algorithm_report(
    args: ReportArgs,
    multi_result: &MultiAlgorithmSimulationResult,
) -> Result<()> {
    // 出力先を決定
    let output_path = determine_output_path(&args)?;

    // Generate HTML report
    let html_content = generate_multi_algorithm_html_report(multi_result)?;
    std::fs::write(&output_path, html_content)
        .with_context(|| format!("Failed to write HTML file: {}", output_path.display()))?;
    println!(
        "📊 Multi-algorithm comparison report saved to: {}",
        output_path.display()
    );
    Ok(())
}

/// Determine output file path based on args
fn determine_output_path(args: &ReportArgs) -> Result<PathBuf> {
    if let Some(output) = &args.output {
        Ok(output.clone())
    } else {
        // 入力ファイルと同じディレクトリにHTMLレポートを作成
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

/// Generate HTML report for multiple algorithms comparison
fn generate_multi_algorithm_html_report(
    multi_result: &MultiAlgorithmSimulationResult,
) -> Result<String> {
    let start_date = multi_result.results.first().unwrap().config.start_date;
    let end_date = multi_result.results.first().unwrap().config.end_date;

    // 比較テーブルのHTML生成
    let comparison_table = generate_comparison_table_html(&multi_result.comparison);

    // 個別結果のHTML生成
    let individual_reports = multi_result
        .results
        .iter()
        .map(generate_individual_algorithm_section)
        .collect::<Result<Vec<_>>>()?
        .join("\n");

    // Chart.jsスクリプトの生成
    let individual_chart_scripts = multi_result
        .results
        .iter()
        .map(generate_chart_script)
        .collect::<Result<Vec<_>>>()?
        .join("\n");

    // 比較チャートスクリプトの生成
    let comparison_chart_script = generate_comparison_chart_script(&multi_result.results)?;

    let css = get_default_css_style();
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="ja">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Multi-Algorithm Trading Simulation Report</title>
    <style>
        {}
        .comparison-section {{
            margin: 20px 0;
            padding: 20px;
            background: #f8f9fa;
            border-radius: 8px;
        }}
        .algorithm-section {{
            margin: 30px 0;
            padding: 20px;
            border: 1px solid #dee2e6;
            border-radius: 8px;
        }}
        .comparison-table {{
            width: 100%;
            border-collapse: collapse;
            margin: 15px 0;
        }}
        .comparison-table th, .comparison-table td {{
            padding: 12px;
            text-align: left;
            border-bottom: 1px solid #dee2e6;
        }}
        .comparison-table th {{
            background-color: #e9ecef;
            font-weight: 600;
        }}
        .best-performer {{
            background-color: #d4edda !important;
            font-weight: bold;
        }}
        .chart-container {{
            margin: 20px 0;
            height: 400px;
        }}
        .comparison-chart-section {{
            margin: 30px 0;
            padding: 20px;
            background: #f8f9fa;
            border-radius: 8px;
        }}
        .comparison-chart-section h2 {{
            color: #495057;
            margin-bottom: 20px;
        }}
    </style>
    <script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>🔄 Multi-Algorithm Trading Simulation Report</h1>
            <p>Comparison of trading algorithms performance</p>
            <div class="period">
                <strong>Period:</strong> {} to {}
            </div>
        </div>

        <div class="comparison-section">
            <h2>🏆 Algorithm Performance Comparison</h2>
            <div class="highlights">
                <div class="highlight">
                    <h3>Best Total Return</h3>
                    <p>{:?}: <strong>{:.2}%</strong></p>
                </div>
                <div class="highlight">
                    <h3>Best Sharpe Ratio</h3>
                    <p>{:?}: <strong>{:.4}</strong></p>
                </div>
                <div class="highlight">
                    <h3>Lowest Drawdown</h3>
                    <p>{:?}: <strong>{:.2}%</strong></p>
                </div>
            </div>
            
            <h3>📊 Performance Summary Table</h3>
            {comparison_table}
        </div>

        <div class="comparison-chart-section">
            <h2>📈 Algorithm Performance Comparison Chart</h2>
            <div class="chart-container">
                <canvas id="comparison_chart"></canvas>
            </div>
        </div>

        <div class="individual-results">
            <h2>📊 Individual Algorithm Details</h2>
            {individual_reports}
        </div>

        <div class="footer">
            <p>Generated on {timestamp} by CLI Tokens Multi-Algorithm Simulator</p>
        </div>
    </div>
    
    <script>
        // Initialize charts for each algorithm
        document.addEventListener('DOMContentLoaded', function() {{
            // Comparison chart
            {comparison_chart_script}
            
            // Individual algorithm charts
            {individual_chart_scripts}
        }});
    </script>
</body>
</html>"#,
        css,
        start_date.format("%Y-%m-%d"),
        end_date.format("%Y-%m-%d"),
        multi_result.comparison.best_return.0,
        multi_result.comparison.best_return.1,
        multi_result.comparison.best_sharpe.0,
        multi_result.comparison.best_sharpe.1,
        multi_result.comparison.lowest_drawdown.0,
        multi_result.comparison.lowest_drawdown.1,
        comparison_table = comparison_table,
        individual_reports = individual_reports,
        comparison_chart_script = comparison_chart_script,
        individual_chart_scripts = individual_chart_scripts,
        timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    );

    Ok(html)
}

/// Generate comparison table HTML
fn generate_comparison_table_html(
    comparison: &crate::commands::simulate::AlgorithmComparison,
) -> String {
    let mut table_rows = String::new();

    for row in &comparison.summary_table {
        let is_best_return =
            (row.algorithm.clone(), row.total_return_pct) == comparison.best_return;
        let is_best_sharpe = (row.algorithm.clone(), row.sharpe_ratio) == comparison.best_sharpe;
        let is_best_drawdown =
            (row.algorithm.clone(), row.max_drawdown_pct) == comparison.lowest_drawdown;

        let row_class = if is_best_return || is_best_sharpe || is_best_drawdown {
            "best-performer"
        } else {
            ""
        };

        table_rows.push_str(&format!(
            r#"
            <tr class="{}">
                <td>{:?}</td>
                <td>{:.2}%</td>
                <td>{:.2}%</td>
                <td>{:.4}</td>
                <td>{:.2}%</td>
                <td>{}</td>
                <td>{:.1}%</td>
            </tr>"#,
            row_class,
            row.algorithm,
            row.total_return_pct,
            row.annualized_return * 100.0,
            row.sharpe_ratio,
            row.max_drawdown_pct,
            row.total_trades,
            row.win_rate * 100.0
        ));
    }

    format!(
        r#"
        <table class="comparison-table">
            <thead>
                <tr>
                    <th>Algorithm</th>
                    <th>Total Return</th>
                    <th>Annual Return</th>
                    <th>Sharpe Ratio</th>
                    <th>Max Drawdown</th>
                    <th>Total Trades</th>
                    <th>Win Rate</th>
                </tr>
            </thead>
            <tbody>
                {table_rows}
            </tbody>
        </table>"#,
        table_rows = table_rows
    )
}

/// Generate individual algorithm section HTML
fn generate_individual_algorithm_section(result: &SimulationResult) -> Result<String> {
    let report_data = extract_report_data(result);
    let report_metrics = calculate_report_metrics(&report_data);

    Ok(format!(
        r#"
        <div class="algorithm-section">
            <h3>📈 {:?} Algorithm</h3>
            <div class="metrics-grid">
                <div class="metric">
                    <h4>Total Return</h4>
                    <p class="{}">{:.2}%</p>
                </div>
                <div class="metric">
                    <h4>Sharpe Ratio</h4>
                    <p>{:.4}</p>
                </div>
                <div class="metric">
                    <h4>Max Drawdown</h4>
                    <p>{:.2}%</p>
                </div>
                <div class="metric">
                    <h4>Total Trades</h4>
                    <p>{}</p>
                </div>
                <div class="metric">
                    <h4>Win Rate</h4>
                    <p>{:.1}%</p>
                </div>
                <div class="metric">
                    <h4>Final Value</h4>
                    <p>{:.2} {}</p>
                </div>
            </div>
            
            <div class="chart-container">
                <canvas id="chart_{}"></canvas>
            </div>
        </div>"#,
        result.config.algorithm,
        report_metrics.performance_class,
        report_data.performance.total_return_pct,
        report_data.performance.sharpe_ratio,
        report_data.performance.max_drawdown_pct,
        report_data.performance.total_trades,
        report_data.performance.win_rate * 100.0,
        result.config.final_value,
        create_default_report_config().currency.symbol,
        format!("{:?}", result.config.algorithm).to_lowercase()
    ))
}

/// Generate Chart.js script for individual algorithm
fn generate_chart_script(result: &SimulationResult) -> Result<String> {
    let report_data = extract_report_data(result);
    let report_metrics = calculate_report_metrics(&report_data);

    let chart_data_json = serde_json::to_string(&report_metrics.chart_data.values)
        .unwrap_or_else(|_| "[]".to_string());
    let chart_labels_json = serde_json::to_string(&report_metrics.chart_data.labels)
        .unwrap_or_else(|_| "[]".to_string());

    let algorithm_name = format!("{:?}", result.config.algorithm).to_lowercase();

    Ok(format!(
        r#"
            const ctx_{} = document.getElementById('chart_{}').getContext('2d');
            new Chart(ctx_{}, {{
                type: 'line',
                data: {{
                    labels: {},
                    datasets: [{{
                        label: 'Portfolio Value',
                        data: {},
                        borderColor: 'rgb(75, 192, 192)',
                        backgroundColor: 'rgba(75, 192, 192, 0.1)',
                        tension: 0.1,
                        fill: true
                    }}]
                }},
                options: {{
                    responsive: true,
                    maintainAspectRatio: false,
                    plugins: {{
                        title: {{
                            display: true,
                            text: '{:?} Algorithm Performance'
                        }}
                    }},
                    scales: {{
                        y: {{
                            beginAtZero: false,
                            title: {{
                                display: true,
                                text: 'Portfolio Value (wrap.near)'
                            }}
                        }},
                        x: {{
                            title: {{
                                display: true,
                                text: 'Date'
                            }}
                        }}
                    }}
                }}
            }});"#,
        algorithm_name,
        algorithm_name,
        algorithm_name,
        chart_labels_json,
        chart_data_json,
        result.config.algorithm
    ))
}

/// Get algorithm color configuration for charts
fn get_algorithm_colors() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("Momentum", "rgb(255, 99, 132)", "rgba(255, 99, 132, 0.1)"),
        ("Portfolio", "rgb(54, 162, 235)", "rgba(54, 162, 235, 0.1)"),
        (
            "TrendFollowing",
            "rgb(75, 192, 192)",
            "rgba(75, 192, 192, 0.1)",
        ),
    ]
}

/// Create chart dataset for a single algorithm
fn create_chart_dataset(
    result: &SimulationResult,
    algorithm_index: usize,
    colors: &[(&str, &str, &str)],
) -> Result<String> {
    let report_data = extract_report_data(result);
    let report_metrics = calculate_report_metrics(&report_data);

    let algorithm_name = format!("{:?}", result.config.algorithm);
    let (_, border_color, background_color) = colors.get(algorithm_index).unwrap_or(&(
        "Unknown",
        "rgb(128, 128, 128)",
        "rgba(128, 128, 128, 0.1)",
    ));

    Ok(format!(
        r#"{{
        label: '{}',
        data: {},
        borderColor: '{}',
        backgroundColor: '{}',
        tension: 0.1,
        fill: false
    }}"#,
        algorithm_name,
        serde_json::to_string(&report_metrics.chart_data.values)
            .unwrap_or_else(|_| "[]".to_string()),
        border_color,
        background_color
    ))
}

/// Extract common labels from first simulation result
fn extract_common_labels(results: &[SimulationResult]) -> Vec<String> {
    if let Some(first_result) = results.first() {
        let report_data = extract_report_data(first_result);
        let report_metrics = calculate_report_metrics(&report_data);
        report_metrics.chart_data.labels
    } else {
        Vec::new()
    }
}

/// Generate Chart.js configuration for comparison chart
fn create_comparison_chart_config(labels_json: String, datasets: Vec<String>) -> String {
    format!(
        r#"
            const comparisonCtx = document.getElementById('comparison_chart').getContext('2d');
            new Chart(comparisonCtx, {{
                type: 'line',
                data: {{
                    labels: {},
                    datasets: [{}]
                }},
                options: {{
                    responsive: true,
                    maintainAspectRatio: false,
                    plugins: {{
                        title: {{
                            display: true,
                            text: 'Algorithm Performance Comparison',
                            font: {{
                                size: 16
                            }}
                        }},
                        legend: {{
                            display: true,
                            position: 'top'
                        }}
                    }},
                    scales: {{
                        y: {{
                            beginAtZero: false,
                            title: {{
                                display: true,
                                text: 'Portfolio Value (wrap.near)'
                            }}
                        }},
                        x: {{
                            title: {{
                                display: true,
                                text: 'Date'
                            }}
                        }}
                    }},
                    interaction: {{
                        mode: 'index',
                        intersect: false,
                    }},
                    hover: {{
                        mode: 'nearest',
                        intersect: true
                    }}
                }}
            }});"#,
        labels_json,
        datasets.join(",\n                ")
    )
}

/// Generate comparison chart script showing all algorithms overlaid
fn generate_comparison_chart_script(results: &[SimulationResult]) -> Result<String> {
    let colors = get_algorithm_colors();
    let common_labels = extract_common_labels(results);
    let labels_json = serde_json::to_string(&common_labels).unwrap_or_else(|_| "[]".to_string());

    let datasets = results
        .iter()
        .enumerate()
        .map(|(i, result)| create_chart_dataset(result, i, &colors))
        .collect::<Result<Vec<_>>>()?;

    Ok(create_comparison_chart_config(labels_json, datasets))
}

#[cfg(test)]
mod comparison_chart_tests;
