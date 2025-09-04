use chrono::Utc;
use std::collections::HashMap;

use super::*;
use crate::commands::simulate::{
    AlgorithmType, ExecutionSummary, PerformanceMetrics as SimPerformanceMetrics, SimulationResult,
    SimulationSummary, TradeExecution, TradingCost,
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
                amount: 100.0,
                executed_price: 1.5,
                cost: TradingCost {
                    protocol_fee: BigDecimal::from(3),
                    slippage: BigDecimal::from(2),
                    gas_fee: BigDecimal::from(1),
                    total: BigDecimal::from(6),
                },
                portfolio_value_before: 1000.0,
                portfolio_value_after: 1020.0,
                success: true,
                reason: "Test trade".to_string(),
            }],
            portfolio_values: vec![
                PortfolioValue {
                    timestamp: Utc::now(),
                    holdings: HashMap::new(),
                    total_value: 1000.0,
                    cash_balance: 1000.0,
                    unrealized_pnl: 0.0,
                },
                PortfolioValue {
                    timestamp: Utc::now(),
                    holdings: {
                        let mut h = HashMap::new();
                        h.insert("token_b".to_string(), 150.0);
                        h
                    },
                    total_value: 1150.0,
                    cash_balance: 850.0,
                    unrealized_pnl: 150.0,
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
                total_value: 1000.0,
                cash_balance: 1000.0,
                unrealized_pnl: 0.0,
            },
            PortfolioValue {
                timestamp: chrono::DateTime::parse_from_rfc3339("2024-01-02T00:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
                holdings: HashMap::new(),
                total_value: 1100.0,
                cash_balance: 1100.0,
                unrealized_pnl: 100.0,
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
            amount: 100.0,
            executed_price: 1.5,
            cost: TradingCost {
                protocol_fee: BigDecimal::from(3),
                slippage: BigDecimal::from(2),
                gas_fee: BigDecimal::from(1),
                total: BigDecimal::from(6),
            },
            portfolio_value_before: 1000.0,
            portfolio_value_after: 1020.0,
            success: true,
            reason: "Test trade".to_string(),
        }];

        let html = generate_trades_table_html(&trades);

        assert!(html.contains("token_a → token_b"));
        assert!(html.contains("2024-01-01 12:00"));
        assert!(html.contains("100.0000"));
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
                amount: 100.0,
                executed_price: 1.5,
                cost: TradingCost {
                    protocol_fee: BigDecimal::from(3),
                    slippage: BigDecimal::from(2),
                    gas_fee: BigDecimal::from(1),
                    total: BigDecimal::from(6),
                },
                portfolio_value_before: 1000.0,
                portfolio_value_after: 1020.0,
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
            format: "html".to_string(),
            output: Some(PathBuf::from("/custom/output.html")),
        };

        let output_path = determine_output_path(&args).unwrap();
        assert_eq!(output_path, PathBuf::from("/custom/output.html"));
    }

    #[test]
    fn test_determine_output_path_default() {
        let args = ReportArgs {
            input: PathBuf::from("/path/to/input.json"),
            format: "html".to_string(),
            output: None,
        };

        let output_path = determine_output_path(&args).unwrap();
        assert_eq!(output_path, PathBuf::from("/path/to/report.html"));
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

        let html = generate_html_report_v2(&report_data, &metrics).unwrap();

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
            format: "html".to_string(),
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
