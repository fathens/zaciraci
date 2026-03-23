use super::*;
use crate::cli::Cli;
use crate::portfolio_state::{
    PortfolioSnapshot, PortfolioState, SwapEvent, SwapMethod, TradeAction, TradeRecord,
};
use bigdecimal::BigDecimal;
use chrono::{TimeZone, Utc};
use common::types::{TokenAccount, TokenAmount, YoctoValue};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn make_snapshot(total_value_near: f64) -> PortfolioSnapshot {
    PortfolioSnapshot {
        timestamp: Utc::now(),
        total_value_near,
        holdings: BTreeMap::new(),
        cash_balance: YoctoValue::zero(),
        realized_pnl_near: 0.0,
    }
}

fn yocto(v: u128) -> YoctoValue {
    YoctoValue::from_yocto(BigDecimal::from(v))
}

// --- calculate_sharpe_ratio ---

#[test]
fn sharpe_ratio_empty_returns() {
    assert_eq!(calculate_sharpe_ratio(&[], 1), 0.0);
}

#[test]
fn sharpe_ratio_single_return() {
    assert_eq!(calculate_sharpe_ratio(&[0.01], 1), 0.0);
}

#[test]
fn sharpe_ratio_zero_returns() {
    // All zero returns → std_dev = 0 → sharpe = 0
    let returns = vec![0.0; 10];
    assert_eq!(calculate_sharpe_ratio(&returns, 1), 0.0);
}

#[test]
fn sharpe_ratio_positive_returns() {
    let returns = vec![0.01, 0.02, 0.015, 0.005, 0.03];
    let result = calculate_sharpe_ratio(&returns, 1);
    assert!(
        result > 0.0,
        "positive mean returns should yield positive sharpe"
    );
}

#[test]
fn sharpe_ratio_negative_returns() {
    let returns = vec![-0.01, -0.02, -0.015, -0.005, -0.03];
    let result = calculate_sharpe_ratio(&returns, 1);
    assert!(
        result < 0.0,
        "negative mean returns should yield negative sharpe"
    );
}

#[test]
fn sharpe_ratio_interval_scaling() {
    let returns = vec![0.01, 0.02, 0.015, 0.005, 0.03];
    let daily = calculate_sharpe_ratio(&returns, 1);
    let weekly = calculate_sharpe_ratio(&returns, 7);
    // With larger interval, fewer periods per year → smaller annualization factor
    assert!(
        daily > weekly,
        "daily interval should produce higher sharpe than weekly: {daily} vs {weekly}"
    );
}

// --- calculate_sortino_ratio ---

#[test]
fn sortino_ratio_empty_returns() {
    assert_eq!(calculate_sortino_ratio(&[], 1), 0.0);
}

#[test]
fn sortino_ratio_single_return() {
    assert_eq!(calculate_sortino_ratio(&[0.01], 1), 0.0);
}

#[test]
fn sortino_ratio_all_positive() {
    // No downside deviation → sortino = 0
    let returns = vec![0.01, 0.02, 0.03];
    assert_eq!(calculate_sortino_ratio(&returns, 1), 0.0);
}

#[test]
fn sortino_ratio_mixed_returns() {
    let returns = vec![0.01, -0.02, 0.03, -0.01, 0.02];
    let result = calculate_sortino_ratio(&returns, 1);
    // Mean is positive, some downside → finite positive sortino
    assert!(result > 0.0);
}

#[test]
fn sortino_ratio_all_negative() {
    let returns = vec![-0.01, -0.02, -0.03];
    let result = calculate_sortino_ratio(&returns, 1);
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
    let perf = calculate_performance(PerformanceInput {
        initial_capital: 100.0,
        snapshots: &[],
        realized_pnl: 0,
        trade_count: 0,
        liquidation_count: 0,
        rebalance_interval_days: 1,
        swap_stats: SwapStats::default(),
    });
    assert_eq!(perf.total_return, 0.0);
    assert_eq!(perf.sharpe_ratio, 0.0);
    assert_eq!(perf.sortino_ratio, 0.0);
    assert_eq!(perf.max_drawdown, 0.0);
    assert_eq!(perf.win_rate, 0.0);
}

#[test]
fn performance_single_snapshot() {
    let snapshots = vec![make_snapshot(110.0)];
    let perf = calculate_performance(PerformanceInput {
        initial_capital: 100.0,
        snapshots: &snapshots,
        realized_pnl: 0,
        trade_count: 0,
        liquidation_count: 0,
        rebalance_interval_days: 1,
        swap_stats: SwapStats::default(),
    });
    assert!(
        (perf.total_return - 0.1).abs() < 1e-10,
        "expected 10% return"
    );
}

#[test]
fn performance_zero_initial_capital() {
    let snapshots = vec![make_snapshot(100.0)];
    let perf = calculate_performance(PerformanceInput {
        initial_capital: 0.0,
        snapshots: &snapshots,
        realized_pnl: 0,
        trade_count: 0,
        liquidation_count: 0,
        rebalance_interval_days: 1,
        swap_stats: SwapStats::default(),
    });
    assert_eq!(perf.total_return, 0.0);
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
    let perf = calculate_performance(PerformanceInput {
        initial_capital: 100.0,
        snapshots: &snapshots,
        realized_pnl: 0,
        trade_count: 0,
        liquidation_count: 0,
        rebalance_interval_days: 1,
        swap_stats: SwapStats::default(),
    });
    assert!(
        (perf.win_rate - 0.6).abs() < 1e-10,
        "expected 60% win rate, got {}",
        perf.win_rate
    );
}

#[test]
fn performance_total_return_loss() {
    let snapshots = vec![make_snapshot(80.0)];
    let perf = calculate_performance(PerformanceInput {
        initial_capital: 100.0,
        snapshots: &snapshots,
        realized_pnl: 0,
        trade_count: 0,
        liquidation_count: 0,
        rebalance_interval_days: 1,
        swap_stats: SwapStats::default(),
    });
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
        price_history_days: 30,
        rebalance_threshold: 0.1,
        rebalance_interval_days: 1,
        output: PathBuf::from("test.json"),
        sweep: None,
    }
}

const NEAR_100_YOCTO: u128 = 100_000_000_000_000_000_000_000_000;

#[test]
fn from_state_maps_trades_correctly() {
    let cli = make_cli("2025-01-01", "2025-01-31");
    let ts = Utc.with_ymd_and_hms(2025, 1, 5, 0, 0, 0).unwrap();

    let mut state = PortfolioState::new(yocto(NEAR_100_YOCTO));
    let token: TokenAccount = "usdt.tether-token.near".parse().unwrap();
    state.trades.push(TradeRecord {
        timestamp: ts,
        action: TradeAction::Buy,
        token: token.clone(),
        amount: TokenAmount::from_smallest_units(BigDecimal::from(1_000_000), 6),
        price_near: 0.5,
        realized_pnl_near: None,
    });
    state.trades.push(TradeRecord {
        timestamp: ts,
        action: TradeAction::Sell,
        token: token.clone(),
        amount: TokenAmount::from_smallest_units(BigDecimal::from(500_000), 6),
        price_near: 0.25,
        realized_pnl_near: Some(0.1),
    });

    let result = SimulationResult::from_state(&cli, &state).unwrap();
    assert_eq!(result.trades.len(), 2);
    assert_eq!(result.trades[0].action, TradeAction::Buy);
    assert_eq!(result.trades[0].token, "usdt.tether-token.near");
    assert_eq!(result.trades[0].amount, 1_000_000);
    assert!((result.trades[0].price - 0.5).abs() < 1e-10);
    assert_eq!(result.trades[1].action, TradeAction::Sell);
    assert_eq!(result.trades[1].amount, 500_000);
}

#[test]
fn from_state_maps_snapshots_to_portfolio_values() {
    let cli = make_cli("2025-01-01", "2025-01-31");
    let ts = Utc.with_ymd_and_hms(2025, 1, 10, 0, 0, 0).unwrap();

    let cash_yocto = 50_000_000_000_000_000_000_000_000u128; // 50 NEAR
    let token: TokenAccount = "token.near".parse().unwrap();
    let mut holdings = BTreeMap::new();
    holdings.insert(
        token.clone(),
        TokenAmount::from_smallest_units(BigDecimal::from(999u64), 24),
    );

    let mut state = PortfolioState::new(yocto(NEAR_100_YOCTO));
    state.snapshots.push(PortfolioSnapshot {
        timestamp: ts,
        total_value_near: 105.0,
        holdings: holdings.clone(),
        cash_balance: yocto(cash_yocto),
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
        price_history_days: 60,
        rebalance_threshold: 0.2,
        rebalance_interval_days: 3,
        output: PathBuf::from("out.json"),
        sweep: None,
    };
    let state = PortfolioState::new(yocto(200_000_000_000_000_000_000_000_000));

    let result = SimulationResult::from_state(&cli, &state).unwrap();
    assert_eq!(result.config.start_date, "2025-03-01");
    assert_eq!(result.config.end_date, "2025-03-31");
    assert!((result.config.initial_capital - 200.0).abs() < 1e-10);
    assert_eq!(result.config.parameters.top_tokens, 5);
    assert_eq!(result.config.parameters.price_history_days, 60);
    assert!((result.config.parameters.rebalance_threshold - 0.2).abs() < 1e-10);
    assert_eq!(result.config.parameters.rebalance_interval_days, 3);
}

#[test]
fn from_state_empty_state() {
    let cli = make_cli("2025-01-01", "2025-01-31");
    let state = PortfolioState::new(yocto(NEAR_100_YOCTO));

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

    let mut state = PortfolioState::new(yocto(NEAR_100_YOCTO));
    state.snapshots.push(PortfolioSnapshot {
        timestamp: ts1,
        total_value_near: 105.0,
        holdings: BTreeMap::new(),
        cash_balance: YoctoValue::zero(),
        realized_pnl_near: 0.0,
    });
    state.snapshots.push(PortfolioSnapshot {
        timestamp: ts2,
        total_value_near: 110.0,
        holdings: BTreeMap::new(),
        cash_balance: YoctoValue::zero(),
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
    let perf = calculate_performance(PerformanceInput {
        initial_capital: 100.0,
        snapshots: &snapshots,
        realized_pnl,
        trade_count: 10,
        liquidation_count: 3,
        rebalance_interval_days: 1,
        swap_stats: SwapStats::default(),
    });
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

    let mut state = PortfolioState::new(yocto(NEAR_100_YOCTO));
    let token: TokenAccount = "token.near".parse().unwrap();
    state.trades.push(TradeRecord {
        timestamp: ts,
        action: TradeAction::Sell,
        token,
        amount: TokenAmount::from_smallest_units(BigDecimal::from(1_000_000), 24),
        price_near: 1.0,
        realized_pnl_near: Some(0.5),
    });

    let result = SimulationResult::from_state(&cli, &state).unwrap();
    assert_eq!(result.trades[0].realized_pnl, Some(0.5));
}

// --- swap event and fallback stats ---

fn make_swap_event(method: SwapMethod, pool_ids: Vec<u32>) -> SwapEvent {
    let token_a: TokenAccount = "token_a.near".parse().unwrap();
    let token_b: TokenAccount = "token_b.near".parse().unwrap();
    SwapEvent {
        timestamp: Utc::now(),
        token_in: token_a,
        amount_in: TokenAmount::from_smallest_units(BigDecimal::from(1_000_000u64), 24),
        token_out: token_b,
        amount_out: TokenAmount::from_smallest_units(BigDecimal::from(500_000u64), 24),
        swap_method: method,
        pool_ids,
    }
}

#[test]
fn from_state_no_swap_events() {
    let cli = make_cli("2025-01-01", "2025-01-31");
    let state = PortfolioState::new(yocto(NEAR_100_YOCTO));

    let result = SimulationResult::from_state(&cli, &state).unwrap();
    assert!(result.swap_events.is_empty());
    assert_eq!(result.performance.swap_stats.total_swaps, 0);
    assert_eq!(result.performance.swap_stats.pool_based_swaps, 0);
    assert_eq!(result.performance.swap_stats.fallback_swaps, 0);
    assert!((result.performance.swap_stats.fallback_rate - 0.0).abs() < 1e-10);
}

#[test]
fn from_state_all_pool_based_swaps() {
    let cli = make_cli("2025-01-01", "2025-01-31");
    let mut state = PortfolioState::new(yocto(NEAR_100_YOCTO));
    state
        .swap_events
        .push(make_swap_event(SwapMethod::PoolBased, vec![1]));
    state
        .swap_events
        .push(make_swap_event(SwapMethod::PoolBased, vec![2, 3]));

    let result = SimulationResult::from_state(&cli, &state).unwrap();
    assert_eq!(result.performance.swap_stats.total_swaps, 2);
    assert_eq!(result.performance.swap_stats.pool_based_swaps, 2);
    assert_eq!(result.performance.swap_stats.fallback_swaps, 0);
    assert!((result.performance.swap_stats.fallback_rate - 0.0).abs() < 1e-10);
}

#[test]
fn from_state_mixed_swap_methods() {
    let cli = make_cli("2025-01-01", "2025-01-31");
    let mut state = PortfolioState::new(yocto(NEAR_100_YOCTO));
    state
        .swap_events
        .push(make_swap_event(SwapMethod::PoolBased, vec![1]));
    state
        .swap_events
        .push(make_swap_event(SwapMethod::DbRate, vec![]));
    state
        .swap_events
        .push(make_swap_event(SwapMethod::PoolBased, vec![2]));
    state
        .swap_events
        .push(make_swap_event(SwapMethod::DbRate, vec![]));

    let result = SimulationResult::from_state(&cli, &state).unwrap();
    assert_eq!(result.performance.swap_stats.total_swaps, 4);
    assert_eq!(result.performance.swap_stats.pool_based_swaps, 2);
    assert_eq!(result.performance.swap_stats.fallback_swaps, 2);
    assert!((result.performance.swap_stats.fallback_rate - 0.5).abs() < 1e-10);
}

#[test]
fn from_state_all_fallback_swaps() {
    let cli = make_cli("2025-01-01", "2025-01-31");
    let mut state = PortfolioState::new(yocto(NEAR_100_YOCTO));
    state
        .swap_events
        .push(make_swap_event(SwapMethod::DbRate, vec![]));
    state
        .swap_events
        .push(make_swap_event(SwapMethod::DbRate, vec![]));
    state
        .swap_events
        .push(make_swap_event(SwapMethod::DbRate, vec![]));

    let result = SimulationResult::from_state(&cli, &state).unwrap();
    assert_eq!(result.performance.swap_stats.total_swaps, 3);
    assert_eq!(result.performance.swap_stats.pool_based_swaps, 0);
    assert_eq!(result.performance.swap_stats.fallback_swaps, 3);
    assert!((result.performance.swap_stats.fallback_rate - 1.0).abs() < 1e-10);
}

#[test]
fn from_state_swap_event_entry_mapping() {
    let cli = make_cli("2025-01-01", "2025-01-31");
    let ts = Utc.with_ymd_and_hms(2025, 1, 5, 0, 0, 0).unwrap();
    let token_a: TokenAccount = "token_a.near".parse().unwrap();
    let token_b: TokenAccount = "token_b.near".parse().unwrap();

    let mut state = PortfolioState::new(yocto(NEAR_100_YOCTO));
    state.swap_events.push(SwapEvent {
        timestamp: ts,
        token_in: token_a,
        amount_in: TokenAmount::from_smallest_units(BigDecimal::from(1_000_000u64), 6),
        token_out: token_b,
        amount_out: TokenAmount::from_smallest_units(BigDecimal::from(500_000u64), 24),
        swap_method: SwapMethod::PoolBased,
        pool_ids: vec![42, 99],
    });

    let result = SimulationResult::from_state(&cli, &state).unwrap();
    assert_eq!(result.swap_events.len(), 1);
    let entry = &result.swap_events[0];
    assert_eq!(entry.token_in, "token_a.near");
    assert_eq!(entry.token_out, "token_b.near");
    assert_eq!(entry.amount_in_raw, 1_000_000);
    assert_eq!(entry.amount_out_raw, 500_000);
    assert_eq!(entry.swap_method, SwapMethod::PoolBased);
    assert_eq!(entry.pool_ids, vec![42, 99]);
}

#[test]
fn performance_includes_non_default_swap_stats() {
    let snapshots = vec![make_snapshot(110.0)];
    let perf = calculate_performance(PerformanceInput {
        initial_capital: 100.0,
        snapshots: &snapshots,
        realized_pnl: 0,
        trade_count: 5,
        liquidation_count: 0,
        rebalance_interval_days: 1,
        swap_stats: SwapStats {
            total_swaps: 10,
            pool_based_swaps: 7,
            fallback_swaps: 3,
            fallback_rate: 0.3,
        },
    });
    assert_eq!(perf.swap_stats.total_swaps, 10);
    assert_eq!(perf.swap_stats.pool_based_swaps, 7);
    assert_eq!(perf.swap_stats.fallback_swaps, 3);
    assert!((perf.swap_stats.fallback_rate - 0.3).abs() < 1e-10);
}
