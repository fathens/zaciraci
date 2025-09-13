use super::algorithms::{run_momentum_timestep_simulation, run_portfolio_optimization_simulation};
use super::types::*;
use bigdecimal::BigDecimal;
use chrono::{NaiveDate, TimeZone, Utc};
use common::stats::ValueAtTime;
use std::collections::HashMap;

// yoctoNEAR単位のテストデータ
fn create_test_price_data() -> HashMap<String, Vec<ValueAtTime>> {
    let mut price_data = HashMap::new();

    // 1 NEAR = 1e24 yoctoNEAR として価格データを作成
    // 現在価格: 0.001 NEAR per token = 1e21 yoctoNEAR per token
    let current_price_yocto = 1e21; // 0.001 NEAR in yoctoNEAR

    let test_data = vec![
        ValueAtTime {
            time: NaiveDate::from_ymd_opt(2024, 8, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            value: current_price_yocto,
        },
        ValueAtTime {
            time: NaiveDate::from_ymd_opt(2024, 8, 2)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            value: current_price_yocto * 1.1, // 10%価格上昇
        },
    ];

    price_data.insert("test.token".to_string(), test_data);
    price_data
}

fn create_test_config() -> SimulationConfig {
    SimulationConfig {
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
        initial_capital: BigDecimal::from(1000), // 1000 NEAR
        target_tokens: vec!["test.token".to_string()],
        quote_token: "wrap.near".to_string(),
        rebalance_interval: RebalanceInterval::parse("1d").unwrap(),
        fee_model: FeeModel::Custom(0.003), // 0.3%
        slippage_rate: 0.01,                // 1%
        historical_days: 7,
        prediction_horizon: chrono::Duration::hours(24),
        model: Some("mock".to_string()),
        verbose: false,
        gas_cost: BigDecimal::from(0),
        min_trade_amount: BigDecimal::from(1),
        portfolio_rebalance_threshold: 0.05,
    }
}

#[tokio::test]
async fn test_portfolio_calculation_units() {
    let config = create_test_config();
    let price_data = create_test_price_data();

    // Portfolio最適化を実行
    let result = run_portfolio_optimization_simulation(&config, &price_data).await;

    assert!(result.is_ok(), "Portfolio simulation should succeed");
    let result = result.unwrap();

    // ポートフォリオ価値が現実的な範囲内にあることを確認
    // 1000 NEAR の初期資本で始まり、異常に大きな値（兆単位）になっていないことを確認
    assert!(
        result.config.initial_capital < 2000.0,
        "Initial capital should be reasonable: {}",
        result.config.initial_capital
    );

    assert!(
        result.config.final_value < 10000.0,
        "Final value should be reasonable: {} (not in trillions)",
        result.config.final_value
    );

    // ポートフォリオ価値の履歴もチェック
    for portfolio_value in &result.portfolio_values {
        assert!(
            portfolio_value.total_value < 10000.0,
            "Portfolio value should be reasonable: {} at {}",
            portfolio_value.total_value,
            portfolio_value.timestamp
        );
        assert!(
            portfolio_value.total_value > 0.0,
            "Portfolio value should be positive: {}",
            portfolio_value.total_value
        );
    }

    println!("✅ Portfolio calculation units test passed");
    println!(
        "  - Initial capital: {:.2} NEAR",
        result.config.initial_capital
    );
    println!("  - Final value: {:.2} NEAR", result.config.final_value);
    println!(
        "  - Portfolio values count: {}",
        result.portfolio_values.len()
    );
}

#[tokio::test]
async fn test_momentum_calculation_units() {
    let config = create_test_config();
    let price_data = create_test_price_data();

    // Momentum戦略を実行
    let result = run_momentum_timestep_simulation(&config, &price_data).await;

    assert!(result.is_ok(), "Momentum simulation should succeed");
    let result = result.unwrap();

    // ポートフォリオ価値が現実的な範囲内にあることを確認
    assert!(
        result.config.initial_capital < 2000.0,
        "Initial capital should be reasonable: {}",
        result.config.initial_capital
    );

    assert!(
        result.config.final_value < 10000.0,
        "Final value should be reasonable: {} (not in trillions)",
        result.config.final_value
    );

    // ポートフォリオ価値の履歴もチェック
    for portfolio_value in &result.portfolio_values {
        assert!(
            portfolio_value.total_value < 10000.0,
            "Portfolio value should be reasonable: {} at {}",
            portfolio_value.total_value,
            portfolio_value.timestamp
        );
        assert!(
            portfolio_value.total_value > 0.0,
            "Portfolio value should be positive: {}",
            portfolio_value.total_value
        );
    }

    println!("✅ Momentum calculation units test passed");
    println!(
        "  - Initial capital: {:.2} NEAR",
        result.config.initial_capital
    );
    println!("  - Final value: {:.2} NEAR", result.config.final_value);
    println!(
        "  - Portfolio values count: {}",
        result.portfolio_values.len()
    );
}

#[test]
fn test_units_utility_functions() {
    // 単位変換ユーティリティ関数のテスト

    // 1 NEAR = 1e24 yoctoNEAR
    let one_near_yocto = 1e24;
    let one_near = common::units::Units::yocto_f64_to_near_f64(one_near_yocto);
    assert!(
        (one_near - 1.0).abs() < 1e-10,
        "1e24 yoctoNEAR should be 1 NEAR"
    );

    // 0.001 NEAR = 1e21 yoctoNEAR
    let small_amount_yocto = 1e21;
    let small_amount_near = common::units::Units::yocto_f64_to_near_f64(small_amount_yocto);
    assert!(
        (small_amount_near - 0.001).abs() < 1e-10,
        "1e21 yoctoNEAR should be 0.001 NEAR"
    );

    // 逆変換のテスト
    let converted_back = common::units::Units::near_f64_to_yocto_f64(small_amount_near);
    assert!(
        (converted_back - small_amount_yocto).abs() < 1e10,
        "Round-trip conversion should work"
    );

    println!("✅ Units utility functions test passed");
    println!("  - 1e24 yoctoNEAR = {:.6} NEAR", one_near);
    println!("  - 1e21 yoctoNEAR = {:.6} NEAR", small_amount_near);
    println!("  - Round-trip: {:.0} yoctoNEAR", converted_back);
}

#[test]
fn test_token_amount_calculation() {
    // トークン数量計算のテスト
    let initial_capital_near = 1000.0; // 1000 NEAR
    let price_per_token_yocto = 1e21; // 0.001 NEAR per token in yoctoNEAR

    // 修正前: 間違った計算
    let wrong_amount = initial_capital_near / price_per_token_yocto;

    // 修正後: 正しい計算
    let price_per_token_near = common::units::Units::yocto_f64_to_near_f64(price_per_token_yocto);
    let correct_amount = initial_capital_near / price_per_token_near;

    println!("💡 Token amount calculation comparison:");
    println!("  - Initial capital: {:.2} NEAR", initial_capital_near);
    println!(
        "  - Price per token: {:.6} NEAR ({:.0} yoctoNEAR)",
        price_per_token_near, price_per_token_yocto
    );
    println!("  - Wrong calculation: {:.2} tokens", wrong_amount);
    println!("  - Correct calculation: {:.2} tokens", correct_amount);

    assert!(
        wrong_amount < 1e-12,
        "Wrong calculation should give tiny amount: {}",
        wrong_amount
    );
    assert!(
        correct_amount > 1e5,
        "Correct calculation should give reasonable amount: {}",
        correct_amount
    );

    // 期待される値: 1000 NEAR / 0.001 NEAR per token = 1,000,000 tokens
    assert!(
        (correct_amount - 1_000_000.0).abs() < 1.0,
        "Should get ~1M tokens for 1000 NEAR at 0.001 NEAR per token"
    );
}
