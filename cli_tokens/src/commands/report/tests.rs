use chrono::Utc;
use std::collections::HashMap;

use super::*;
use crate::commands::simulate::{
    AlgorithmType, DataQualityStats, ExecutionSummary, NearValueF64,
    PerformanceMetrics as SimPerformanceMetrics, SimulationResult, SimulationSummary,
    TokenAmountF64, TokenPriceF64, TradeExecution, TradingCost,
};
use bigdecimal::BigDecimal;

#[cfg(test)]
mod unit_tests {
    use super::*;

    pub fn create_test_simulation_result() -> SimulationResult {
        SimulationResult {
            config: SimulationSummary {
                start_date: Utc::now(),
                end_date: Utc::now(),
                algorithm: AlgorithmType::Momentum,
                initial_capital: 1000.0,
                final_value: 1150.0,
                duration_days: 10,
                total_return: 150.0,
            },
            performance: SimPerformanceMetrics {
                total_return: 0.15,
                annualized_return: 5.475,
                total_return_pct: 15.0,
                volatility: 0.2,
                max_drawdown: -0.05,
                max_drawdown_pct: -5.0,
                sharpe_ratio: 0.75,
                sortino_ratio: 0.85,
                total_trades: 5,
                winning_trades: 3,
                losing_trades: 2,
                win_rate: 0.6,
                profit_factor: 1.5,
                total_costs: 15.0,
                cost_ratio: 1.3,
                simulation_days: 10,
                active_trading_days: 8,
            },
            trades: vec![TradeExecution {
                timestamp: Utc::now(),
                from_token: "token_a".to_string(),
                to_token: "token_b".to_string(),
                amount: TokenAmountF64::from_smallest_units(100.0, 24),
                executed_price: TokenPriceF64::from_near_per_token(1.5),
                cost: TradingCost {
                    protocol_fee: BigDecimal::from(3),
                    slippage: BigDecimal::from(2),
                    gas_fee: BigDecimal::from(1),
                    total: BigDecimal::from(6),
                },
                portfolio_value_before: NearValueF64::from_near(1000.0),
                portfolio_value_after: NearValueF64::from_near(1020.0),
                success: true,
                reason: "Test trade".to_string(),
            }],
            portfolio_values: vec![
                PortfolioValue {
                    timestamp: Utc::now(),
                    holdings: HashMap::new(),
                    total_value: NearValueF64::from_near(1000.0),
                    cash_balance: NearValueF64::from_near(1000.0),
                    unrealized_pnl: NearValueF64::zero(),
                },
                PortfolioValue {
                    timestamp: Utc::now(),
                    holdings: {
                        let mut h = HashMap::new();
                        h.insert("token_b".to_string(), NearValueF64::from_near(150.0));
                        h
                    },
                    total_value: NearValueF64::from_near(1150.0),
                    cash_balance: NearValueF64::from_near(850.0),
                    unrealized_pnl: NearValueF64::from_near(150.0),
                },
            ],
            execution_summary: ExecutionSummary {
                total_trades: 1,
                successful_trades: 1,
                failed_trades: 0,
                success_rate: 1.0,
                total_cost: 6.0,
                avg_cost_per_trade: 6.0,
            },
            data_quality: DataQualityStats {
                total_timesteps: 10,
                skipped_timesteps: 0,
                data_coverage_percentage: 100.0,
                longest_gap_hours: 0,
                gap_events: Vec::new(),
            },
        }
    }

    #[test]
    fn test_extract_report_data() {
        let simulation_result = create_test_simulation_result();
        let report_data = extract_report_data(&simulation_result);

        assert_eq!(report_data.config.initial_capital, 1000.0);
        assert_eq!(report_data.config.final_value, 1150.0);
        assert_eq!(report_data.config.algorithm, "Momentum");

        assert_eq!(report_data.performance.total_return_pct, 15.0);
        assert_eq!(report_data.performance.sharpe_ratio, 0.75);
        assert_eq!(report_data.performance.total_trades, 5);

        assert_eq!(report_data.trades.len(), 1);
        assert_eq!(report_data.portfolio_values.len(), 2);
    }

    #[test]
    fn test_calculate_report_metrics() {
        let simulation_result = create_test_simulation_result();
        let report_data = extract_report_data(&simulation_result);
        let metrics = calculate_report_metrics(&report_data);

        assert_eq!(metrics.performance_class, "positive");
        assert_eq!(metrics.currency_symbol, "wrap.near");
        assert!(metrics.trades_html.contains("token_a"));
        assert!(metrics.trades_html.contains("token_b"));
        assert!(metrics.generation_timestamp.contains("UTC"));

        // Chart data should have labels and values
        assert_eq!(metrics.chart_data.labels.len(), 2);
        assert_eq!(metrics.chart_data.values.len(), 2);
        assert_eq!(metrics.chart_data.values[0], 1000.0);
        assert_eq!(metrics.chart_data.values[1], 1150.0);
    }

    #[test]
    fn test_calculate_report_metrics_negative_return() {
        let mut simulation_result = create_test_simulation_result();
        // Make performance negative
        simulation_result.performance.total_return_pct = -10.0;
        simulation_result.config.final_value = 900.0;

        let report_data = extract_report_data(&simulation_result);
        let metrics = calculate_report_metrics(&report_data);

        assert_eq!(metrics.performance_class, "negative");
    }

    #[test]
    fn test_generate_portfolio_chart_data() {
        let portfolio_values = vec![
            PortfolioValue {
                timestamp: chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
                holdings: HashMap::new(),
                total_value: NearValueF64::from_near(1000.0),
                cash_balance: NearValueF64::from_near(1000.0),
                unrealized_pnl: NearValueF64::zero(),
            },
            PortfolioValue {
                timestamp: chrono::DateTime::parse_from_rfc3339("2024-01-02T00:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
                holdings: HashMap::new(),
                total_value: NearValueF64::from_near(1100.0),
                cash_balance: NearValueF64::from_near(1100.0),
                unrealized_pnl: NearValueF64::from_near(100.0),
            },
        ];

        let chart_data = generate_portfolio_chart_data(&portfolio_values);

        assert_eq!(chart_data.labels.len(), 2);
        assert_eq!(chart_data.values.len(), 2);
        assert_eq!(chart_data.labels[0], "'01/01'");
        assert_eq!(chart_data.labels[1], "'01/02'");
        assert_eq!(chart_data.values[0], 1000.0);
        assert_eq!(chart_data.values[1], 1100.0);
    }

    #[test]
    fn test_generate_trades_table_html_empty() {
        let trades = vec![];
        let html = generate_trades_table_html(&trades);

        assert!(html.contains("No trades executed"));
        assert!(html.contains("colspan=\"6\""));
    }

    #[test]
    fn test_generate_trades_table_html_with_trades() {
        let trades = vec![TradeExecution {
            timestamp: chrono::DateTime::parse_from_rfc3339("2024-01-01T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            from_token: "token_a".to_string(),
            to_token: "token_b".to_string(),
            amount: TokenAmountF64::from_smallest_units(100.0, 24),
            executed_price: TokenPriceF64::from_near_per_token(1.5),
            cost: TradingCost {
                protocol_fee: BigDecimal::from(3),
                slippage: BigDecimal::from(2),
                gas_fee: BigDecimal::from(1),
                total: BigDecimal::from(6),
            },
            portfolio_value_before: NearValueF64::from_near(1000.0),
            portfolio_value_after: NearValueF64::from_near(1020.0),
            success: true,
            reason: "Test trade".to_string(),
        }];

        let html = generate_trades_table_html(&trades);

        assert!(html.contains("token_a → token_b"));
        assert!(html.contains("2024-01-01 12:00"));
        // TokenAmountF64 の Display は "100 (decimals=24)" 形式
        assert!(html.contains("100"));
        assert!(html.contains("1.500000"));
        assert!(html.contains("6.0000"));
        assert!(html.contains("Test trade"));
    }

    #[test]
    fn test_generate_trades_table_html_limits_to_ten() {
        let trades: Vec<TradeExecution> = (0..15)
            .map(|i| TradeExecution {
                timestamp: Utc::now(),
                from_token: format!("token_{}", i),
                to_token: format!("token_{}", i + 1),
                amount: TokenAmountF64::from_smallest_units(100.0, 24),
                executed_price: TokenPriceF64::from_near_per_token(1.5),
                cost: TradingCost {
                    protocol_fee: BigDecimal::from(3),
                    slippage: BigDecimal::from(2),
                    gas_fee: BigDecimal::from(1),
                    total: BigDecimal::from(6),
                },
                portfolio_value_before: NearValueF64::from_near(1000.0),
                portfolio_value_after: NearValueF64::from_near(1020.0),
                success: true,
                reason: "Test trade".to_string(),
            })
            .collect();

        let html = generate_trades_table_html(&trades);

        // Should only show recent 10 trades (reversed order)
        let row_count = html.matches("<tr>").count();
        assert_eq!(row_count, 10);

        // Should show most recent trades first (token_14 → token_15)
        assert!(html.contains("token_14 → token_15"));
        // Should not show oldest trades (token_0 → token_1)
        assert!(!html.contains("token_0 → token_1"));
    }

    #[test]
    fn test_generate_chart_data_js() {
        let chart_data = ChartData {
            labels: vec!["'01/01'".to_string(), "'01/02'".to_string()],
            values: vec![1000.0, 1150.5],
        };

        let js = generate_chart_data_js(&chart_data);

        assert_eq!(js, "{labels: ['01/01','01/02'], values: [1000.00,1150.50]}");
    }

    #[test]
    fn test_determine_output_path_with_explicit_output() {
        let args = ReportArgs {
            input: PathBuf::from("/path/to/input.json"),
            output: Some(PathBuf::from("/custom/output.html")),
        };

        let output_path = determine_output_path(&args).unwrap();
        assert_eq!(output_path, PathBuf::from("/custom/output.html"));
    }

    #[test]
    fn test_determine_output_path_default() {
        let args = ReportArgs {
            input: PathBuf::from("/path/to/input.json"),
            output: None,
        };

        let output_path = determine_output_path(&args).unwrap();
        assert_eq!(output_path, PathBuf::from("/path/to/report.html"));
    }
}

#[cfg(test)]
mod phase_4_2_tests {
    use super::*;

    #[test]
    fn test_create_default_report_config() {
        let config = create_default_report_config();

        assert!(matches!(config.theme, ReportTheme::Default));
        assert_eq!(config.currency.symbol, "wrap.near");
        assert_eq!(config.currency.decimal_places, 2);
        assert!(matches!(config.currency.position, CurrencyPosition::After));

        assert!(matches!(config.chart_settings.chart_type, ChartType::Line));
        assert!(!config.chart_settings.show_volume);
        assert!(config.chart_settings.show_trades);

        assert_eq!(config.display_options.max_trades_displayed, 10);
        assert!(config.display_options.show_detailed_trades);
        assert!(config.display_options.show_risk_metrics);
    }

    #[test]
    fn test_calculate_extended_metrics() {
        let simulation_result = unit_tests::create_test_simulation_result();
        let report_data = extract_report_data(&simulation_result);
        let extended_metrics = calculate_extended_metrics(&report_data);

        // Risk metrics should be calculated (VaR can be positive for profitable portfolios)
        assert!(extended_metrics.risk_metrics.value_at_risk_95.is_finite()); // VaR should be a valid number
        assert!(
            extended_metrics.risk_metrics.expected_shortfall
                <= extended_metrics.risk_metrics.value_at_risk_95
        );

        // Trade analysis should process the trade
        assert!(extended_metrics.trade_analysis.largest_win >= 0.0);
        assert!(extended_metrics.trade_analysis.trade_frequency_per_day > 0.0);

        // Performance comparison should be None (no benchmark)
        assert!(extended_metrics.performance_comparison.is_none());
    }

    #[test]
    fn test_analyze_trades_empty() {
        let trades = vec![];
        let analysis = analyze_trades(&trades);

        assert_eq!(analysis.average_trade_duration, 0.0);
        assert_eq!(analysis.largest_win, 0.0);
        assert_eq!(analysis.largest_loss, 0.0);
        assert_eq!(analysis.consecutive_wins, 0);
        assert_eq!(analysis.consecutive_losses, 0);
        assert_eq!(analysis.trade_frequency_per_day, 0.0);
    }

    #[test]
    fn test_analyze_trades_with_data() {
        let trades = vec![
            TradeExecution {
                timestamp: Utc::now(),
                from_token: "token_a".to_string(),
                to_token: "token_b".to_string(),
                amount: TokenAmountF64::from_smallest_units(100.0, 24),
                executed_price: TokenPriceF64::from_near_per_token(1.5),
                cost: TradingCost {
                    protocol_fee: BigDecimal::from(3),
                    slippage: BigDecimal::from(2),
                    gas_fee: BigDecimal::from(1),
                    total: BigDecimal::from(6),
                },
                portfolio_value_before: NearValueF64::from_near(1000.0),
                portfolio_value_after: NearValueF64::from_near(1020.0), // +20 profit
                success: true,
                reason: "Profitable trade".to_string(),
            },
            TradeExecution {
                timestamp: Utc::now(),
                from_token: "token_b".to_string(),
                to_token: "token_c".to_string(),
                amount: TokenAmountF64::from_smallest_units(120.0, 24),
                executed_price: TokenPriceF64::from_near_per_token(0.8),
                cost: TradingCost {
                    protocol_fee: BigDecimal::from(4),
                    slippage: BigDecimal::from(3),
                    gas_fee: BigDecimal::from(1),
                    total: BigDecimal::from(8),
                },
                portfolio_value_before: NearValueF64::from_near(1020.0),
                portfolio_value_after: NearValueF64::from_near(990.0), // -30 loss
                success: true,
                reason: "Losing trade".to_string(),
            },
        ];

        let analysis = analyze_trades(&trades);

        assert_eq!(analysis.largest_win, 20.0);
        assert_eq!(analysis.largest_loss, -30.0);
        assert_eq!(analysis.consecutive_wins, 1);
        assert_eq!(analysis.consecutive_losses, 1);
        assert!(analysis.trade_frequency_per_day > 0.0);
    }

    #[test]
    fn test_format_currency_value_before() {
        let config = CurrencyConfig {
            symbol: "USD".to_string(),
            decimal_places: 2,
            position: CurrencyPosition::Before,
        };

        let formatted = format_currency_value(1234.567, &config);
        assert_eq!(formatted, "USD 1234.57");
    }

    #[test]
    fn test_format_currency_value_after() {
        let config = CurrencyConfig {
            symbol: "EUR".to_string(),
            decimal_places: 3,
            position: CurrencyPosition::After,
        };

        let formatted = format_currency_value(1234.567, &config);
        assert_eq!(formatted, "1234.567 EUR");
    }

    #[test]
    fn test_calculate_var() {
        let returns = vec![-0.1, -0.05, 0.0, 0.05, 0.1, 0.15];

        let var_95 = calculate_var(&returns, 0.95);
        assert!(var_95 <= -0.05); // Should be in the lower tail

        let var_99 = calculate_var(&returns, 0.99);
        assert!(var_99 <= var_95); // 99% VaR should be more extreme
    }

    #[test]
    fn test_calculate_expected_shortfall() {
        let returns = vec![-0.1, -0.05, 0.0, 0.05, 0.1];

        let es = calculate_expected_shortfall(&returns, 0.8);
        assert!(es <= 0.0); // Expected shortfall should be negative
    }

    #[test]
    fn test_consecutive_wins_calculation() {
        let trade_pnls = vec![10.0, 20.0, -5.0, 15.0, 25.0, 5.0, -10.0];
        let consecutive_wins = calculate_max_consecutive_wins(&trade_pnls);
        assert_eq!(consecutive_wins, 3); // trades at positions 3, 4, 5
    }

    #[test]
    fn test_consecutive_losses_calculation() {
        let trade_pnls = vec![10.0, -5.0, -10.0, -2.0, 15.0, -8.0];
        let consecutive_losses = calculate_max_consecutive_losses(&trade_pnls);
        assert_eq!(consecutive_losses, 3); // trades at positions 1, 2, 3
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_generate_html_report_v2() {
        let simulation_result = unit_tests::create_test_simulation_result();
        let report_data = extract_report_data(&simulation_result);
        let metrics = calculate_report_metrics(&report_data);

        let html = generate_html_report_v3(&report_data, &metrics, None, None).unwrap();

        // Basic HTML structure
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<html lang=\"ja\">"));
        assert!(html.contains("Trading Simulation Report"));

        // Performance data
        assert!(html.contains("1000.00 wrap.near")); // Initial capital
        assert!(html.contains("1150.00 wrap.near")); // Final value
        assert!(html.contains("Momentum Algorithm"));

        // Metrics
        assert!(html.contains("15.00%")); // Total return
        assert!(html.contains("0.75")); // Sharpe ratio
        assert!(html.contains("20.00%")); // Volatility

        // Trading activity
        assert!(html.contains("60.0%")); // Win rate
        assert!(html.contains("8 / 10")); // Active days

        // Chart and JavaScript
        assert!(html.contains("chart.js")); // Case insensitive match
        assert!(html.contains("portfolioChart"));
        assert!(html.contains("{labels: ["));

        // Footer
        assert!(html.contains("CLI Tokens Trading Simulator"));
    }

    #[test]
    fn test_full_report_generation_pipeline() {
        let temp_dir = TempDir::new().unwrap();
        let input_path = temp_dir.path().join("simulation_result.json");
        let output_path = temp_dir.path().join("report.html");

        // Create test simulation result
        let simulation_result = unit_tests::create_test_simulation_result();
        let json_content = serde_json::to_string_pretty(&simulation_result).unwrap();
        fs::write(&input_path, json_content).unwrap();

        // Create report args
        let args = ReportArgs {
            input: input_path,
            output: Some(output_path.clone()),
        };

        // Run report generation
        let result = run_report(args);
        assert!(result.is_ok(), "Report generation failed: {:?}", result);

        // Verify output file was created
        assert!(output_path.exists(), "Output file was not created");

        // Verify HTML content
        let html_content = fs::read_to_string(output_path).unwrap();
        assert!(html_content.contains("Trading Simulation Report"));
        assert!(html_content.contains("Momentum"));
        assert!(html_content.len() > 1000, "HTML content seems too short");
    }
}

// === Phase 4.3: Template System Tests ===

#[cfg(test)]
mod phase_4_3_tests {
    use super::super::*;
    use super::unit_tests::create_test_simulation_result;
    use super::*;

    #[test]
    fn test_create_default_html_template() {
        let template = create_default_html_template();

        assert_eq!(template.template_name, "default");
        assert_eq!(template.title, "Trading Simulation Report");
        assert!(!template.css_style.is_empty());
        assert!(
            template
                .javascript_libs
                .contains(&"https://cdn.jsdelivr.net/npm/chart.js".to_string())
        );
    }

    #[test]
    fn test_create_template_context() {
        let result = create_test_simulation_result();
        let data = extract_report_data(&result);
        let metrics = calculate_report_metrics(&data);
        let extended_metrics = calculate_extended_metrics(&data);

        let context = create_template_context(&data, &metrics, None, Some(extended_metrics));

        assert_eq!(context.data.config.algorithm, "Momentum");
        assert!(context.extended_metrics.is_some());
        assert!(matches!(context.config.theme, ReportTheme::Default));
    }

    #[test]
    fn test_render_header_section() {
        let result = create_test_simulation_result();
        let data = extract_report_data(&result);
        let metrics = calculate_report_metrics(&data);
        let context = create_template_context(&data, &metrics, None, None);

        let header_html = render_header_section(&context);

        assert!(header_html.contains("<div class=\"header\">"));
        assert!(header_html.contains("Trading Report"));
        assert!(header_html.contains("Momentum Algorithm"));
    }

    #[test]
    fn test_render_performance_summary_section() {
        let result = create_test_simulation_result();
        let data = extract_report_data(&result);
        let metrics = calculate_report_metrics(&data);
        let context = create_template_context(&data, &metrics, None, None);

        let performance_html = render_performance_summary_section(&context);

        assert!(performance_html.contains("📈 Performance Summary"));
        assert!(performance_html.contains("Initial Capital"));
        assert!(performance_html.contains("Final Value"));
        assert!(performance_html.contains("Total Return"));
        assert!(performance_html.contains("Sharpe Ratio"));
    }

    #[test]
    fn test_render_portfolio_chart_section() {
        let result = create_test_simulation_result();
        let data = extract_report_data(&result);
        let metrics = calculate_report_metrics(&data);
        let context = create_template_context(&data, &metrics, None, None);

        let chart_html = render_portfolio_chart_section(&context);

        assert!(chart_html.contains("📊 Portfolio Value Over Time"));
        assert!(chart_html.contains("<canvas id=\"portfolioChart\"></canvas>"));
    }

    #[test]
    fn test_render_trading_activity_section() {
        let result = create_test_simulation_result();
        let data = extract_report_data(&result);
        let metrics = calculate_report_metrics(&data);
        let context = create_template_context(&data, &metrics, None, None);

        let activity_html = render_trading_activity_section(&context);

        assert!(activity_html.contains("🔄 Trading Activity"));
        assert!(activity_html.contains("Total Trades"));
        assert!(activity_html.contains("Win Rate"));
        assert!(activity_html.contains("Active Days"));
    }

    #[test]
    fn test_render_risk_analysis_section_with_extended_metrics() {
        let result = create_test_simulation_result();
        let data = extract_report_data(&result);
        let metrics = calculate_report_metrics(&data);
        let extended_metrics = calculate_extended_metrics(&data);
        let context = create_template_context(&data, &metrics, None, Some(extended_metrics));

        let risk_html = render_risk_analysis_section(&context);

        assert!(risk_html.contains("⚠️ Risk Analysis"));
        assert!(risk_html.contains("Value at Risk"));
        assert!(risk_html.contains("Expected Shortfall"));
        assert!(risk_html.contains("Max Consecutive Losses"));
    }

    #[test]
    fn test_render_risk_analysis_section_without_extended_metrics() {
        let result = create_test_simulation_result();
        let data = extract_report_data(&result);
        let metrics = calculate_report_metrics(&data);
        let context = create_template_context(&data, &metrics, None, None);

        let risk_html = render_risk_analysis_section(&context);

        assert!(
            risk_html.is_empty(),
            "Risk section should be empty without extended metrics"
        );
    }

    #[test]
    fn test_render_footer_section() {
        let result = create_test_simulation_result();
        let data = extract_report_data(&result);
        let metrics = calculate_report_metrics(&data);
        let context = create_template_context(&data, &metrics, None, None);

        let footer_html = render_footer_section(&context);

        assert!(footer_html.contains("<div class=\"footer\">"));
        assert!(footer_html.contains("CLI Tokens Trading Simulator"));
        assert!(footer_html.contains("Generated at"));
    }

    #[test]
    fn test_generate_html_report_v3() {
        let result = create_test_simulation_result();
        let data = extract_report_data(&result);
        let metrics = calculate_report_metrics(&data);

        let html_result = generate_html_report_v3(&data, &metrics, None, None);

        assert!(
            html_result.is_ok(),
            "HTML generation failed: {:?}",
            html_result
        );

        let html_content = html_result.unwrap();
        assert!(html_content.contains("<!DOCTYPE html>"));
        assert!(html_content.contains("Trading Simulation Report"));
        assert!(html_content.contains("📈 Performance Summary"));
        assert!(html_content.contains("📊 Portfolio Value Over Time"));
        assert!(html_content.contains("🔄 Trading Activity"));
        assert!(html_content.contains("📝 Recent Trades"));
        assert!(html_content.contains("CLI Tokens Trading Simulator"));
    }

    #[test]
    fn test_get_default_css_style() {
        let css = get_default_css_style();

        assert!(!css.is_empty());
        assert!(css.contains("body"));
        assert!(css.contains(".container"));
        assert!(css.contains(".header"));
        assert!(css.contains(".metrics-grid"));
    }
}
