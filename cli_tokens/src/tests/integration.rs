//! コマンド間の連携や統合的な機能のテスト
//! - ファイル形式の互換性
//! - アルゴリズム統合
//! - API呼び出し

use bigdecimal::BigDecimal;
use bigdecimal::ToPrimitive;
use chrono::Utc;
use common::algorithm::calculate_volatility_score;
use common::stats::ValueAtTime;

use crate::commands::simulate::{FeeModel, calculate_trading_cost};
use crate::commands::top::parse_date;
use crate::utils::file::sanitize_filename;

#[test]
fn test_parse_date_function() {
    // Test valid date parsing
    let result = parse_date("2023-01-15");
    assert!(result.is_ok());

    let date = result.unwrap();
    assert_eq!(date.format("%Y-%m-%d").to_string(), "2023-01-15");

    // Test invalid date parsing
    let invalid_result = parse_date("invalid-date");
    assert!(invalid_result.is_err());
}

#[test]
fn test_date_calculations() {
    // Test date calculations used in top command
    let base_date = parse_date("2023-01-15").unwrap();
    let thirty_days_ago = base_date - chrono::Duration::days(30);

    assert_eq!(thirty_days_ago.format("%Y-%m-%d").to_string(), "2022-12-16");
}

#[test]
fn test_sanitize_filename() {
    // Test filename sanitization
    assert_eq!(sanitize_filename("wrap.near"), "wrap.near");
    assert_eq!(
        sanitize_filename("token/with/slashes"),
        "token_with_slashes"
    );
    assert_eq!(sanitize_filename("token:with:colons"), "token_with_colons");
    assert_eq!(sanitize_filename("token with spaces"), "token_with_spaces");
}

#[test]
fn test_simulate_args_integration() {
    use crate::commands::simulate::SimulateArgs;

    // Test integration of SimulateArgs with various configurations
    let momentum_config = SimulateArgs {
        start: Some("2024-08-01".to_string()),
        end: Some("2024-08-10".to_string()),
        capital: 10000.0,
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
        portfolio_rebalance_threshold: 0.05,
        portfolio_rebalance_interval: "1d".to_string(),
        momentum_min_profit_threshold: 0.01,
        momentum_switch_multiplier: 1.2,
        momentum_min_trade_amount: 0.1,
        trend_rsi_overbought: 80.0,
        trend_rsi_oversold: 20.0,
        trend_adx_strong_threshold: 20.0,
        trend_r_squared_threshold: 0.5,
    };

    // Test that configuration is parsed correctly
    assert_eq!(momentum_config.capital, 10000.0);
    assert_eq!(momentum_config.quote_token, "wrap.near");
    assert_eq!(momentum_config.fee_model, "realistic");
    assert_eq!(momentum_config.slippage, 0.01);

    // Test portfolio algorithm config
    let portfolio_config = SimulateArgs {
        start: Some("2024-08-01".to_string()),
        end: Some("2024-08-10".to_string()),
        capital: 5000.0,
        quote_token: "wrap.near".to_string(),
        output: "portfolio_results".to_string(),
        rebalance_interval: "1h".to_string(),
        fee_model: "zero".to_string(),
        custom_fee: Some(0.002),
        slippage: 0.005,
        gas_cost: 0.005,
        min_trade: 0.5,
        prediction_horizon: 12,
        historical_days: 14,
        chart: true,
        verbose: true,
        model: None,
        portfolio_rebalance_threshold: 0.05,
        portfolio_rebalance_interval: "1d".to_string(),
        momentum_min_profit_threshold: 0.01,
        momentum_switch_multiplier: 1.2,
        momentum_min_trade_amount: 0.1,
        trend_rsi_overbought: 80.0,
        trend_rsi_oversold: 20.0,
        trend_adx_strong_threshold: 20.0,
        trend_r_squared_threshold: 0.5,
    };

    assert_eq!(portfolio_config.capital, 5000.0);
    assert_eq!(portfolio_config.quote_token, "wrap.near");
    assert_eq!(portfolio_config.fee_model, "zero");
    assert_eq!(portfolio_config.custom_fee.unwrap(), 0.002);
    assert!(portfolio_config.verbose);
}

#[test]
fn test_fee_model_integration() {
    let trade_amount = BigDecimal::from(1000);
    let slippage = "0.01".parse::<BigDecimal>().unwrap();
    let gas_cost = "0.01".parse::<BigDecimal>().unwrap();

    // Test zero fee model (still includes slippage and gas costs)
    let zero_cost = calculate_trading_cost(
        trade_amount.to_f64().unwrap(),
        &FeeModel::Zero,
        slippage.to_f64().unwrap(),
        gas_cost.to_f64().unwrap(),
    );
    let expected_zero_cost = &trade_amount * &slippage + &gas_cost; // Only slippage + gas, no protocol fee
    assert!((zero_cost - expected_zero_cost.to_f64().unwrap()).abs() < 0.01);

    // Test realistic fee model
    let realistic_cost = calculate_trading_cost(
        trade_amount.to_f64().unwrap(),
        &FeeModel::Realistic,
        slippage.to_f64().unwrap(),
        gas_cost.to_f64().unwrap(),
    );
    // Should include pool fee (0.3%) + slippage + gas
    let expected_cost = &trade_amount * "0.003".parse::<BigDecimal>().unwrap()
        + &trade_amount * &slippage
        + &gas_cost;
    assert!((realistic_cost - expected_cost.to_f64().unwrap()).abs() < 0.01);

    // Test custom fee model
    let custom_fee = "0.005".parse::<BigDecimal>().unwrap(); // 0.5%
    let custom_cost = calculate_trading_cost(
        trade_amount.to_f64().unwrap(),
        &FeeModel::Custom(custom_fee.to_f64().unwrap()),
        slippage.to_f64().unwrap(),
        gas_cost.to_f64().unwrap(),
    );
    let expected_custom_cost = &trade_amount * &custom_fee + &trade_amount * &slippage + &gas_cost;
    assert!((custom_cost - expected_custom_cost.to_f64().unwrap()).abs() < 0.01);
}

#[test]
fn test_volatility_token_filtering() {
    // Test that high volatility tokens are correctly scored
    let high_volatility_data = vec![
        ValueAtTime {
            time: Utc::now().naive_utc(),
            value: BigDecimal::from(100),
        },
        ValueAtTime {
            time: Utc::now().naive_utc(),
            value: BigDecimal::from(150),
        }, // +50%
        ValueAtTime {
            time: Utc::now().naive_utc(),
            value: BigDecimal::from(75),
        }, // -50%
        ValueAtTime {
            time: Utc::now().naive_utc(),
            value: BigDecimal::from(125),
        }, // +67%
    ];

    let high_score = calculate_volatility_score(&high_volatility_data, true);

    // Test that low volatility tokens get lower scores
    let low_volatility_data = vec![
        ValueAtTime {
            time: Utc::now().naive_utc(),
            value: BigDecimal::from(100),
        },
        ValueAtTime {
            time: Utc::now().naive_utc(),
            value: BigDecimal::from(101),
        }, // +1%
        ValueAtTime {
            time: Utc::now().naive_utc(),
            value: BigDecimal::from(102),
        }, // +1%
        ValueAtTime {
            time: Utc::now().naive_utc(),
            value: BigDecimal::from(103),
        }, // +1%
    ];

    let low_score = calculate_volatility_score(&low_volatility_data, true);

    // High volatility should score higher than low volatility
    assert!(high_score > low_score);
    assert!(high_score <= 1.0);
    assert!(low_score >= 0.0);
}
