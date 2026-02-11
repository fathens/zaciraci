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

// ---------------------------------------------------------------------------
// Constructor
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
// execute_simulated_swap
// ---------------------------------------------------------------------------

#[test]
fn swap_wnear_to_token_updates_state() {
    let mut state = PortfolioState::new(NEAR_100);
    let wnear = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_string();

    // Buy 50 NEAR worth of TOKEN_A (1:1 rate, 24 decimals)
    state.execute_simulated_swap(&wnear, NEAR_50, TOKEN_A, NEAR_50);

    assert_eq!(state.cash_balance, NEAR_50);
    assert_eq!(state.holdings[TOKEN_A], NEAR_50);
    assert_eq!(state.cost_basis[TOKEN_A], NEAR_50);
    assert_eq!(state.realized_pnl, 0); // no sell, no P&L
}

#[test]
fn swap_token_to_wnear_updates_state_and_pnl() {
    let mut state = PortfolioState::new(0);
    let wnear = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_string();

    // Set up: hold TOKEN_A with cost basis
    state.holdings.insert(TOKEN_A.to_string(), NEAR_100);
    state.cost_basis.insert(TOKEN_A.to_string(), NEAR_100);

    // Sell all TOKEN_A for 120 NEAR (profit)
    let sell_proceeds = 120_000_000_000_000_000_000_000_000u128;
    state.execute_simulated_swap(TOKEN_A, NEAR_100, &wnear, sell_proceeds);

    // TOKEN_A should be fully removed
    assert!(!state.holdings.contains_key(TOKEN_A));
    assert!(!state.cost_basis.contains_key(TOKEN_A));
    assert_eq!(state.cash_balance, sell_proceeds);

    // Realized P&L: 120 - 100 = 20 NEAR in yocto
    let expected_pnl = sell_proceeds as i128 - NEAR_100 as i128;
    assert_eq!(state.realized_pnl, expected_pnl);
}

#[test]
fn swap_partial_sell_adjusts_cost_basis() {
    let mut state = PortfolioState::new(0);
    let wnear = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_string();

    // Set up: hold 100 units with 100 NEAR cost basis
    state.holdings.insert(TOKEN_A.to_string(), NEAR_100);
    state.cost_basis.insert(TOKEN_A.to_string(), NEAR_100);

    // Sell half
    state.execute_simulated_swap(TOKEN_A, NEAR_50, &wnear, NEAR_50);

    assert_eq!(state.holdings[TOKEN_A], NEAR_50);
    assert_eq!(state.cost_basis[TOKEN_A], NEAR_50);
    // Sold at cost → 0 P&L
    assert_eq!(state.realized_pnl, 0);
}

#[test]
fn swap_wnear_to_token_multiple_buys_accumulate_cost() {
    let mut state = PortfolioState::new(NEAR_100);
    let wnear = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_string();

    // Buy 1: 30 NEAR
    let buy1 = 30_000_000_000_000_000_000_000_000u128;
    state.execute_simulated_swap(&wnear, buy1, TOKEN_A, buy1);

    // Buy 2: 20 NEAR
    let buy2 = 20_000_000_000_000_000_000_000_000u128;
    state.execute_simulated_swap(&wnear, buy2, TOKEN_A, buy2);

    assert_eq!(state.cash_balance, NEAR_50);
    assert_eq!(state.holdings[TOKEN_A], buy1 + buy2);
    assert_eq!(state.cost_basis[TOKEN_A], buy1 + buy2);
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
