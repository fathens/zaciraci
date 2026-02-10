use super::*;
use bigdecimal::BigDecimal;
use chrono::{TimeZone, Utc};
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
