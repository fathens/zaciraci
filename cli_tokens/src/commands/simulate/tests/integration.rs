//! Simulateコマンドの統合テスト
//! - パフォーマンス指標の計算テスト
//! - トレード実行の統合テスト
//! - ポートフォリオ価値の計算テスト
//! - 利益率・損失率の統合計算

use chrono::{DateTime, Utc};
use std::collections::HashMap;

use super::super::metrics::calculate_performance_metrics;
use super::super::*;

// Import newtype wrappers
use super::super::{NearValueF64, TokenAmountF64, TokenPriceF64, YoctoValueF64};

// Note: generate_mock_price_data function is not available in simulate module
// This test has been commented out as it depends on non-existent functionality
// #[tokio::test]
// async fn test_generate_mock_price_data() -> Result<()> { ... }

// Helper function to create a test trade execution
fn create_test_trade(
    portfolio_value_before: f64,
    portfolio_value_after: f64,
    cost: f64,
) -> TradeExecution {
    TradeExecution {
        timestamp: Utc::now(),
        from_token: "token_a".to_string(),
        to_token: "token_b".to_string(),
        amount: TokenAmountF64::from_smallest_units(100.0, 24),
        executed_price: TokenPriceF64::from_near_per_token(1.0),
        cost: TradingCost {
            protocol_fee: YoctoValueF64::zero(),
            slippage: YoctoValueF64::zero(),
            gas_fee: YoctoValueF64::zero(),
            total: YoctoValueF64::from_yocto(cost * 1e24), // コストを yoctoNEAR に変換
        },
        portfolio_value_before: NearValueF64::from_near(portfolio_value_before),
        portfolio_value_after: NearValueF64::from_near(portfolio_value_after),
        success: true,
        reason: "Test trade".to_string(),
    }
}

// Helper function to create a test portfolio value
fn create_portfolio_value(timestamp: DateTime<Utc>, total_value: f64) -> PortfolioValue {
    PortfolioValue {
        timestamp,
        holdings: HashMap::new(),
        total_value: NearValueF64::from_near(total_value),
        cash_balance: NearValueF64::zero(),
        unrealized_pnl: NearValueF64::zero(),
    }
}

/// Helper function to create a NearValueF64 for testing
fn nv(value: f64) -> NearValueF64 {
    NearValueF64::from_near(value)
}

#[test]
fn test_profit_factor_calculation_only_profits() {
    let trades = vec![
        create_test_trade(1000.0, 1100.0, 5.0), // +100 profit
        create_test_trade(1100.0, 1200.0, 5.0), // +100 profit
        create_test_trade(1200.0, 1350.0, 5.0), // +150 profit
    ];

    let portfolio_values = vec![
        create_portfolio_value(Utc::now(), 1000.0),
        create_portfolio_value(Utc::now(), 1350.0),
    ];

    let start_date = Utc::now() - chrono::Duration::days(30);
    let end_date = Utc::now();
    let metrics = calculate_performance_metrics(
        nv(1000.0),
        nv(1350.0),
        &portfolio_values,
        &trades,
        nv(50.0),
        start_date,
        end_date,
    )
    .unwrap();

    // Total profit = 350, no losses, so profit factor should be very high (f64::MAX)
    assert_eq!(metrics.profit_factor, f64::INFINITY);
    assert_eq!(metrics.winning_trades, 3);
    assert_eq!(metrics.losing_trades, 0);
    assert_eq!(metrics.win_rate, 100.0);
}

#[test]
fn test_profit_factor_calculation_only_losses() {
    let trades = vec![
        create_test_trade(1000.0, 950.0, 5.0), // -50 loss
        create_test_trade(950.0, 900.0, 5.0),  // -50 loss
        create_test_trade(900.0, 800.0, 5.0),  // -100 loss
    ];

    let portfolio_values = vec![
        create_portfolio_value(Utc::now(), 1000.0),
        create_portfolio_value(Utc::now(), 800.0),
    ];

    let start_date = Utc::now() - chrono::Duration::days(30);
    let end_date = Utc::now();
    let metrics = calculate_performance_metrics(
        nv(1000.0),
        nv(800.0),
        &portfolio_values,
        &trades,
        nv(30.0),
        start_date,
        end_date,
    )
    .unwrap();

    // Total loss = 200, no profits, so profit factor should be 0
    assert_eq!(metrics.profit_factor, 0.0);
    assert_eq!(metrics.winning_trades, 0);
    assert_eq!(metrics.losing_trades, 3);
    assert_eq!(metrics.win_rate, 0.0);
}

#[test]
fn test_profit_factor_calculation_mixed_trades() {
    let trades = vec![
        create_test_trade(1000.0, 1200.0, 5.0), // +200 profit
        create_test_trade(1200.0, 1000.0, 5.0), // -200 loss
        create_test_trade(1000.0, 1150.0, 5.0), // +150 profit
        create_test_trade(1150.0, 1100.0, 5.0), // -50 loss
    ];

    let portfolio_values = vec![
        create_portfolio_value(Utc::now(), 1000.0),
        create_portfolio_value(Utc::now(), 1100.0),
    ];

    let start_date = Utc::now() - chrono::Duration::days(30);
    let end_date = Utc::now();
    let metrics = calculate_performance_metrics(
        nv(1000.0),
        nv(1100.0),
        &portfolio_values,
        &trades,
        nv(30.0),
        start_date,
        end_date,
    )
    .unwrap();

    // Total profit = 350, Total loss = 250, Profit factor = 350/250 = 1.4
    assert_eq!(metrics.profit_factor, 1.4);
    assert_eq!(metrics.winning_trades, 2);
    assert_eq!(metrics.losing_trades, 2);
    assert_eq!(metrics.win_rate, 50.0);
}

#[test]
fn test_profit_factor_calculation_no_trades() {
    let trades = vec![];
    let portfolio_values = vec![
        create_portfolio_value(Utc::now(), 1000.0),
        create_portfolio_value(Utc::now(), 1000.0),
    ];

    let start_date = Utc::now() - chrono::Duration::days(30);
    let end_date = Utc::now();
    let metrics = calculate_performance_metrics(
        nv(1000.0),
        nv(1000.0),
        &portfolio_values,
        &trades,
        nv(0.0),
        start_date,
        end_date,
    )
    .unwrap();

    // No trades, so profit factor should be 0
    assert_eq!(metrics.profit_factor, 0.0);
    assert_eq!(metrics.winning_trades, 0);
    assert_eq!(metrics.losing_trades, 0);
    assert_eq!(metrics.win_rate, 0.0);
    assert_eq!(metrics.total_trades, 0);
}

#[test]
fn test_performance_metrics_calculation() {
    let initial_value = 1000.0;
    let final_value = 1100.0;

    let portfolio_values = vec![
        PortfolioValue {
            timestamp: Utc::now(),
            total_value: NearValueF64::from_near(initial_value),
            cash_balance: NearValueF64::from_near(initial_value),
            holdings: HashMap::new(),
            unrealized_pnl: NearValueF64::zero(),
        },
        PortfolioValue {
            timestamp: Utc::now(),
            total_value: NearValueF64::from_near(final_value),
            cash_balance: NearValueF64::zero(),
            holdings: HashMap::new(),
            unrealized_pnl: NearValueF64::from_near(100.0),
        },
    ];

    let trades = vec![];
    let simulation_days = 30;

    let start_date = Utc::now() - chrono::Duration::days(simulation_days);
    let end_date = Utc::now();
    let performance = calculate_performance_metrics(
        nv(initial_value),
        nv(final_value),
        &portfolio_values,
        &trades,
        nv(0.0),
        start_date,
        end_date,
    )
    .unwrap();

    assert_eq!(performance.total_return, nv(100.0)); // 100 profit amount
    assert_eq!(performance.total_trades, 0);
    assert_eq!(performance.simulation_days, 30);
}

#[test]
fn test_config_creation() {
    let args = SimulateArgs {
        start: Some("2024-01-01".to_string()),
        end: Some("2024-01-31".to_string()),
        capital: 1000.0,
        quote_token: "wrap.near".to_string(),
        output: "test_output".to_string(),
        rebalance_interval: "1d".to_string(),
        fee_model: "zero".to_string(),
        custom_fee: None,
        slippage: 0.01,
        gas_cost: 0.01,
        min_trade: 1.0,
        prediction_horizon: 24,
        historical_days: 30,
        chart: false,
        verbose: false,
        model: None,
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

    // Test that the args contain expected values
    assert_eq!(args.capital, 1000.0);
    assert_eq!(args.quote_token, "wrap.near");
    assert_eq!(args.historical_days, 30);
}
