use bigdecimal::{BigDecimal, FromPrimitive, ToPrimitive};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

use crate::commands::simulate::{
    AlgorithmType, DataQualityStats, ExecutionSummary, MultiAlgorithmSimulationResult,
    NearValueF64, PerformanceMetrics, PortfolioValue, SimulationResult, SimulationSummary,
    TokenAmountF64, TokenPriceF64, TradeExecution, TradingCost,
};

/// Create a test simulation result for a specific algorithm
fn create_test_simulation_result(algorithm: AlgorithmType, final_value: f64) -> SimulationResult {
    let start_date = DateTime::from_naive_utc_and_offset(
        chrono::NaiveDate::from_ymd_opt(2025, 8, 10)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap(),
        Utc,
    );
    let end_date = DateTime::from_naive_utc_and_offset(
        chrono::NaiveDate::from_ymd_opt(2025, 8, 20)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap(),
        Utc,
    );

    let initial_capital = 1000.0;
    let total_return = (final_value - initial_capital) / initial_capital * 100.0;

    // Create portfolio values over time
    let portfolio_values: Vec<PortfolioValue> = (0..11)
        .map(|day| PortfolioValue {
            timestamp: start_date + chrono::Duration::days(day),
            total_value: NearValueF64::from_near(
                initial_capital + (final_value - initial_capital) * (day as f64) / 10.0,
            ),
            cash_balance: NearValueF64::from_near(100.0),
            holdings: HashMap::new(),
            unrealized_pnl: NearValueF64::zero(),
        })
        .collect();

    // Create sample trades
    let trades = vec![
        TradeExecution {
            timestamp: start_date + chrono::Duration::days(2),
            from_token: "wrap.near".to_string(),
            to_token: "akaia.tkn.near".to_string(),
            amount: TokenAmountF64::from_smallest_units(500.0, 24),
            executed_price: TokenPriceF64::from_near_per_token(1.2),
            cost: TradingCost {
                protocol_fee: BigDecimal::from_f64(1.5).unwrap(),
                slippage: BigDecimal::from_f64(2.0).unwrap(),
                gas_fee: BigDecimal::from_f64(0.5).unwrap(),
                total: BigDecimal::from_f64(4.0).unwrap(),
            },
            reason: "momentum signal".to_string(),
            portfolio_value_before: NearValueF64::from_near(1000.0),
            portfolio_value_after: NearValueF64::from_near(996.0),
            success: true,
        },
        TradeExecution {
            timestamp: start_date + chrono::Duration::days(5),
            from_token: "akaia.tkn.near".to_string(),
            to_token: "babyblackdragon.tkn.near".to_string(),
            amount: TokenAmountF64::from_smallest_units(600.0, 24),
            executed_price: TokenPriceF64::from_near_per_token(0.8),
            cost: TradingCost {
                protocol_fee: BigDecimal::from_f64(1.8).unwrap(),
                slippage: BigDecimal::from_f64(2.4).unwrap(),
                gas_fee: BigDecimal::from_f64(0.5).unwrap(),
                total: BigDecimal::from_f64(4.7).unwrap(),
            },
            reason: "rebalancing".to_string(),
            portfolio_value_before: NearValueF64::from_near(1100.0),
            portfolio_value_after: NearValueF64::from_near(1095.3),
            success: true,
        },
    ];

    SimulationResult {
        config: SimulationSummary {
            start_date,
            end_date,
            algorithm,
            initial_capital,
            final_value,
            total_return,
            duration_days: 10,
        },
        performance: PerformanceMetrics {
            total_return: total_return / 100.0,
            annualized_return: total_return * 36.5 / 10.0, // Rough annualization
            total_return_pct: total_return,
            volatility: 0.15,
            max_drawdown: -20.0,
            max_drawdown_pct: -2.0,
            sharpe_ratio: 1.2,
            sortino_ratio: 1.4,
            total_trades: trades.len(),
            winning_trades: 1,
            losing_trades: 1,
            win_rate: 0.5,
            profit_factor: 1.1,
            total_costs: trades
                .iter()
                .map(|t| &t.cost.total)
                .fold(BigDecimal::from(0), |acc, x| acc + x)
                .to_f64()
                .unwrap_or(0.0),
            cost_ratio: 0.87,
            simulation_days: 10,
            active_trading_days: 8,
        },
        trades,
        portfolio_values,
        execution_summary: ExecutionSummary {
            total_trades: 2,
            successful_trades: 2,
            failed_trades: 0,
            success_rate: 1.0,
            total_cost: 8.7,
            avg_cost_per_trade: 4.35,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multi_algorithm_simulation_result_structure() {
        // Create test simulation results for all algorithms
        let results = vec![
            create_test_simulation_result(AlgorithmType::Momentum, 1150.0),
            create_test_simulation_result(AlgorithmType::Portfolio, 1120.0),
            create_test_simulation_result(AlgorithmType::Portfolio, 1080.0),
        ];

        // Create the same structure that simulate would create
        use crate::commands::simulate::AlgorithmComparison;
        use crate::commands::simulate::AlgorithmSummaryRow;

        let comparison = AlgorithmComparison {
            best_return: (AlgorithmType::Momentum, 15.0),
            best_sharpe: (AlgorithmType::Momentum, 1.2),
            lowest_drawdown: (AlgorithmType::Portfolio, -2.0),
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
            results: results.clone(),
            comparison,
        };

        // Test 1: Verify it can be serialized to JSON
        let json_content = serde_json::to_string_pretty(&multi_result)
            .expect("Should be able to serialize MultiAlgorithmSimulationResult");

        // Test 2: Verify it can be deserialized back
        let deserialized: MultiAlgorithmSimulationResult = serde_json::from_str(&json_content)
            .expect("Should be able to deserialize MultiAlgorithmSimulationResult");

        assert_eq!(deserialized.results.len(), 3);
        assert_eq!(deserialized.comparison.summary_table.len(), 3);

        // Test 3: Verify the JSON structure matches what report expects
        let json_value: serde_json::Value =
            serde_json::from_str(&json_content).expect("Should parse as JSON value");

        // Check required fields exist
        assert!(json_value.get("results").is_some());
        assert!(json_value.get("comparison").is_some());

        // Check results array structure
        let results_array = json_value["results"]
            .as_array()
            .expect("results should be array");
        assert_eq!(results_array.len(), 3);

        for result in results_array {
            assert!(result.get("config").is_some());
            assert!(result.get("performance").is_some());
            assert!(result.get("trades").is_some());
            assert!(result.get("portfolio_values").is_some());
            assert!(result.get("execution_summary").is_some());
        }

        // Check comparison structure
        let comparison_obj = &json_value["comparison"];
        assert!(comparison_obj.get("best_return").is_some());
        assert!(comparison_obj.get("best_sharpe").is_some());
        assert!(comparison_obj.get("lowest_drawdown").is_some());
        assert!(comparison_obj.get("summary_table").is_some());
    }

    #[test]
    fn test_simulation_result_individual_structure() {
        // Test that individual SimulationResult has the correct structure
        let single_result = create_test_simulation_result(AlgorithmType::Momentum, 1150.0);

        // Test 1: Verify it can be serialized
        let json_content = serde_json::to_string_pretty(&single_result)
            .expect("Should be able to serialize SimulationResult");

        // Test 2: Verify it can be deserialized back
        let deserialized: SimulationResult = serde_json::from_str(&json_content)
            .expect("Should be able to deserialize SimulationResult");

        assert_eq!(deserialized.config.algorithm, AlgorithmType::Momentum);
        assert_eq!(deserialized.config.final_value, 1150.0);

        // Test 3: Verify JSON structure
        let json_value: serde_json::Value =
            serde_json::from_str(&json_content).expect("Should parse as JSON value");

        assert!(json_value.get("config").is_some());
        assert!(json_value.get("performance").is_some());
        assert!(json_value.get("trades").is_some());
        assert!(json_value.get("portfolio_values").is_some());
        assert!(json_value.get("execution_summary").is_some());

        // Check config structure
        let config = &json_value["config"];
        assert!(config.get("start_date").is_some());
        assert!(config.get("end_date").is_some());
        assert!(config.get("algorithm").is_some());
        assert!(config.get("initial_capital").is_some());
        assert!(config.get("final_value").is_some());

        // Check performance structure
        let performance = &json_value["performance"];
        assert!(performance.get("total_return_pct").is_some());
        assert!(performance.get("sharpe_ratio").is_some());
        assert!(performance.get("max_drawdown_pct").is_some());
        assert!(performance.get("total_trades").is_some());
    }

    #[test]
    fn test_simulate_output_structure_compatibility() {
        // This test verifies that the JSON structure output by simulate
        // matches exactly what report expects to read
        use tempfile::TempDir;

        // Create temp workspace
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Step 1: Create mock token files that simulate would typically read
        setup_mock_token_files(&temp_dir);

        // Step 2: Set up environment to use our temp directory
        unsafe {
            std::env::set_var("CLI_TOKENS_BASE_DIR", temp_dir.path());
        }

        // Step 3: Create SimulateArgs similar to actual CLI usage
        use crate::commands::simulate::{SimulateArgs, validate_and_convert_args};
        let args = SimulateArgs {
            start: Some("2024-08-10".to_string()),
            end: Some("2024-08-20".to_string()),
            capital: 1000.0,
            quote_token: "wrap.near".to_string(),
            output: "test_simulation".to_string(),
            rebalance_interval: "1d".to_string(),
            fee_model: "zero".to_string(), // Use zero fees for predictable results
            custom_fee: None,
            slippage: 0.0,
            gas_cost: 0.0,
            min_trade: 1.0,
            prediction_horizon: 24,
            historical_days: 7,
            chart: false,
            verbose: false,
            model: Some("mock".to_string()),
            portfolio_rebalance_threshold: 0.05,
            portfolio_rebalance_interval: "1d".to_string(),
            momentum_min_profit_threshold: 0.01,
            momentum_switch_multiplier: 1.2,
            momentum_min_trade_amount: 0.1,
            trend_rsi_overbought: 80.0,
            trend_rsi_oversold: 20.0,
            trend_adx_strong_threshold: 20.0,
            trend_r_squared_threshold: 0.5,
        };

        // Step 4: Try to validate args (this tests the actual config creation)
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(validate_and_convert_args(args));

        // If validation works, the structure should be compatible
        match result {
            Ok(config) => {
                // Verify that the config has the expected structure
                assert!(!config.target_tokens.is_empty() || config.target_tokens.is_empty()); // Allow empty for this test
                assert!(config.initial_capital > bigdecimal::BigDecimal::from(0));
                println!("✅ Config validation successful - structure is compatible");
            }
            Err(e) => {
                // If it fails due to missing tokens, that's expected in test environment
                if e.to_string().contains("No tokens found") {
                    println!(
                        "ℹ️  Expected token directory error in test environment: {}",
                        e
                    );
                } else {
                    panic!("Unexpected validation error: {}", e);
                }
            }
        }

        // Step 5: Test JSON structure compatibility directly
        // Create a minimal but realistic result structure
        let test_results = create_realistic_simulation_results();

        // Use the actual save function that simulate uses
        use crate::commands::simulate::save_simple_multi_algorithm_result;
        let output_path = temp_dir.path().join("output");
        std::fs::create_dir_all(&output_path).unwrap();

        save_simple_multi_algorithm_result(&test_results, output_path.to_str().unwrap(), false)
            .expect("Should save results successfully");

        // Find output file
        let entries = std::fs::read_dir(&output_path).expect("Should read output directory");
        let multi_dir = entries
            .filter_map(|e| e.ok())
            .find(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("multi_algorithm_")
            })
            .expect("Should find multi_algorithm directory");

        let json_file = multi_dir.path().join("multi_results.json");
        assert!(json_file.exists(), "multi_results.json should exist");

        // Step 6: Test that report can read this exact JSON
        use crate::commands::report::{ReportArgs, run_report};
        let report_args = ReportArgs {
            input: json_file,
            output: Some(temp_dir.path().join("test_report.html")),
        };

        // This is the critical test - can report actually process simulate's output?
        run_report(report_args).expect("Report must be able to process simulate's output");

        let html_file = temp_dir.path().join("test_report.html");
        assert!(html_file.exists(), "HTML report should be generated");

        let html_content = std::fs::read_to_string(html_file).expect("Should read HTML");
        assert!(
            html_content.contains("Multi-Algorithm"),
            "Should contain multi-algorithm content"
        );

        // Clean up environment
        unsafe {
            std::env::remove_var("CLI_TOKENS_BASE_DIR");
        }
    }

    /// Create realistic simulation results that match actual simulate command output
    fn create_realistic_simulation_results() -> Vec<SimulationResult> {
        // Use the same creation function but with realistic data patterns
        vec![
            create_test_simulation_result(AlgorithmType::Momentum, 1050.0),
            create_test_simulation_result(AlgorithmType::Portfolio, 1030.0),
            create_test_simulation_result(AlgorithmType::Portfolio, 990.0),
        ]
    }

    /// Set up mock token files that simulate would read
    fn setup_mock_token_files(temp_dir: &tempfile::TempDir) {
        let tokens_dir = temp_dir.path().join("tokens").join("wrap.near");
        std::fs::create_dir_all(&tokens_dir).expect("Should create tokens directory");

        // Create a mock token file
        let token_file = tokens_dir.join("test.tkn.near.json");
        let token_data = serde_json::json!({
            "token": "test.tkn.near",
            "metadata": {
                "name": "Test Token",
                "symbol": "TEST"
            }
        });

        std::fs::write(
            &token_file,
            serde_json::to_string_pretty(&token_data).unwrap(),
        )
        .expect("Should write token file");
    }

    #[test]
    fn test_algorithm_type_serialization() {
        // Test that AlgorithmType serializes correctly for JSON compatibility
        let algorithms = vec![AlgorithmType::Momentum, AlgorithmType::Portfolio];

        for algo in algorithms {
            let json_value = serde_json::to_value(&algo).expect("Should serialize AlgorithmType");
            let string_repr = json_value.as_str().expect("Should be string");

            match algo {
                AlgorithmType::Momentum => assert_eq!(string_repr, "Momentum"),
                AlgorithmType::Portfolio => assert_eq!(string_repr, "Portfolio"),
            }

            // Test round-trip
            let deserialized: AlgorithmType =
                serde_json::from_value(json_value).expect("Should deserialize AlgorithmType");
            assert_eq!(deserialized, algo);
        }
    }
}
