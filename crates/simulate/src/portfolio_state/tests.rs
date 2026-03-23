use super::*;
use bigdecimal::BigDecimal;
use chrono::{NaiveDate, TimeZone, Utc};
use serial_test::serial;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// MockRateProvider
// ---------------------------------------------------------------------------

/// Mock rate provider for unit tests. Returns pre-configured rates.
struct MockRateProvider {
    /// token_id -> ExchangeRate
    rates: HashMap<String, ExchangeRate>,
}

impl MockRateProvider {
    fn new() -> Self {
        Self {
            rates: HashMap::new(),
        }
    }

    /// Add a token with a rate specified as an integer BigDecimal (avoids scientific notation issues).
    fn with_token(mut self, token_id: &str, raw_rate: BigDecimal, decimals: u8) -> Self {
        self.rates.insert(
            token_id.to_string(),
            ExchangeRate::from_raw_rate(raw_rate, decimals),
        );
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
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const NEAR_100: u128 = 100_000_000_000_000_000_000_000_000; // 100 NEAR in yocto
const NEAR_50: u128 = 50_000_000_000_000_000_000_000_000; // 50 NEAR in yocto

fn yocto(v: u128) -> YoctoValue {
    YoctoValue::from_yocto(BigDecimal::from(v))
}

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

fn token_a() -> TokenAccount {
    TOKEN_A.parse().unwrap()
}

fn token_b() -> TokenAccount {
    TOKEN_B.parse().unwrap()
}

fn token_amount_24d(raw: u128) -> TokenAmount {
    TokenAmount::from_smallest_units(BigDecimal::from(raw), TOKEN_A_DECIMALS)
}

fn token_amount_6d(raw: u128) -> TokenAmount {
    TokenAmount::from_smallest_units(BigDecimal::from(raw), TOKEN_B_DECIMALS)
}

fn wnear() -> TokenAccount {
    blockchain::ref_finance::token_account::WNEAR_TOKEN.clone()
}

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
    let state = PortfolioState::new(yocto(NEAR_100));
    assert_eq!(state.cash_balance, yocto(NEAR_100));
    assert!(state.holdings.is_empty());
    assert!(state.snapshots.is_empty());
    assert!(state.trades.is_empty());
    assert!(state.swap_events.is_empty());
}

#[test]
fn new_zero_capital() {
    let state = PortfolioState::new(YoctoValue::zero());
    assert_eq!(state.cash_balance, YoctoValue::zero());
}

// ---------------------------------------------------------------------------
// execute_simulated_swap
// ---------------------------------------------------------------------------

#[test]
fn swap_wnear_to_token_updates_state() {
    let mut state = PortfolioState::new(yocto(NEAR_100));
    let wnear = wnear();

    // Buy 50 NEAR worth of TOKEN_A (1:1 rate, 24 decimals)
    state.execute_simulated_swap(&wnear, NEAR_50, &token_a(), NEAR_50);

    assert_eq!(state.cash_balance, yocto(NEAR_50));
    assert_eq!(
        state.holdings[&token_a()].smallest_units(),
        &BigDecimal::from(NEAR_50)
    );
    assert_eq!(state.cost_basis[&token_a()], yocto(NEAR_50));
    assert_eq!(state.realized_pnl, 0); // no sell, no P&L
}

#[test]
fn swap_token_to_wnear_updates_state_and_pnl() {
    let mut state = PortfolioState::new(YoctoValue::zero());
    let wnear = wnear();

    // Set up: hold TOKEN_A with cost basis
    state.holdings.insert(token_a(), token_amount_24d(NEAR_100));
    state.cost_basis.insert(token_a(), yocto(NEAR_100));

    // Sell all TOKEN_A for 120 NEAR (profit)
    let sell_proceeds = 120_000_000_000_000_000_000_000_000u128;
    state.execute_simulated_swap(&token_a(), NEAR_100, &wnear, sell_proceeds);

    // TOKEN_A should be fully removed
    assert!(!state.holdings.contains_key(&token_a()));
    assert!(!state.cost_basis.contains_key(&token_a()));
    assert_eq!(state.cash_balance, yocto(sell_proceeds));

    // Realized P&L: 120 - 100 = 20 NEAR in yocto
    let expected_pnl = sell_proceeds as i128 - NEAR_100 as i128;
    assert_eq!(state.realized_pnl, expected_pnl);
}

#[test]
fn swap_partial_sell_adjusts_cost_basis() {
    let mut state = PortfolioState::new(YoctoValue::zero());
    let wnear = wnear();

    // Set up: hold 100 units with 100 NEAR cost basis
    state.holdings.insert(token_a(), token_amount_24d(NEAR_100));
    state.cost_basis.insert(token_a(), yocto(NEAR_100));

    // Sell half
    state.execute_simulated_swap(&token_a(), NEAR_50, &wnear, NEAR_50);

    assert_eq!(
        state.holdings[&token_a()].smallest_units(),
        &BigDecimal::from(NEAR_50)
    );
    assert_eq!(state.cost_basis[&token_a()], yocto(NEAR_50));
    // Sold at cost → 0 P&L
    assert_eq!(state.realized_pnl, 0);
}

#[test]
fn swap_wnear_to_token_multiple_buys_accumulate_cost() {
    let mut state = PortfolioState::new(yocto(NEAR_100));
    let wnear = wnear();

    // Buy 1: 30 NEAR
    let buy1 = 30_000_000_000_000_000_000_000_000u128;
    state.execute_simulated_swap(&wnear, buy1, &token_a(), buy1);

    // Buy 2: 20 NEAR
    let buy2 = 20_000_000_000_000_000_000_000_000u128;
    state.execute_simulated_swap(&wnear, buy2, &token_a(), buy2);

    assert_eq!(state.cash_balance, yocto(NEAR_50));
    assert_eq!(
        state.holdings[&token_a()].smallest_units(),
        &BigDecimal::from(buy1 + buy2)
    );
    assert_eq!(state.cost_basis[&token_a()], yocto(buy1 + buy2));
}

// ---------------------------------------------------------------------------
// execute_simulated_swap edge cases
// ---------------------------------------------------------------------------

#[test]
fn swap_token_to_token_non_wnear() {
    let mut state = PortfolioState::new(yocto(NEAR_100));

    // Set up: hold TOKEN_A
    state.holdings.insert(token_a(), token_amount_24d(NEAR_50));
    state.cost_basis.insert(token_a(), yocto(NEAR_50));

    // Swap TOKEN_A -> TOKEN_B (neither is WNEAR)
    let token_b_amount = 500_000u128; // 0.5 TOKEN_B (6 decimals)
    state.execute_simulated_swap(&token_a(), NEAR_50, &token_b(), token_b_amount);

    // TOKEN_A fully sold
    assert!(!state.holdings.contains_key(&token_a()));
    assert!(!state.cost_basis.contains_key(&token_a()));

    // TOKEN_B acquired, but no cost_basis tracked (non-WNEAR source)
    assert_eq!(
        state.holdings[&token_b()].smallest_units(),
        &BigDecimal::from(token_b_amount)
    );
    assert!(!state.cost_basis.contains_key(&token_b()));

    // Cash unchanged (no WNEAR involved)
    assert_eq!(state.cash_balance, yocto(NEAR_100));

    // Realized P&L for token-to-token: sell_proceeds = cost_of_sold → P&L = 0
    assert_eq!(state.realized_pnl, 0);
}

#[test]
fn swap_from_token_with_no_holdings() {
    let mut state = PortfolioState::new(yocto(NEAR_100));

    // Try selling TOKEN_A that we don't hold
    let wnear = wnear();
    state.execute_simulated_swap(&token_a(), NEAR_50, &wnear, NEAR_50);

    // Entire swap is skipped: no TOKEN_A deducted, no WNEAR added
    assert!(!state.holdings.contains_key(&token_a()));
    assert_eq!(state.cash_balance, yocto(NEAR_100));
    assert_eq!(state.realized_pnl, 0);
}

#[test]
fn swap_sell_more_than_holdings_scales_output() {
    let mut state = PortfolioState::new(YoctoValue::zero());
    let wnear = wnear();

    // Hold only 10 NEAR worth of TOKEN_A
    let ten_near = 10_000_000_000_000_000_000_000_000u128;
    state.holdings.insert(token_a(), token_amount_24d(ten_near));
    state.cost_basis.insert(token_a(), yocto(ten_near));

    // Try to sell 50 NEAR worth (more than holdings) for 50 NEAR proceeds
    state.execute_simulated_swap(&token_a(), NEAR_50, &wnear, NEAR_50);

    // actual_deduct = min(50, 10) = 10, to_amount scaled to 10/50 * 50 = 10
    assert!(!state.holdings.contains_key(&token_a()));
    assert!(!state.cost_basis.contains_key(&token_a()));
    assert_eq!(
        state.cash_balance,
        yocto(ten_near),
        "to_amount should be proportionally scaled: 50 * 10/50 = 10 NEAR"
    );
}

#[test]
fn swap_wnear_more_than_cash_scales_output() {
    // Cash = 10 NEAR, try to spend 50 NEAR
    let ten_near = 10_000_000_000_000_000_000_000_000u128;
    let mut state = PortfolioState::new(yocto(ten_near));

    // Try to buy TOKEN_A with 50 NEAR (more than cash balance)
    // If 50 NEAR → 500 TOKEN_A, then 10 NEAR → 100 TOKEN_A
    let token_a_amount = 500_000_000_000_000_000_000_000_000u128;
    state.execute_simulated_swap(&wnear(), NEAR_50, &token_a(), token_a_amount);

    assert_eq!(state.cash_balance, YoctoValue::zero(), "all cash spent");
    // Scaled: 500 * 10/50 = 100
    let expected_token_a = 100_000_000_000_000_000_000_000_000u128;
    assert_eq!(
        state.holdings[&token_a()].smallest_units(),
        &BigDecimal::from(expected_token_a),
        "to_amount should be proportionally scaled"
    );
    assert_eq!(
        state.cost_basis[&token_a()],
        yocto(ten_near),
        "cost basis should reflect actual amount spent"
    );
}

#[test]
fn swap_sell_with_loss() {
    let mut state = PortfolioState::new(YoctoValue::zero());
    let wnear = wnear();

    // Hold TOKEN_A bought at 100 NEAR
    state.holdings.insert(token_a(), token_amount_24d(NEAR_100));
    state.cost_basis.insert(token_a(), yocto(NEAR_100));

    // Sell all for only 80 NEAR (loss of 20 NEAR)
    let eighty_near = 80_000_000_000_000_000_000_000_000u128;
    state.execute_simulated_swap(&token_a(), NEAR_100, &wnear, eighty_near);

    assert!(!state.holdings.contains_key(&token_a()));
    assert_eq!(state.cash_balance, yocto(eighty_near));

    // Realized P&L: 80 - 100 = -20 NEAR in yocto
    let expected_pnl = eighty_near as i128 - NEAR_100 as i128;
    assert!(expected_pnl < 0);
    assert_eq!(state.realized_pnl, expected_pnl);
    assert_eq!(state.realized_pnl_by_token[&token_a()], expected_pnl);
}

// ---------------------------------------------------------------------------
// Value calculation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn total_value_near_cash_only() {
    let state = PortfolioState::new(yocto(NEAR_100));
    let provider = MockRateProvider::new();

    let value = state
        .calculate_total_value_near(sim_day(), &provider)
        .await
        .unwrap();
    assert!((value - 100.0).abs() < 1e-6, "expected 100.0, got {value}");
}

#[tokio::test]
async fn total_value_near_with_holdings() {
    let mut state = PortfolioState::new(yocto(NEAR_50)); // 50 NEAR cash
    state.holdings.insert(token_a(), token_amount_24d(NEAR_50));
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
    let mut state = PortfolioState::new(yocto(NEAR_50));
    state.holdings.insert(token_a(), token_amount_24d(NEAR_50));
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
    let mut state = PortfolioState::new(yocto(NEAR_100));
    let provider = MockRateProvider::new();

    state.record_snapshot(sim_day(), &provider).await.unwrap();

    assert_eq!(state.snapshots.len(), 1);
    assert_eq!(state.snapshots[0].cash_balance, yocto(NEAR_100));
    assert!(state.snapshots[0].holdings.is_empty());
}

#[tokio::test]
async fn record_snapshot_captures_correct_value() {
    let mut state = PortfolioState::new(yocto(NEAR_50));
    state.holdings.insert(token_a(), token_amount_24d(NEAR_50));
    let provider = provider_with_a();

    state.record_snapshot(sim_day(), &provider).await.unwrap();

    assert_eq!(state.snapshots.len(), 1);
    assert!(
        (state.snapshots[0].total_value_near - 100.0).abs() < 1e-6,
        "expected ~100.0, got {}",
        state.snapshots[0].total_value_near
    );
    assert_eq!(
        state.snapshots[0].holdings[&token_a()].smallest_units(),
        &BigDecimal::from(NEAR_50)
    );
}

// ---------------------------------------------------------------------------
// Liquidation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn liquidate_all_sells_everything() {
    let mut state = PortfolioState::new(YoctoValue::zero());
    state.holdings.insert(token_a(), token_amount_24d(NEAR_50));
    state.holdings.insert(token_b(), token_amount_6d(1_000_000));
    state.cost_basis.insert(token_a(), yocto(NEAR_50)); // cost = 50 NEAR
    state
        .cost_basis
        .insert(token_b(), yocto(1_000_000_000_000_000_000_000_000)); // cost = 1 NEAR

    let provider = provider_with_ab();
    state.liquidate_all(sim_day(), &provider).await.unwrap();

    assert!(
        state.holdings.is_empty() || state.holdings.values().all(|v| v.is_zero()),
        "all holdings should be sold"
    );
    assert!(
        state.cash_balance > YoctoValue::zero(),
        "cash should increase"
    );
}

#[tokio::test]
async fn liquidate_all_records_liquidation_trades() {
    let mut state = PortfolioState::new(YoctoValue::zero());
    state.holdings.insert(token_a(), token_amount_24d(NEAR_50));
    state.cost_basis.insert(token_a(), yocto(NEAR_50));
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
    let mut state = PortfolioState::new(yocto(NEAR_100));
    let provider = provider_with_a();

    state.liquidate_all(sim_day(), &provider).await.unwrap();

    assert!(state.trades.is_empty(), "no trades for empty portfolio");
    assert_eq!(state.cash_balance, yocto(NEAR_100), "cash unchanged");
}

#[tokio::test]
async fn liquidate_all_computes_pnl() {
    let mut state = PortfolioState::new(YoctoValue::zero());
    state.holdings.insert(token_a(), token_amount_24d(NEAR_50));
    // Cost basis = 50 NEAR (bought at 1:1)
    state.cost_basis.insert(token_a(), yocto(NEAR_50));

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

fn int_token_a() -> TokenAccount {
    INT_TOKEN_A.parse().unwrap()
}

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
    let cfg = common::config::ConfigResolver;
    TokenRate::batch_insert(&[token_rate], &cfg).await?;
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

    let mut state = PortfolioState::new(yocto(NEAR_50));
    state.holdings.insert(
        int_token_a(),
        TokenAmount::from_smallest_units(BigDecimal::from(1_000_000), INT_TOKEN_A_DECIMALS),
    ); // 1 token = 1 NEAR

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

    let mut state = PortfolioState::new(yocto(NEAR_50));
    state.holdings.insert(
        int_token_a(),
        TokenAmount::from_smallest_units(BigDecimal::from(1_000_000), INT_TOKEN_A_DECIMALS),
    );

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
    assert_eq!(
        state.snapshots[0].holdings[&int_token_a()].smallest_units(),
        &BigDecimal::from(1_000_000)
    );
    assert_eq!(state.snapshots[0].cash_balance, yocto(NEAR_50));

    cleanup_token_rates().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// scale_output unit tests
// ---------------------------------------------------------------------------

#[test]
fn scale_output_no_scaling_when_equal() {
    // actual == requested → return to_amount unchanged
    assert_eq!(PortfolioState::scale_output(1000, 500, 500), 1000);
}

#[test]
fn scale_output_half_input() {
    // actual = half of requested → output halved
    assert_eq!(PortfolioState::scale_output(1000, 50, 100), 500);
}

#[test]
fn scale_output_one_third() {
    // actual = 1/3 of requested → output = 1000/3 = 333 (floor)
    assert_eq!(PortfolioState::scale_output(1000, 1, 3), 333);
}

#[test]
fn scale_output_actual_one() {
    // minimal actual input
    assert_eq!(PortfolioState::scale_output(1_000_000, 1, 1_000_000), 1);
}

#[test]
fn scale_output_to_amount_zero() {
    // zero output stays zero regardless of scaling
    assert_eq!(PortfolioState::scale_output(0, 50, 100), 0);
}

#[test]
fn scale_output_large_values() {
    // large values near u128 range
    let large = 10u128.pow(30);
    let result = PortfolioState::scale_output(large, large / 2, large);
    assert_eq!(result, large / 2);
}

// ---------------------------------------------------------------------------
// to_u128_or_warn
// ---------------------------------------------------------------------------

#[test]
fn to_u128_or_warn_normal_value() {
    let v = BigDecimal::from(42u64);
    assert_eq!(to_u128_or_warn(&v, "test"), 42);
}

#[test]
fn to_u128_or_warn_zero() {
    let v = BigDecimal::from(0u64);
    assert_eq!(to_u128_or_warn(&v, "test"), 0);
}

#[test]
fn to_u128_or_warn_negative_returns_zero() {
    let v = BigDecimal::from(-100i64);
    assert_eq!(
        to_u128_or_warn(&v, "test"),
        0,
        "negative BigDecimal should return 0"
    );
}

#[test]
fn to_u128_or_warn_fractional_returns_zero() {
    use std::str::FromStr;
    let v = BigDecimal::from_str("0.5").unwrap();
    assert_eq!(
        to_u128_or_warn(&v, "test"),
        0,
        "fractional BigDecimal (0.5) should return 0 since to_u128 truncates"
    );
}

#[test]
fn to_u128_or_warn_large_integer() {
    // u128::MAX = 340282366920938463463374607431768211455
    let v = BigDecimal::from(u128::MAX);
    assert_eq!(to_u128_or_warn(&v, "test"), u128::MAX);
}

#[test]
fn to_u128_or_warn_exceeds_u128_returns_zero() {
    let v = BigDecimal::from(u128::MAX) + BigDecimal::from(1u64);
    assert_eq!(
        to_u128_or_warn(&v, "test"),
        0,
        "value exceeding u128::MAX should return 0"
    );
}

// ---------------------------------------------------------------------------
// execute_simulated_swap: actual_to == 0 guard
// ---------------------------------------------------------------------------

#[test]
fn swap_skipped_when_scaled_output_is_zero() {
    // scale_output(1, 1, very_large) → 0 → swap should be skipped entirely
    let mut state = PortfolioState::new(yocto(NEAR_100));
    let wnear = wnear();

    // to_amount = 1, from_amount = NEAR_100, actual_from = 1 (min(NEAR_100, NEAR_100))
    // But we want actual_to = 0: set to_amount = 0
    state.execute_simulated_swap(&wnear, NEAR_50, &token_a(), 0);

    // Nothing should change: to_amount = 0 → amount_out check in mock_client prevents this,
    // but execute_simulated_swap should also guard
    assert_eq!(state.cash_balance, yocto(NEAR_100));
    assert!(state.holdings.is_empty());
}
