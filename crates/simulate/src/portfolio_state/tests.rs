use super::*;
use bigdecimal::BigDecimal;
use chrono::{NaiveDate, TimeZone, Utc};
use serial_test::serial;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// MockRateProvider
// ---------------------------------------------------------------------------

/// Mock rate provider for unit tests. Returns pre-configured rates and decimals.
struct MockRateProvider {
    /// token_id -> ExchangeRate
    rates: HashMap<String, ExchangeRate>,
    /// token_id -> decimals
    decimals_map: HashMap<String, u8>,
}

impl MockRateProvider {
    fn new() -> Self {
        Self {
            rates: HashMap::new(),
            decimals_map: HashMap::new(),
        }
    }

    /// Add a token with a rate specified as an integer BigDecimal (avoids scientific notation issues).
    fn with_token(mut self, token_id: &str, raw_rate: BigDecimal, decimals: u8) -> Self {
        self.rates.insert(
            token_id.to_string(),
            ExchangeRate::from_raw_rate(raw_rate, decimals),
        );
        self.decimals_map.insert(token_id.to_string(), decimals);
        self
    }
}

impl RateProvider for MockRateProvider {
    async fn get_rate(
        &self,
        token: &TokenOutAccount,
        _sim_day: DateTime<Utc>,
    ) -> Option<ExchangeRate> {
        self.rates.get(&token.to_string()).cloned()
    }

    async fn get_decimals(&self, token_id: &str) -> u8 {
        self.decimals_map.get(token_id).copied().unwrap_or(24)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const NEAR_100: u128 = 100_000_000_000_000_000_000_000_000; // 100 NEAR in yocto
const NEAR_50: u128 = 50_000_000_000_000_000_000_000_000; // 50 NEAR in yocto

fn sim_day() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2025, 6, 15, 0, 0, 0).unwrap()
}

/// Rate for 24-decimal token: 1 token = 1 NEAR → raw_rate = 10^24
fn rate_24d() -> BigDecimal {
    BigDecimal::from(10u128.pow(24))
}

/// Rate for 6-decimal token: 1 token = 1 NEAR → raw_rate = 10^6
fn rate_6d() -> BigDecimal {
    BigDecimal::from(10u128.pow(6))
}

/// Token A: 24 decimals, 1 token = 1 NEAR
const TOKEN_A: &str = "token-a.near";
const TOKEN_A_DECIMALS: u8 = 24;

/// Token B: 6 decimals, 1 token = 1 NEAR
const TOKEN_B: &str = "token-b.near";
const TOKEN_B_DECIMALS: u8 = 6;

fn provider_with_a() -> MockRateProvider {
    MockRateProvider::new().with_token(TOKEN_A, rate_24d(), TOKEN_A_DECIMALS)
}

fn provider_with_ab() -> MockRateProvider {
    MockRateProvider::new()
        .with_token(TOKEN_A, rate_24d(), TOKEN_A_DECIMALS)
        .with_token(TOKEN_B, rate_6d(), TOKEN_B_DECIMALS)
}

fn token_out(id: &str) -> TokenOutAccount {
    id.parse().unwrap()
}

// ---------------------------------------------------------------------------
// Existing tests (migrated from inline)
// ---------------------------------------------------------------------------

#[test]
fn new_sets_initial_capital() {
    let state = PortfolioState::new(NEAR_100);
    assert_eq!(state.cash_balance, NEAR_100);
    assert!(state.holdings.is_empty());
    assert!(state.decimals.is_empty());
    assert!(state.snapshots.is_empty());
    assert!(state.trades.is_empty());
}

#[test]
fn new_zero_capital() {
    let state = PortfolioState::new(0);
    assert_eq!(state.cash_balance, 0);
}

// ---------------------------------------------------------------------------
// Hold
// ---------------------------------------------------------------------------

#[tokio::test]
async fn apply_hold_does_nothing() {
    let mut state = PortfolioState::new(NEAR_100);
    let provider = provider_with_a();

    state
        .apply_actions(&[TradingAction::Hold], sim_day(), &provider)
        .await
        .unwrap();

    assert_eq!(state.cash_balance, NEAR_100);
    assert!(state.holdings.is_empty());
    assert!(state.trades.is_empty());
}

// ---------------------------------------------------------------------------
// AddPosition
// ---------------------------------------------------------------------------

#[tokio::test]
async fn apply_add_position_buys_token() {
    let mut state = PortfolioState::new(NEAR_100);
    let provider = provider_with_a();

    let actions = vec![TradingAction::AddPosition {
        token: token_out(TOKEN_A),
        weight: 0.5, // spend 50% of cash
    }];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();

    // Cash should decrease (50% of 100 NEAR, with f64 precision)
    assert!(state.cash_balance < NEAR_100);
    // Should have bought tokens
    assert!(state.holdings.contains_key(TOKEN_A));
    assert!(state.holdings[TOKEN_A] > 0);
    // One trade record
    assert_eq!(state.trades.len(), 1);
    assert_eq!(state.trades[0].action, "add_position");
    assert_eq!(state.trades[0].token, TOKEN_A);
}

#[tokio::test]
async fn apply_add_position_zero_weight_does_nothing() {
    let mut state = PortfolioState::new(NEAR_100);
    let provider = provider_with_a();

    let actions = vec![TradingAction::AddPosition {
        token: token_out(TOKEN_A),
        weight: 0.0,
    }];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();

    assert_eq!(state.cash_balance, NEAR_100);
    assert!(state.holdings.is_empty());
    assert!(state.trades.is_empty());
}

#[tokio::test]
async fn apply_add_position_rate_unavailable_returns_zero() {
    let mut state = PortfolioState::new(NEAR_100);
    // Provider has no rates configured
    let provider = MockRateProvider::new();

    let actions = vec![TradingAction::AddPosition {
        token: token_out(TOKEN_A),
        weight: 0.5,
    }];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();

    // yocto_to_token_amount returns 0 → buy_amount is 0 → nothing happens
    assert_eq!(state.cash_balance, NEAR_100);
    assert!(state.holdings.is_empty());
}

// ---------------------------------------------------------------------------
// ReducePosition
// ---------------------------------------------------------------------------

#[tokio::test]
async fn apply_reduce_position_sells_portion() {
    // Use 6-decimal token with small amounts to avoid f64 precision issues
    // in `(current as f64 * weight) as u128`
    let holding = 1_000_000u128; // 1 token (6 decimals), fits in f64 exactly
    let mut state = PortfolioState::new(0);
    state.holdings.insert(TOKEN_B.to_string(), holding);
    let provider = provider_with_ab();

    let actions = vec![TradingAction::ReducePosition {
        token: token_out(TOKEN_B),
        weight: 0.5, // sell 50% of holdings
    }];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();

    // Holdings should decrease by 50%
    assert_eq!(state.holdings[TOKEN_B], 500_000);
    // Cash should increase (0.5 token at 1 NEAR/token = 0.5 NEAR)
    assert!(state.cash_balance > 0);
    assert_eq!(state.trades.len(), 1);
    assert_eq!(state.trades[0].action, "reduce_position");
}

#[tokio::test]
async fn apply_reduce_position_no_holding_does_nothing() {
    let mut state = PortfolioState::new(NEAR_100);
    let provider = provider_with_a();

    let actions = vec![TradingAction::ReducePosition {
        token: token_out(TOKEN_A),
        weight: 0.5,
    }];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();

    assert_eq!(state.cash_balance, NEAR_100);
    assert!(state.trades.is_empty());
}

// ---------------------------------------------------------------------------
// Sell
// ---------------------------------------------------------------------------

#[tokio::test]
async fn apply_sell_to_wnear_converts_to_cash() {
    let mut state = PortfolioState::new(NEAR_50);
    state.holdings.insert(TOKEN_A.to_string(), NEAR_50);
    let provider = provider_with_a();

    let wnear_str = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_string();
    let actions = vec![TradingAction::Sell {
        token: token_out(TOKEN_A),
        target: wnear_str.parse().unwrap(),
    }];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();

    // Token A should be fully removed
    assert!(!state.holdings.contains_key(TOKEN_A));
    // Cash should increase (original 50 + sell proceeds)
    assert!(state.cash_balance > NEAR_50);
    // Only one sell trade (no buy since target is wnear)
    assert_eq!(state.trades.len(), 1);
    assert_eq!(state.trades[0].action, "sell");
}

#[tokio::test]
async fn apply_sell_to_other_token_via_cash() {
    let mut state = PortfolioState::new(0);
    state.holdings.insert(TOKEN_A.to_string(), NEAR_50);
    let provider = provider_with_ab();

    let actions = vec![TradingAction::Sell {
        token: token_out(TOKEN_A),
        target: token_out(TOKEN_B),
    }];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();

    // Token A should be fully removed
    assert!(!state.holdings.contains_key(TOKEN_A));
    // Token B should be acquired
    assert!(state.holdings.contains_key(TOKEN_B));
    assert!(state.holdings[TOKEN_B] > 0);
    // Two trades: sell A, buy B
    assert_eq!(state.trades.len(), 2);
    assert_eq!(state.trades[0].action, "sell");
    assert_eq!(state.trades[1].action, "buy");
}

#[tokio::test]
async fn apply_sell_zero_holding_does_nothing() {
    let mut state = PortfolioState::new(NEAR_100);
    let provider = provider_with_a();

    let wnear_str = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_string();
    let actions = vec![TradingAction::Sell {
        token: token_out(TOKEN_A),
        target: wnear_str.parse().unwrap(),
    }];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();

    assert_eq!(state.cash_balance, NEAR_100);
    assert!(state.trades.is_empty());
}

// ---------------------------------------------------------------------------
// Switch
// ---------------------------------------------------------------------------

#[tokio::test]
async fn apply_switch_converts_holdings() {
    let mut state = PortfolioState::new(0);
    state.holdings.insert(TOKEN_A.to_string(), NEAR_50);
    let provider = provider_with_ab();

    let actions = vec![TradingAction::Switch {
        from: token_out(TOKEN_A),
        to: token_out(TOKEN_B),
    }];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();

    // Token A fully removed
    assert!(!state.holdings.contains_key(TOKEN_A));
    // Token B acquired
    assert!(state.holdings.contains_key(TOKEN_B));
    assert!(state.holdings[TOKEN_B] > 0);
    // One switch trade record
    assert_eq!(state.trades.len(), 1);
    assert_eq!(state.trades[0].action, "switch");
    assert!(state.trades[0].token.contains("->"));
}

#[tokio::test]
async fn apply_switch_zero_holding_does_nothing() {
    let mut state = PortfolioState::new(NEAR_100);
    let provider = provider_with_ab();

    let actions = vec![TradingAction::Switch {
        from: token_out(TOKEN_A),
        to: token_out(TOKEN_B),
    }];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();

    assert!(state.holdings.is_empty());
    assert!(state.trades.is_empty());
}

// ---------------------------------------------------------------------------
// Rebalance
// ---------------------------------------------------------------------------

#[tokio::test]
async fn apply_rebalance_sells_overweight_buys_underweight() {
    let provider = provider_with_ab();
    let mut state = PortfolioState::new(0);
    // Hold 2M units of TOKEN_B (2 tokens = 2 NEAR worth)
    state.holdings.insert(TOKEN_B.to_string(), 2_000_000u128);

    let mut target_weights = BTreeMap::new();
    target_weights.insert(token_out(TOKEN_A), 0.5);
    target_weights.insert(token_out(TOKEN_B), 0.5);

    let actions = vec![TradingAction::Rebalance { target_weights }];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();

    // Should have sold some B and bought some A
    assert!(state.holdings.contains_key(TOKEN_B));
    assert!(state.holdings.contains_key(TOKEN_A));
    // B should be less than original
    assert!(state.holdings[TOKEN_B] < 2_000_000);
    // A should have been bought
    assert!(state.holdings[TOKEN_A] > 0);
    // Should have at least 2 trades (sell B + buy A)
    assert!(state.trades.len() >= 2);
}

#[tokio::test]
async fn apply_rebalance_zero_total_value_does_nothing() {
    let mut state = PortfolioState::new(0);
    let provider = provider_with_a();

    let mut target_weights = BTreeMap::new();
    target_weights.insert(token_out(TOKEN_A), 1.0);

    let actions = vec![TradingAction::Rebalance { target_weights }];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();

    assert!(state.holdings.is_empty());
    assert!(state.trades.is_empty());
}

#[tokio::test]
async fn apply_rebalance_skips_wnear_token() {
    let wnear_str = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_string();
    let mut state = PortfolioState::new(NEAR_100); // 100 NEAR cash
    let provider = provider_with_a();

    let mut target_weights = BTreeMap::new();
    target_weights.insert(wnear_str.parse().unwrap(), 0.5);
    target_weights.insert(token_out(TOKEN_A), 0.5);

    let actions = vec![TradingAction::Rebalance { target_weights }];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();

    // wnear target should be skipped in rebalance logic
    // Token A should have been bought with some cash
    assert!(state.holdings.contains_key(TOKEN_A));
}

// ---------------------------------------------------------------------------
// Value calculation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn total_value_near_cash_only() {
    let state = PortfolioState::new(NEAR_100);
    let provider = MockRateProvider::new();

    let value = state
        .calculate_total_value_near(sim_day(), &provider)
        .await
        .unwrap();
    assert!((value - 100.0).abs() < 1e-6, "expected 100.0, got {value}");
}

#[tokio::test]
async fn total_value_near_with_holdings() {
    let mut state = PortfolioState::new(NEAR_50); // 50 NEAR cash
    state.holdings.insert(TOKEN_A.to_string(), NEAR_50);
    let provider = provider_with_a();

    let value = state
        .calculate_total_value_near(sim_day(), &provider)
        .await
        .unwrap();
    // 50 NEAR cash + 50 NEAR in token A = 100 NEAR
    assert!((value - 100.0).abs() < 1e-6, "expected ~100.0, got {value}");
}

#[tokio::test]
async fn total_value_near_missing_rate_skips_token() {
    let mut state = PortfolioState::new(NEAR_50);
    state.holdings.insert(TOKEN_A.to_string(), NEAR_50);
    let provider = MockRateProvider::new(); // empty: no rates

    let value = state
        .calculate_total_value_near(sim_day(), &provider)
        .await
        .unwrap();
    // Only cash should count
    assert!((value - 50.0).abs() < 1e-6, "expected 50.0, got {value}");
}

// ---------------------------------------------------------------------------
// record_snapshot
// ---------------------------------------------------------------------------

#[tokio::test]
async fn record_snapshot_appends_snapshot() {
    let mut state = PortfolioState::new(NEAR_100);
    let provider = MockRateProvider::new();

    state.record_snapshot(sim_day(), &provider).await.unwrap();

    assert_eq!(state.snapshots.len(), 1);
    assert_eq!(state.snapshots[0].cash_balance, NEAR_100);
    assert!(state.snapshots[0].holdings.is_empty());
}

#[tokio::test]
async fn record_snapshot_captures_correct_value() {
    let mut state = PortfolioState::new(NEAR_50);
    state.holdings.insert(TOKEN_A.to_string(), NEAR_50);
    let provider = provider_with_a();

    state.record_snapshot(sim_day(), &provider).await.unwrap();

    assert_eq!(state.snapshots.len(), 1);
    assert!(
        (state.snapshots[0].total_value_near - 100.0).abs() < 1e-6,
        "expected ~100.0, got {}",
        state.snapshots[0].total_value_near
    );
    assert_eq!(state.snapshots[0].holdings[TOKEN_A], NEAR_50);
}

// ---------------------------------------------------------------------------
// Multiple actions applied sequentially
// ---------------------------------------------------------------------------

#[tokio::test]
async fn multiple_actions_applied_sequentially() {
    let mut state = PortfolioState::new(NEAR_100);
    let provider = provider_with_ab();

    let actions = vec![
        TradingAction::AddPosition {
            token: token_out(TOKEN_A),
            weight: 0.3,
        },
        TradingAction::AddPosition {
            token: token_out(TOKEN_B),
            weight: 0.3,
        },
        TradingAction::Hold,
    ];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();

    // Cash reduced by 30% + 30% of remaining
    assert!(state.cash_balance < NEAR_100);
    // Both tokens should have holdings
    assert!(state.holdings.contains_key(TOKEN_A));
    assert!(state.holdings.contains_key(TOKEN_B));
    // Two add_position trades (Hold doesn't generate trades)
    assert_eq!(state.trades.len(), 2);
}

// ---------------------------------------------------------------------------
// Cost basis tracking
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cost_basis_tracks_buy() {
    let mut state = PortfolioState::new(NEAR_100);
    let provider = provider_with_a();

    let actions = vec![TradingAction::AddPosition {
        token: token_out(TOKEN_A),
        weight: 0.5,
    }];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();

    // Cost basis should be set to the buy amount in yocto
    assert!(
        state.cost_basis.contains_key(TOKEN_A),
        "cost_basis should track TOKEN_A"
    );
    let expected_cost = NEAR_100 - state.cash_balance; // what was spent
    assert_eq!(
        state.cost_basis[TOKEN_A], expected_cost,
        "cost_basis should equal spent amount"
    );
}

#[tokio::test]
async fn cost_basis_average_on_multiple_buys() {
    let mut state = PortfolioState::new(NEAR_100);
    let provider = provider_with_a();

    // Buy 1: 30% of 100 NEAR
    let actions = vec![TradingAction::AddPosition {
        token: token_out(TOKEN_A),
        weight: 0.3,
    }];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();
    let cost_after_first = state.cost_basis[TOKEN_A];

    // Buy 2: 30% of remaining
    let actions = vec![TradingAction::AddPosition {
        token: token_out(TOKEN_A),
        weight: 0.3,
    }];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();

    // Total cost should be sum of both buys
    assert!(
        state.cost_basis[TOKEN_A] > cost_after_first,
        "cost_basis should increase after second buy"
    );
    let total_spent = NEAR_100 - state.cash_balance;
    assert_eq!(
        state.cost_basis[TOKEN_A], total_spent,
        "cost_basis should equal total spent"
    );
}

#[tokio::test]
async fn cost_basis_cleared_on_full_sell() {
    let mut state = PortfolioState::new(NEAR_100);
    let provider = provider_with_a();

    // Buy token A
    let actions = vec![TradingAction::AddPosition {
        token: token_out(TOKEN_A),
        weight: 0.5,
    }];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();
    assert!(state.cost_basis.contains_key(TOKEN_A));

    // Sell all of token A
    let wnear_str = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_string();
    let actions = vec![TradingAction::Sell {
        token: token_out(TOKEN_A),
        target: wnear_str.parse().unwrap(),
    }];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();

    // Cost basis should be cleared
    assert!(
        !state.cost_basis.contains_key(TOKEN_A),
        "cost_basis should be removed after full sell"
    );
}

#[tokio::test]
async fn realized_pnl_partial_sell() {
    // Use 6-decimal token for precision
    let mut state = PortfolioState::new(NEAR_100);
    let provider = provider_with_ab();

    // Buy 1 TOKEN_B (1_000_000 units, 6 decimals) for ~1 NEAR
    let actions = vec![TradingAction::AddPosition {
        token: token_out(TOKEN_B),
        weight: 0.5,
    }];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();

    let cost_before = state.cost_basis[TOKEN_B];
    let holding_before = state.holdings[TOKEN_B];

    // Reduce 50%
    let actions = vec![TradingAction::ReducePosition {
        token: token_out(TOKEN_B),
        weight: 0.5,
    }];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();

    // Cost basis should be approximately halved
    let cost_after = state.cost_basis.get(TOKEN_B).copied().unwrap_or(0);
    let expected_remaining_cost = cost_before / 2;
    let tolerance = cost_before / 100; // 1% tolerance
    assert!(
        (cost_after as i128 - expected_remaining_cost as i128).unsigned_abs() < tolerance,
        "cost should be ~halved: {} vs expected ~{}",
        cost_after,
        expected_remaining_cost
    );

    // Holdings should be halved
    let holding_after = state.holdings[TOKEN_B];
    assert_eq!(
        holding_after,
        holding_before / 2,
        "holdings should be halved"
    );

    // Trade should have realized_pnl_near set
    let sell_trade = state
        .trades
        .iter()
        .find(|t| t.action == "reduce_position")
        .unwrap();
    assert!(
        sell_trade.realized_pnl_near.is_some(),
        "reduce_position should have realized_pnl"
    );
}

#[tokio::test]
async fn realized_pnl_by_token_separate() {
    let mut state = PortfolioState::new(NEAR_100);
    let provider = provider_with_ab();

    // Buy both tokens
    let actions = vec![
        TradingAction::AddPosition {
            token: token_out(TOKEN_A),
            weight: 0.25,
        },
        TradingAction::AddPosition {
            token: token_out(TOKEN_B),
            weight: 0.25,
        },
    ];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();

    // Sell token A to WNEAR
    let wnear_str = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_string();
    let actions = vec![TradingAction::Sell {
        token: token_out(TOKEN_A),
        target: wnear_str.parse().unwrap(),
    }];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();

    // Only TOKEN_A should have realized P&L
    assert!(
        state.realized_pnl_by_token.contains_key(TOKEN_A),
        "TOKEN_A should have realized P&L"
    );
    // TOKEN_B should not have realized P&L yet (not sold)
    assert!(
        !state.realized_pnl_by_token.contains_key(TOKEN_B),
        "TOKEN_B should not have realized P&L yet"
    );
}

#[tokio::test]
async fn snapshot_includes_realized_pnl() {
    let mut state = PortfolioState::new(NEAR_100);
    let provider = provider_with_a();

    // First snapshot: no trades yet
    state.record_snapshot(sim_day(), &provider).await.unwrap();
    assert!(
        (state.snapshots[0].realized_pnl_near - 0.0).abs() < 1e-10,
        "initial snapshot should have 0 realized P&L"
    );

    // Buy and sell to generate realized P&L
    let actions = vec![TradingAction::AddPosition {
        token: token_out(TOKEN_A),
        weight: 0.5,
    }];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();

    let wnear_str = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_string();
    let actions = vec![TradingAction::Sell {
        token: token_out(TOKEN_A),
        target: wnear_str.parse().unwrap(),
    }];
    state
        .apply_actions(&actions, sim_day(), &provider)
        .await
        .unwrap();

    state.record_snapshot(sim_day(), &provider).await.unwrap();

    // Second snapshot should record the realized P&L
    assert_eq!(state.snapshots.len(), 2);
    // The realized P&L is the same value as state.realized_pnl converted to NEAR
    let expected = state.realized_pnl as f64 / 1e24;
    assert!(
        (state.snapshots[1].realized_pnl_near - expected).abs() < 1e-10,
        "snapshot realized_pnl_near {} should equal {}",
        state.snapshots[1].realized_pnl_near,
        expected
    );
}

// ---------------------------------------------------------------------------
// Liquidation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn liquidate_all_sells_everything() {
    let mut state = PortfolioState::new(0);
    state.holdings.insert(TOKEN_A.to_string(), NEAR_50);
    state.holdings.insert(TOKEN_B.to_string(), 1_000_000);
    state.cost_basis.insert(TOKEN_A.to_string(), NEAR_50); // cost = 50 NEAR
    state
        .cost_basis
        .insert(TOKEN_B.to_string(), 1_000_000_000_000_000_000_000_000); // cost = 1 NEAR

    let provider = provider_with_ab();
    state.liquidate_all(sim_day(), &provider).await.unwrap();

    assert!(
        state.holdings.is_empty() || state.holdings.values().all(|&v| v == 0),
        "all holdings should be sold"
    );
    assert!(state.cash_balance > 0, "cash should increase");
}

#[tokio::test]
async fn liquidate_all_records_liquidation_trades() {
    let mut state = PortfolioState::new(0);
    state.holdings.insert(TOKEN_A.to_string(), NEAR_50);
    state.cost_basis.insert(TOKEN_A.to_string(), NEAR_50);
    let provider = provider_with_a();

    state.liquidate_all(sim_day(), &provider).await.unwrap();

    assert!(!state.trades.is_empty(), "should have liquidation trades");
    assert_eq!(
        state.trades[0].action, "liquidation",
        "action should be liquidation"
    );
}

#[tokio::test]
async fn liquidate_all_empty_portfolio_noop() {
    let mut state = PortfolioState::new(NEAR_100);
    let provider = provider_with_a();

    state.liquidate_all(sim_day(), &provider).await.unwrap();

    assert!(state.trades.is_empty(), "no trades for empty portfolio");
    assert_eq!(state.cash_balance, NEAR_100, "cash unchanged");
}

#[tokio::test]
async fn liquidate_all_computes_pnl() {
    let mut state = PortfolioState::new(0);
    state.holdings.insert(TOKEN_A.to_string(), NEAR_50);
    // Cost basis = 50 NEAR (bought at 1:1)
    state.cost_basis.insert(TOKEN_A.to_string(), NEAR_50);

    let provider = provider_with_a();
    state.liquidate_all(sim_day(), &provider).await.unwrap();

    // With 1:1 rate, selling 50 NEAR worth of tokens should yield ~0 P&L
    assert!(
        state.trades[0].realized_pnl_near.is_some(),
        "liquidation should have P&L"
    );
    // P&L should be close to 0 (bought and sold at same rate)
    let pnl = state.trades[0].realized_pnl_near.unwrap();
    assert!(
        pnl.abs() < 1.0,
        "P&L should be near zero for same-rate trade, got {}",
        pnl
    );
}

// ===========================================================================
// DB Integration Tests
// ===========================================================================
//
// These tests require a running PostgreSQL at localhost:5433 (test DB).
// They use #[serial(token_rates)] to avoid parallel mutation of the
// token_rates table.

/// WNEAR quote account (depends on USE_MAINNET; defaults to wrap.testnet)
fn wnear_quote() -> common::types::TokenInAccount {
    blockchain::ref_finance::token_account::WNEAR_TOKEN.to_in()
}

/// Seed timestamp within 24h of `integration_sim_day()`
fn seed_ts() -> chrono::NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 2, 9)
        .unwrap()
        .and_hms_opt(6, 0, 0)
        .unwrap()
}

/// Simulation timestamp used by integration tests
fn integration_sim_day() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 2, 9, 12, 0, 0).unwrap()
}

const INT_TOKEN_A: &str = "test-token-a.testnet";
const INT_TOKEN_A_DECIMALS: u8 = 6;

const INT_TOKEN_B: &str = "test-token-b.testnet";
const INT_TOKEN_B_DECIMALS: u8 = 6;

/// Delete all records with timestamp < now() from token_rates
async fn cleanup_token_rates() -> anyhow::Result<()> {
    TokenRate::cleanup_old_records(0).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    Ok(())
}

/// Insert a single token rate record into the DB
async fn seed_rate(
    base: &str,
    raw_rate: BigDecimal,
    decimals: u8,
    timestamp: chrono::NaiveDateTime,
) -> anyhow::Result<()> {
    let token_rate = TokenRate {
        base: base.parse().unwrap(),
        quote: wnear_quote(),
        exchange_rate: ExchangeRate::from_raw_rate(raw_rate, decimals),
        timestamp,
        rate_calc_near: 10,
        swap_path: None,
    };
    TokenRate::batch_insert(&[token_rate]).await?;
    Ok(())
}

/// Clean table, seed rates, and reload token decimals cache
async fn setup_integration(tokens: &[(&str, BigDecimal, u8)]) -> anyhow::Result<()> {
    cleanup_token_rates().await?;
    for (base, rate, decimals) in tokens {
        seed_rate(base, rate.clone(), *decimals, seed_ts()).await?;
    }
    trade::token_cache::load_from_db().await?;
    Ok(())
}

#[tokio::test]
#[serial(token_rates)]
async fn integration_add_position_with_db_rate() -> anyhow::Result<()> {
    setup_integration(&[(INT_TOKEN_A, rate_6d(), INT_TOKEN_A_DECIMALS)]).await?;

    let mut state = PortfolioState::new(NEAR_100);
    let provider = DbRateProvider;

    let actions = vec![TradingAction::AddPosition {
        token: token_out(INT_TOKEN_A),
        weight: 0.5,
    }];
    state
        .apply_actions(&actions, integration_sim_day(), &provider)
        .await?;

    assert!(state.cash_balance < NEAR_100, "cash should decrease");
    assert!(
        state.holdings.contains_key(INT_TOKEN_A),
        "should hold token A"
    );
    assert!(state.holdings[INT_TOKEN_A] > 0, "amount should be positive");
    assert_eq!(state.trades.len(), 1);
    assert_eq!(state.trades[0].action, "add_position");

    cleanup_token_rates().await?;
    Ok(())
}

#[tokio::test]
#[serial(token_rates)]
async fn integration_sell_with_db_rate() -> anyhow::Result<()> {
    setup_integration(&[(INT_TOKEN_A, rate_6d(), INT_TOKEN_A_DECIMALS)]).await?;

    let mut state = PortfolioState::new(NEAR_50);
    state.holdings.insert(INT_TOKEN_A.to_string(), 1_000_000); // 1 token

    let provider = DbRateProvider;
    let wnear_str = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_string();

    let actions = vec![TradingAction::Sell {
        token: token_out(INT_TOKEN_A),
        target: wnear_str.parse().unwrap(),
    }];
    state
        .apply_actions(&actions, integration_sim_day(), &provider)
        .await?;

    assert!(
        !state.holdings.contains_key(INT_TOKEN_A),
        "token A should be sold"
    );
    assert!(state.cash_balance > NEAR_50, "cash should increase");
    assert_eq!(state.trades.len(), 1);
    assert_eq!(state.trades[0].action, "sell");

    cleanup_token_rates().await?;
    Ok(())
}

#[tokio::test]
#[serial(token_rates)]
async fn integration_rebalance_with_db_rate() -> anyhow::Result<()> {
    setup_integration(&[
        (INT_TOKEN_A, rate_6d(), INT_TOKEN_A_DECIMALS),
        (INT_TOKEN_B, rate_6d(), INT_TOKEN_B_DECIMALS),
    ])
    .await?;

    let mut state = PortfolioState::new(0);
    state.holdings.insert(INT_TOKEN_A.to_string(), 2_000_000); // 2 tokens = 2 NEAR

    let provider = DbRateProvider;

    let mut target_weights = BTreeMap::new();
    target_weights.insert(token_out(INT_TOKEN_A), 0.5);
    target_weights.insert(token_out(INT_TOKEN_B), 0.5);

    let actions = vec![TradingAction::Rebalance { target_weights }];
    state
        .apply_actions(&actions, integration_sim_day(), &provider)
        .await?;

    assert!(
        state.holdings.contains_key(INT_TOKEN_A),
        "should still hold A"
    );
    assert!(
        state.holdings.contains_key(INT_TOKEN_B),
        "should now hold B"
    );
    assert!(state.holdings[INT_TOKEN_A] < 2_000_000, "A should decrease");
    assert!(state.holdings[INT_TOKEN_B] > 0, "B should increase");
    assert!(state.trades.len() >= 2, "should have sell and buy trades");

    cleanup_token_rates().await?;
    Ok(())
}

#[tokio::test]
#[serial(token_rates)]
async fn integration_calculate_total_value_with_db() -> anyhow::Result<()> {
    setup_integration(&[(INT_TOKEN_A, rate_6d(), INT_TOKEN_A_DECIMALS)]).await?;

    let mut state = PortfolioState::new(NEAR_50);
    state.holdings.insert(INT_TOKEN_A.to_string(), 1_000_000); // 1 token = 1 NEAR

    let provider = DbRateProvider;
    let value = state
        .calculate_total_value_near(integration_sim_day(), &provider)
        .await?;

    // 50 NEAR cash + 1 token (1 NEAR) = 51 NEAR
    assert!((value - 51.0).abs() < 0.1, "expected ~51 NEAR, got {value}");

    cleanup_token_rates().await?;
    Ok(())
}

#[tokio::test]
#[serial(token_rates)]
async fn integration_record_snapshot_with_db() -> anyhow::Result<()> {
    setup_integration(&[(INT_TOKEN_A, rate_6d(), INT_TOKEN_A_DECIMALS)]).await?;

    let mut state = PortfolioState::new(NEAR_50);
    state.holdings.insert(INT_TOKEN_A.to_string(), 1_000_000);

    let provider = DbRateProvider;
    state
        .record_snapshot(integration_sim_day(), &provider)
        .await?;

    assert_eq!(state.snapshots.len(), 1);
    assert!(
        (state.snapshots[0].total_value_near - 51.0).abs() < 0.1,
        "expected ~51 NEAR, got {}",
        state.snapshots[0].total_value_near
    );
    assert_eq!(state.snapshots[0].holdings[INT_TOKEN_A], 1_000_000);
    assert_eq!(state.snapshots[0].cash_balance, NEAR_50);

    cleanup_token_rates().await?;
    Ok(())
}

#[tokio::test]
#[serial(token_rates)]
async fn integration_multiple_day_simulation() -> anyhow::Result<()> {
    let day1_ts = NaiveDate::from_ymd_opt(2026, 2, 8)
        .unwrap()
        .and_hms_opt(6, 0, 0)
        .unwrap();
    let day2_ts = seed_ts(); // 2026-02-09T06:00:00

    cleanup_token_rates().await?;
    seed_rate(INT_TOKEN_A, rate_6d(), INT_TOKEN_A_DECIMALS, day1_ts).await?;
    seed_rate(INT_TOKEN_A, rate_6d(), INT_TOKEN_A_DECIMALS, day2_ts).await?;
    trade::token_cache::load_from_db().await?;

    let provider = DbRateProvider;
    let mut state = PortfolioState::new(NEAR_100);

    // Day 1: Add position (50% of 100 NEAR)
    let day1 = Utc.with_ymd_and_hms(2026, 2, 8, 12, 0, 0).unwrap();
    let actions = vec![TradingAction::AddPosition {
        token: token_out(INT_TOKEN_A),
        weight: 0.5,
    }];
    state.apply_actions(&actions, day1, &provider).await?;
    state.record_snapshot(day1, &provider).await?;

    assert!(state.holdings.contains_key(INT_TOKEN_A));
    let holdings_after_day1 = state.holdings[INT_TOKEN_A];

    // Day 2: Reduce position by 50%
    let day2 = integration_sim_day();
    let actions = vec![TradingAction::ReducePosition {
        token: token_out(INT_TOKEN_A),
        weight: 0.5,
    }];
    state.apply_actions(&actions, day2, &provider).await?;
    state.record_snapshot(day2, &provider).await?;

    assert!(
        state.holdings[INT_TOKEN_A] < holdings_after_day1,
        "holdings should decrease after reduce"
    );
    assert_eq!(state.snapshots.len(), 2);
    assert_eq!(state.trades.len(), 2); // 1 add + 1 reduce

    cleanup_token_rates().await?;
    Ok(())
}
