use super::*;

#[test]
fn new_sets_initial_capital() {
    let state = PortfolioState::new(100_000_000_000_000_000_000_000_000); // 100 NEAR
    assert_eq!(state.cash_balance, 100_000_000_000_000_000_000_000_000);
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
