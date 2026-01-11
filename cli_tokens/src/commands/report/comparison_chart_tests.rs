use super::*;
use crate::commands::simulate::{
    AlgorithmType, DataQualityStats, ExecutionSummary, NearValueF64, PerformanceMetrics,
    PortfolioValue, SimulationResult, SimulationSummary, TradeExecution,
};
use chrono::{DateTime, Utc};

fn create_test_simulation_result(algorithm: AlgorithmType, values: Vec<f64>) -> SimulationResult {
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

    let portfolio_values: Vec<PortfolioValue> = values
        .iter()
        .enumerate()
        .map(|(i, &value)| PortfolioValue {
            timestamp: start_date + chrono::Duration::days(i as i64),
            total_value: NearValueF64::from_near(value),
            cash_balance: NearValueF64::zero(),
            holdings: std::collections::HashMap::new(),
            unrealized_pnl: NearValueF64::from_near(value - 1000.0),
        })
        .collect();

    SimulationResult {
        config: SimulationSummary {
            start_date,
            end_date,
            algorithm,
            initial_capital: 1000.0,
            final_value: values.last().copied().unwrap_or(1000.0),
            total_return: (values.last().copied().unwrap_or(1000.0) - 1000.0) / 1000.0 * 100.0,
            duration_days: 10,
        },
        performance: PerformanceMetrics {
            total_return: (values.last().copied().unwrap_or(1000.0) - 1000.0) / 1000.0 * 100.0,
            annualized_return: 0.1,
            total_return_pct: (values.last().copied().unwrap_or(1000.0) - 1000.0) / 1000.0 * 100.0,
            volatility: 0.2,
            max_drawdown: -50.0,
            max_drawdown_pct: -5.0,
            sharpe_ratio: 1.5,
            sortino_ratio: 2.0,
            total_trades: 5,
            winning_trades: 3,
            losing_trades: 2,
            win_rate: 0.6,
            profit_factor: 1.2,
            total_costs: 10.0,
            cost_ratio: 0.01,
            simulation_days: 10,
            active_trading_days: 8,
        },
        trades: Vec::<TradeExecution>::new(),
        portfolio_values,
        execution_summary: ExecutionSummary {
            total_trades: 5,
            successful_trades: 5,
            failed_trades: 0,
            success_rate: 1.0,
            total_cost: 10.0,
            avg_cost_per_trade: 2.0,
        },
        data_quality: DataQualityStats {
            total_timesteps: 100,
            skipped_timesteps: 0,
            data_coverage_percentage: 100.0,
            longest_gap_hours: 0,
            gap_events: Vec::new(),
        },
    }
}

#[test]
fn test_get_algorithm_colors() {
    let colors = get_algorithm_colors();
    assert_eq!(colors.len(), 3);
    assert_eq!(colors[0].0, "Momentum");
    assert_eq!(colors[1].0, "Portfolio");
    assert_eq!(colors[2].0, "TrendFollowing");
}

#[test]
fn test_extract_common_labels() {
    let results = vec![
        create_test_simulation_result(AlgorithmType::Momentum, vec![1000.0, 1100.0, 1200.0]),
        create_test_simulation_result(AlgorithmType::Portfolio, vec![1000.0, 1150.0, 1250.0]),
    ];

    let labels = extract_common_labels(&results);
    assert!(!labels.is_empty());
    // Labels should be date formatted strings
    assert!(labels[0].contains("08/10"));
}

#[test]
fn test_create_chart_dataset() {
    let colors = get_algorithm_colors();
    let result =
        create_test_simulation_result(AlgorithmType::Momentum, vec![1000.0, 1100.0, 1200.0]);

    let dataset = create_chart_dataset(&result, 0, &colors).unwrap();
    assert!(dataset.contains("Momentum"));
    assert!(dataset.contains("rgb(255, 99, 132)")); // Momentum color
    assert!(dataset.contains("1000"));
    assert!(dataset.contains("1100"));
    assert!(dataset.contains("1200"));
}
