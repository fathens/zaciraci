use super::types::*;
use bigdecimal::{BigDecimal, FromPrimitive};
use chrono::{NaiveDate, TimeZone, Utc};
use common::algorithm::portfolio::{
    PortfolioData, execute_portfolio_optimization, needs_rebalancing,
};
use common::algorithm::{PriceHistory, PricePoint, TokenData, WalletInfo};
use common::types::{ExchangeRate, NearValue, TokenAmount, TokenPrice};
use std::collections::{BTreeMap, HashMap};

fn price(v: f64) -> TokenPrice {
    TokenPrice::from_near_per_token(BigDecimal::from_f64(v).unwrap())
}

/// ExchangeRate を price (NEAR/token) から作成するヘルパー
fn rate_from_price(near_per_token: f64) -> ExchangeRate {
    ExchangeRate::from_price(&price(near_per_token), 18)
}

fn cap(v: i64) -> NearValue {
    NearValue::from_near(BigDecimal::from(v))
}

// テスト用のポートフォリオデータを作成
fn create_test_portfolio_data() -> PortfolioData {
    // current_price = 0.001 NEAR/token
    // 10% リターン → predicted_price = 0.001 * 1.1 = 0.0011
    let mut predictions = HashMap::new();
    predictions.insert("test.token".to_string(), price(0.001 * 1.1)); // 10%のリターン予測

    let tokens = vec![TokenData {
        symbol: "test.token".to_string(),
        current_rate: rate_from_price(0.001), // 0.001 NEAR/token
        historical_volatility: 0.2,
        liquidity_score: Some(1.0),
        market_cap: Some(cap(1000000)),
    }];

    // 価格履歴を作成（7日分）
    let mut historical_prices = Vec::new();
    for i in 0..7 {
        let date = NaiveDate::from_ymd_opt(2024, 8, 1 + i)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        historical_prices.push(PriceHistory {
            token: "test.token".to_string(),
            quote_token: "wrap.near".to_string(),
            prices: vec![PricePoint {
                timestamp: Utc.from_utc_datetime(&date),
                price: price(0.001 * (1.0 + i as f64 * 0.01)), // わずかな価格変動
                volume: Some(BigDecimal::from_f64(1000.0).unwrap()),
            }],
        });
    }

    PortfolioData {
        tokens,
        predictions: predictions.into_iter().collect(),
        historical_prices,
        correlation_matrix: None,
    }
}

fn create_test_wallet_info() -> WalletInfo {
    WalletInfo {
        holdings: BTreeMap::from([(
            "test.token".to_string(),
            TokenAmount::from_smallest_units(BigDecimal::from(500000), 18), // 500000 smallest units
        )]),
        total_value: NearValue::from_near(BigDecimal::from(1000)), // 1000 NEAR
        cash_balance: NearValue::from_near(BigDecimal::from(500)), // 500 NEAR
    }
}

#[test]
fn test_needs_rebalancing_with_different_thresholds() {
    println!("🧪 Testing needs_rebalancing with different thresholds");

    let current_weights = vec![0.5, 0.5]; // 50%, 50%
    let target_weights = vec![0.6, 0.4]; // 60%, 40% (10%の変化)

    // 5%閾値 - リバランスが必要
    let threshold_5_percent = 0.05;
    let needs_rebalancing_5 =
        needs_rebalancing(&current_weights, &target_weights, threshold_5_percent);
    println!(
        "  - 5% threshold: needs_rebalancing = {}",
        needs_rebalancing_5
    );
    assert!(
        needs_rebalancing_5,
        "10% weight change should trigger rebalancing with 5% threshold"
    );

    // 15%閾値 - リバランス不要
    let threshold_15_percent = 0.15;
    let needs_rebalancing_15 =
        needs_rebalancing(&current_weights, &target_weights, threshold_15_percent);
    println!(
        "  - 15% threshold: needs_rebalancing = {}",
        needs_rebalancing_15
    );
    assert!(
        !needs_rebalancing_15,
        "10% weight change should NOT trigger rebalancing with 15% threshold"
    );

    // 10%閾値（境界ケース） - リバランス不要
    let threshold_10_percent = 0.10;
    let needs_rebalancing_10 =
        needs_rebalancing(&current_weights, &target_weights, threshold_10_percent);
    println!(
        "  - 10% threshold: needs_rebalancing = {}",
        needs_rebalancing_10
    );
    assert!(
        !needs_rebalancing_10,
        "10% weight change should NOT trigger rebalancing with exactly 10% threshold"
    );

    println!("✅ needs_rebalancing threshold test passed");
}

#[tokio::test]
async fn test_portfolio_optimization_with_custom_threshold() {
    println!("🧪 Testing portfolio optimization with custom rebalance threshold");

    let portfolio_data = create_test_portfolio_data();
    let wallet_info = create_test_wallet_info();

    // 異なる閾値でポートフォリオ最適化を実行
    let threshold_low = 0.01; // 1% - 敏感
    let threshold_high = 0.2; // 20% - 鈍感

    println!("  Testing with low threshold ({})", threshold_low);
    if let Ok(report_low) =
        execute_portfolio_optimization(&wallet_info, portfolio_data.clone(), threshold_low).await
    {
        println!("    - Rebalance needed: {}", report_low.rebalance_needed);
        println!("    - Actions count: {}", report_low.actions.len());
    }

    println!("  Testing with high threshold ({})", threshold_high);
    if let Ok(report_high) =
        execute_portfolio_optimization(&wallet_info, portfolio_data.clone(), threshold_high).await
    {
        println!("    - Rebalance needed: {}", report_high.rebalance_needed);
        println!("    - Actions count: {}", report_high.actions.len());
    }

    println!("✅ Portfolio optimization with custom threshold test completed");
}

#[test]
fn test_simulate_args_portfolio_rebalance_threshold() {
    println!("🧪 Testing SimulateArgs portfolio_rebalance_threshold field");

    // デフォルト値のテスト
    let args_default = SimulateArgs {
        start: Some("2024-01-01".to_string()),
        end: Some("2024-01-10".to_string()),
        capital: 1000.0,
        quote_token: "wrap.near".to_string(),
        output: "simulation_results".to_string(),
        rebalance_interval: "1d".to_string(),
        fee_model: "realistic".to_string(),
        custom_fee: None,
        slippage: 0.01,
        gas_cost: 0.01,
        min_trade: 1.0,
        prediction_horizon: 24,
        historical_days: 30,
        chart: false,
        verbose: false,
        model: None,
        portfolio_rebalance_threshold: 0.05, // デフォルト値
        portfolio_rebalance_interval: "1d".to_string(),
        momentum_min_profit_threshold: 0.01,
        momentum_switch_multiplier: 1.2,
        momentum_min_trade_amount: 0.1,
        trend_rsi_overbought: 80.0,
        trend_rsi_oversold: 20.0,
        trend_adx_strong_threshold: 20.0,
        trend_r_squared_threshold: 0.5,
    };

    assert_eq!(args_default.portfolio_rebalance_threshold, 0.05);
    println!(
        "  - Default threshold: {}",
        args_default.portfolio_rebalance_threshold
    );

    // カスタム値のテスト
    let args_custom = SimulateArgs {
        portfolio_rebalance_threshold: 0.1, // カスタム値
        ..args_default
    };

    assert_eq!(args_custom.portfolio_rebalance_threshold, 0.1);
    println!(
        "  - Custom threshold: {}",
        args_custom.portfolio_rebalance_threshold
    );

    println!("✅ SimulateArgs portfolio_rebalance_threshold test passed");
}

#[test]
fn test_simulation_config_portfolio_rebalance_threshold() {
    println!("🧪 Testing SimulationConfig portfolio_rebalance_threshold field");

    let config = SimulationConfig {
        start_date: Utc.from_utc_datetime(
            &NaiveDate::from_ymd_opt(2024, 8, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
        ),
        end_date: Utc.from_utc_datetime(
            &NaiveDate::from_ymd_opt(2024, 8, 2)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
        ),
        algorithm: AlgorithmType::Portfolio,
        initial_capital: BigDecimal::from(1000),
        target_tokens: vec!["test.token".to_string()],
        quote_token: "wrap.near".to_string(),
        rebalance_interval: RebalanceInterval::parse("1d").unwrap(),
        fee_model: FeeModel::Custom(0.003),
        slippage_rate: 0.01,
        historical_days: 7,
        prediction_horizon: chrono::Duration::hours(24),
        model: Some("mock".to_string()),
        verbose: false,
        gas_cost: BigDecimal::from(0),
        min_trade_amount: BigDecimal::from(1),
        portfolio_rebalance_threshold: 0.08, // カスタム値
        portfolio_rebalance_interval: RebalanceInterval::parse("1d").unwrap(),
        momentum_min_profit_threshold: 0.01,
        momentum_switch_multiplier: 1.2,
        momentum_min_trade_amount: 0.1,
        trend_rsi_overbought: 80.0,
        trend_rsi_oversold: 20.0,
        trend_adx_strong_threshold: 20.0,
        trend_r_squared_threshold: 0.5,
    };

    assert_eq!(config.portfolio_rebalance_threshold, 0.08);
    println!(
        "  - Config threshold: {}",
        config.portfolio_rebalance_threshold
    );

    println!("✅ SimulationConfig portfolio_rebalance_threshold test passed");
}

#[test]
fn test_threshold_boundary_conditions() {
    println!("🧪 Testing rebalance threshold boundary conditions");

    let current_weights = vec![0.5];

    // 閾値よりわずかに小さい変化 (4.9% < 5%)
    let target_weights_small = vec![0.549];
    let needs_rebalancing_small = needs_rebalancing(&current_weights, &target_weights_small, 0.05);
    println!(
        "  - 4.9% change with 5% threshold: {}",
        needs_rebalancing_small
    );
    assert!(
        !needs_rebalancing_small,
        "4.9% change should NOT trigger rebalancing with 5% threshold"
    );

    // 閾値よりわずかに大きい変化 (5.1% > 5%)
    let target_weights_large = vec![0.551];
    let needs_rebalancing_large = needs_rebalancing(&current_weights, &target_weights_large, 0.05);
    println!(
        "  - 5.1% change with 5% threshold: {}",
        needs_rebalancing_large
    );
    assert!(
        needs_rebalancing_large,
        "5.1% change should trigger rebalancing with 5% threshold"
    );

    // 正確に閾値と同じ変化 (5% = 5%)
    let target_weights_exact = vec![0.55];
    let needs_rebalancing_exact = needs_rebalancing(&current_weights, &target_weights_exact, 0.05);
    println!(
        "  - Exact 5% change with 5% threshold: {}",
        needs_rebalancing_exact
    );
    assert!(
        needs_rebalancing_exact,
        "Exact 5% change should trigger rebalancing (using >= not just >)"
    );

    println!("✅ Threshold boundary conditions test passed");
}
