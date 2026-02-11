use super::*;
use crate::cli::Cli;
use crate::portfolio_state::{PortfolioSnapshot, PortfolioState, TradeRecord};
use chrono::{TimeZone, Utc};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn make_snapshot(total_value_near: f64) -> PortfolioSnapshot {
    PortfolioSnapshot {
        timestamp: Utc::now(),
        total_value_near,
        holdings: BTreeMap::new(),
        cash_balance: 0,
        realized_pnl_near: 0.0,
    }
}

// --- calculate_sharpe_ratio ---

#[test]
fn sharpe_ratio_empty_returns() {
    assert_eq!(calculate_sharpe_ratio(&[]), 0.0);
}

#[test]
fn sharpe_ratio_single_return() {
    assert_eq!(calculate_sharpe_ratio(&[0.01]), 0.0);
}

#[test]
fn sharpe_ratio_zero_returns() {
    // All zero returns → std_dev = 0 → sharpe = 0
    let returns = vec![0.0; 10];
    assert_eq!(calculate_sharpe_ratio(&returns), 0.0);
}

#[test]
fn sharpe_ratio_positive_returns() {
    let returns = vec![0.01, 0.02, 0.015, 0.005, 0.03];
    let result = calculate_sharpe_ratio(&returns);
    assert!(
        result > 0.0,
        "positive mean returns should yield positive sharpe"
    );
}

#[test]
fn sharpe_ratio_negative_returns() {
    let returns = vec![-0.01, -0.02, -0.015, -0.005, -0.03];
    let result = calculate_sharpe_ratio(&returns);
    assert!(
        result < 0.0,
        "negative mean returns should yield negative sharpe"
    );
}

// --- calculate_sortino_ratio ---

#[test]
fn sortino_ratio_empty_returns() {
    assert_eq!(calculate_sortino_ratio(&[]), 0.0);
}

#[test]
fn sortino_ratio_single_return() {
    assert_eq!(calculate_sortino_ratio(&[0.01]), 0.0);
}

#[test]
fn sortino_ratio_all_positive() {
    // No downside deviation → sortino = 0
    let returns = vec![0.01, 0.02, 0.03];
    assert_eq!(calculate_sortino_ratio(&returns), 0.0);
}

#[test]
fn sortino_ratio_mixed_returns() {
    let returns = vec![0.01, -0.02, 0.03, -0.01, 0.02];
    let result = calculate_sortino_ratio(&returns);
    // Mean is positive, some downside → finite positive sortino
    assert!(result > 0.0);
}

#[test]
fn sortino_ratio_all_negative() {
    let returns = vec![-0.01, -0.02, -0.03];
    let result = calculate_sortino_ratio(&returns);
    assert!(
        result < 0.0,
        "all-negative returns should yield negative sortino"
    );
}

// --- calculate_max_drawdown ---

#[test]
fn max_drawdown_empty() {
    assert_eq!(calculate_max_drawdown(&[]), 0.0);
}

#[test]
fn max_drawdown_monotonic_increase() {
    let snapshots = vec![
        make_snapshot(100.0),
        make_snapshot(110.0),
        make_snapshot(120.0),
    ];
    assert_eq!(calculate_max_drawdown(&snapshots), 0.0);
}

#[test]
fn max_drawdown_monotonic_decrease() {
    let snapshots = vec![
        make_snapshot(100.0),
        make_snapshot(80.0),
        make_snapshot(60.0),
    ];
    let dd = calculate_max_drawdown(&snapshots);
    assert!((dd - 0.4).abs() < 1e-10, "expected 40% drawdown, got {dd}");
}

#[test]
fn max_drawdown_peak_then_recovery() {
    let snapshots = vec![
        make_snapshot(100.0),
        make_snapshot(120.0),
        make_snapshot(90.0), // 25% drawdown from 120
        make_snapshot(130.0),
    ];
    let dd = calculate_max_drawdown(&snapshots);
    assert!((dd - 0.25).abs() < 1e-10, "expected 25% drawdown, got {dd}");
}

#[test]
fn max_drawdown_multiple_drawdowns() {
    let snapshots = vec![
        make_snapshot(100.0),
        make_snapshot(90.0), // 10% dd from 100
        make_snapshot(110.0),
        make_snapshot(77.0), // 30% dd from 110
        make_snapshot(120.0),
    ];
    let dd = calculate_max_drawdown(&snapshots);
    assert!((dd - 0.3).abs() < 1e-10, "expected 30% drawdown, got {dd}");
}

// --- calculate_performance ---

#[test]
fn performance_empty_snapshots() {
    let perf = calculate_performance(100.0, &[], 0, 0, 0);
    assert_eq!(perf.total_return, 0.0);
    assert_eq!(perf.annualized_return, 0.0);
    assert_eq!(perf.sharpe_ratio, 0.0);
    assert_eq!(perf.sortino_ratio, 0.0);
    assert_eq!(perf.max_drawdown, 0.0);
    assert_eq!(perf.win_rate, 0.0);
}

#[test]
fn performance_single_snapshot() {
    let snapshots = vec![make_snapshot(110.0)];
    let perf = calculate_performance(100.0, &snapshots, 0, 0, 0);
    assert!(
        (perf.total_return - 0.1).abs() < 1e-10,
        "expected 10% return"
    );
}

#[test]
fn performance_zero_initial_capital() {
    let snapshots = vec![make_snapshot(100.0)];
    let perf = calculate_performance(0.0, &snapshots, 0, 0, 0);
    assert_eq!(perf.total_return, 0.0);
    assert_eq!(perf.annualized_return, 0.0);
}

#[test]
fn performance_win_rate() {
    // 3 up days, 2 down days → win_rate = 3/5 = 0.6
    let snapshots = vec![
        make_snapshot(110.0), // up from 100
        make_snapshot(105.0), // down
        make_snapshot(115.0), // up
        make_snapshot(110.0), // down
        make_snapshot(120.0), // up
    ];
    let perf = calculate_performance(100.0, &snapshots, 0, 0, 0);
    assert!(
        (perf.win_rate - 0.6).abs() < 1e-10,
        "expected 60% win rate, got {}",
        perf.win_rate
    );
}

#[test]
fn performance_total_return_loss() {
    let snapshots = vec![make_snapshot(80.0)];
    let perf = calculate_performance(100.0, &snapshots, 0, 0, 0);
    assert!(
        (perf.total_return - (-0.2)).abs() < 1e-10,
        "expected -20% return"
    );
}

// --- SimulationResult::from_state ---

fn make_cli(start: &str, end: &str) -> Cli {
    Cli {
        start_date: start.to_string(),
        end_date: end.to_string(),
        initial_capital: 100.0,
        top_tokens: 10,
        volatility_days: 7,
        price_history_days: 30,
        rebalance_threshold: 0.1,
        rebalance_interval_days: 1,
        output: PathBuf::from("test.json"),
        sweep: None,
    }
}

#[test]
fn from_state_maps_trades_correctly() {
    let cli = make_cli("2025-01-01", "2025-01-31");
    let ts = Utc.with_ymd_and_hms(2025, 1, 5, 0, 0, 0).unwrap();

    let mut state = PortfolioState::new(100_000_000_000_000_000_000_000_000);
    state.trades.push(TradeRecord {
        timestamp: ts,
        action: "buy".to_string(),
        token: "usdt.tether-token.near".to_string(),
        amount: 1_000_000,
        price_near: 0.5,
        realized_pnl_near: None,
    });
    state.trades.push(TradeRecord {
        timestamp: ts,
        action: "sell".to_string(),
        token: "usdt.tether-token.near".to_string(),
        amount: 500_000,
        price_near: 0.25,
        realized_pnl_near: Some(0.1),
    });

    let result = SimulationResult::from_state(&cli, &state).unwrap();
    assert_eq!(result.trades.len(), 2);
    assert_eq!(result.trades[0].action, "buy");
    assert_eq!(result.trades[0].token, "usdt.tether-token.near");
    assert_eq!(result.trades[0].amount, 1_000_000);
    assert!((result.trades[0].price - 0.5).abs() < 1e-10);
    assert_eq!(result.trades[1].action, "sell");
    assert_eq!(result.trades[1].amount, 500_000);
}

#[test]
fn from_state_maps_snapshots_to_portfolio_values() {
    let cli = make_cli("2025-01-01", "2025-01-31");
    let ts = Utc.with_ymd_and_hms(2025, 1, 10, 0, 0, 0).unwrap();

    let cash_yocto = 50_000_000_000_000_000_000_000_000u128; // 50 NEAR
    let mut holdings = BTreeMap::new();
    holdings.insert("token.near".to_string(), 999u128);

    let mut state = PortfolioState::new(100_000_000_000_000_000_000_000_000);
    state.snapshots.push(PortfolioSnapshot {
        timestamp: ts,
        total_value_near: 105.0,
        holdings: holdings.clone(),
        cash_balance: cash_yocto,
        realized_pnl_near: 0.0,
    });

    let result = SimulationResult::from_state(&cli, &state).unwrap();
    assert_eq!(result.portfolio_values.len(), 1);
    assert!((result.portfolio_values[0].total_value - 105.0).abs() < 1e-10);
    assert_eq!(result.portfolio_values[0].holdings["token.near"], 999);
    assert!((result.portfolio_values[0].cash_balance - 50.0).abs() < 1e-10);
}

#[test]
fn from_state_config_reflects_cli_params() {
    let cli = Cli {
        start_date: "2025-03-01".to_string(),
        end_date: "2025-03-31".to_string(),
        initial_capital: 200.0,
        top_tokens: 5,
        volatility_days: 14,
        price_history_days: 60,
        rebalance_threshold: 0.2,
        rebalance_interval_days: 3,
        output: PathBuf::from("out.json"),
        sweep: None,
    };
    let state = PortfolioState::new(200_000_000_000_000_000_000_000_000);

    let result = SimulationResult::from_state(&cli, &state).unwrap();
    assert_eq!(result.config.start_date, "2025-03-01");
    assert_eq!(result.config.end_date, "2025-03-31");
    assert!((result.config.initial_capital - 200.0).abs() < 1e-10);
    assert_eq!(result.config.parameters.top_tokens, 5);
    assert_eq!(result.config.parameters.volatility_days, 14);
    assert_eq!(result.config.parameters.price_history_days, 60);
    assert!((result.config.parameters.rebalance_threshold - 0.2).abs() < 1e-10);
    assert_eq!(result.config.parameters.rebalance_interval_days, 3);
}

#[test]
fn from_state_empty_state() {
    let cli = make_cli("2025-01-01", "2025-01-31");
    let state = PortfolioState::new(100_000_000_000_000_000_000_000_000);

    let result = SimulationResult::from_state(&cli, &state).unwrap();
    assert!(result.trades.is_empty());
    assert!(result.portfolio_values.is_empty());
    assert_eq!(result.performance.total_return, 0.0);
    assert_eq!(result.performance.sharpe_ratio, 0.0);
}

// --- daily P&L ---

#[test]
fn portfolio_value_entry_daily_pnl() {
    let cli = make_cli("2025-01-01", "2025-01-31");
    let ts1 = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    let ts2 = Utc.with_ymd_and_hms(2025, 1, 2, 0, 0, 0).unwrap();

    let mut state = PortfolioState::new(100_000_000_000_000_000_000_000_000);
    state.snapshots.push(PortfolioSnapshot {
        timestamp: ts1,
        total_value_near: 105.0,
        holdings: BTreeMap::new(),
        cash_balance: 0,
        realized_pnl_near: 0.0,
    });
    state.snapshots.push(PortfolioSnapshot {
        timestamp: ts2,
        total_value_near: 110.0,
        holdings: BTreeMap::new(),
        cash_balance: 0,
        realized_pnl_near: 2.5,
    });

    let result = SimulationResult::from_state(&cli, &state).unwrap();
    // Day 1: 105 - 100 (initial) = +5
    assert!(
        (result.portfolio_values[0].daily_pnl_near - 5.0).abs() < 1e-10,
        "day1 pnl: {}",
        result.portfolio_values[0].daily_pnl_near
    );
    assert!(
        (result.portfolio_values[0].daily_pnl_pct - 0.05).abs() < 1e-10,
        "day1 pct: {}",
        result.portfolio_values[0].daily_pnl_pct
    );
    // Day 2: 110 - 105 = +5
    assert!(
        (result.portfolio_values[1].daily_pnl_near - 5.0).abs() < 1e-10,
        "day2 pnl: {}",
        result.portfolio_values[1].daily_pnl_near
    );
    // cumulative realized pnl
    assert!(
        (result.portfolio_values[1].cumulative_realized_pnl_near - 2.5).abs() < 1e-10,
        "cum pnl: {}",
        result.portfolio_values[1].cumulative_realized_pnl_near
    );
}

// --- performance new fields ---

#[test]
fn performance_includes_new_fields() {
    let snapshots = vec![make_snapshot(110.0)];
    let realized_pnl: i128 = 5_000_000_000_000_000_000_000_000; // 5 NEAR
    let perf = calculate_performance(100.0, &snapshots, realized_pnl, 10, 3);
    assert!(
        (perf.final_balance_near - 110.0).abs() < 1e-10,
        "final balance: {}",
        perf.final_balance_near
    );
    assert!(
        (perf.total_realized_pnl_near - 5.0).abs() < 1e-10,
        "realized pnl: {}",
        perf.total_realized_pnl_near
    );
    assert_eq!(perf.trade_count, 10);
    assert_eq!(perf.liquidation_count, 3);
}

#[test]
fn from_state_maps_realized_pnl_on_trade() {
    let cli = make_cli("2025-01-01", "2025-01-31");
    let ts = Utc.with_ymd_and_hms(2025, 1, 5, 0, 0, 0).unwrap();

    let mut state = PortfolioState::new(100_000_000_000_000_000_000_000_000);
    state.trades.push(TradeRecord {
        timestamp: ts,
        action: "sell".to_string(),
        token: "token.near".to_string(),
        amount: 1_000_000,
        price_near: 1.0,
        realized_pnl_near: Some(0.5),
    });

    let result = SimulationResult::from_state(&cli, &state).unwrap();
    assert_eq!(result.trades[0].realized_pnl, Some(0.5));
}
