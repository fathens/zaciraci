use super::*;
use crate::portfolio_state::PortfolioSnapshot;
use chrono::Utc;
use std::collections::BTreeMap;

fn make_snapshot(total_value_near: f64) -> PortfolioSnapshot {
    PortfolioSnapshot {
        timestamp: Utc::now(),
        total_value_near,
        holdings: BTreeMap::new(),
        cash_balance: 0,
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
    let perf = calculate_performance(100.0, &[]);
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
    let perf = calculate_performance(100.0, &snapshots);
    assert!(
        (perf.total_return - 0.1).abs() < 1e-10,
        "expected 10% return"
    );
}

#[test]
fn performance_zero_initial_capital() {
    let snapshots = vec![make_snapshot(100.0)];
    let perf = calculate_performance(0.0, &snapshots);
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
    let perf = calculate_performance(100.0, &snapshots);
    assert!(
        (perf.win_rate - 0.6).abs() < 1e-10,
        "expected 60% win rate, got {}",
        perf.win_rate
    );
}

#[test]
fn performance_total_return_loss() {
    let snapshots = vec![make_snapshot(80.0)];
    let perf = calculate_performance(100.0, &snapshots);
    assert!(
        (perf.total_return - (-0.2)).abs() < 1e-10,
        "expected -20% return"
    );
}
