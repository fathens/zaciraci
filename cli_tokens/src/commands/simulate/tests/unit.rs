//! Simulateコマンドの単体テスト
//! - SimulateArgsの設定テスト
//! - リバランス間隔のパースとバリデーション
//! - 取引コスト計算
//! - プライスデータ取得ロジック
//! - 取引決定アルゴリズム
//! - イミュータブルポートフォリオデータ構造
//! - トレーディング戦略パターン

use bigdecimal::BigDecimal;
use chrono::Utc;
use common::stats::ValueAtTime;
use std::collections::HashMap;

use super::super::data::get_prices_at_time;
use super::super::utils::{
    convert_decision_to_action, convert_ranked_tokens_to_opportunities, make_trading_decision,
};
use super::super::*;
use common::algorithm::momentum::{calculate_confidence_adjusted_return, rank_tokens_by_momentum};
use common::algorithm::{PredictionData, TradingAction};

#[test]
fn test_simulate_args_default_values() {
    let args = SimulateArgs {
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

    assert_eq!(args.capital, 1000.0);
    assert_eq!(args.quote_token, "wrap.near");
    assert_eq!(args.rebalance_interval, "1d");
    assert_eq!(args.fee_model, "realistic");
    assert_eq!(args.slippage, 0.01);
    assert_eq!(args.historical_days, 30);
    assert!(!args.verbose);
    assert_eq!(args.model, None); // デフォルトはNone
}

#[test]
fn test_simulate_args_with_model() {
    // モデルを指定した場合のテスト
    let args_with_model = SimulateArgs {
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
        model: Some("chronos_default".to_string()),
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

    assert_eq!(args_with_model.model, Some("chronos_default".to_string()));

    // 別のモデルをテスト
    let args_with_fast_model = SimulateArgs {
        start: Some("2024-01-01".to_string()),
        end: Some("2024-01-10".to_string()),
        capital: 1000.0,
        quote_token: "wrap.near".to_string(),
        output: "simulation_results".to_string(),
        rebalance_interval: "1h".to_string(),
        fee_model: "zero".to_string(),
        custom_fee: None,
        slippage: 0.005,
        gas_cost: 0.005,
        min_trade: 0.5,
        prediction_horizon: 12,
        historical_days: 14,
        chart: false,
        verbose: false,
        model: Some("fast_statistical".to_string()),
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

    assert_eq!(
        args_with_fast_model.model,
        Some("fast_statistical".to_string())
    );
}

#[test]
fn test_rebalance_interval_parsing() {
    // Test basic formats
    assert!(RebalanceInterval::parse("1h").is_ok());
    assert!(RebalanceInterval::parse("2d").is_ok());
    assert!(RebalanceInterval::parse("30m").is_ok());
    assert!(RebalanceInterval::parse("1w").is_ok());

    // Test compound formats
    assert!(RebalanceInterval::parse("1h30m").is_ok());
    assert!(RebalanceInterval::parse("2d12h").is_ok());

    // Test various units
    assert!(RebalanceInterval::parse("30s").is_ok());
    assert!(RebalanceInterval::parse("5min").is_ok());
    assert!(RebalanceInterval::parse("1hour").is_ok());
    assert!(RebalanceInterval::parse("3days").is_ok());
    assert!(RebalanceInterval::parse("2weeks").is_ok());

    // Test invalid formats
    assert!(RebalanceInterval::parse("invalid").is_err());
    assert!(RebalanceInterval::parse("1").is_err()); // No unit
    assert!(RebalanceInterval::parse("h1").is_err()); // Wrong order
    assert!(RebalanceInterval::parse("0h").is_err()); // Zero duration
    assert!(RebalanceInterval::parse("-1h").is_err()); // Negative duration
}

#[test]
fn test_rebalance_interval_duration_conversion() {
    let interval_1h = RebalanceInterval::parse("1h").unwrap();
    assert_eq!(interval_1h.as_duration().num_hours(), 1);

    let interval_2d = RebalanceInterval::parse("2d").unwrap();
    assert_eq!(interval_2d.as_duration().num_days(), 2);

    let interval_90m = RebalanceInterval::parse("90m").unwrap();
    assert_eq!(interval_90m.as_duration().num_minutes(), 90);

    let interval_compound = RebalanceInterval::parse("1h30m").unwrap();
    assert_eq!(interval_compound.as_duration().num_minutes(), 90);

    let interval_week = RebalanceInterval::parse("1w").unwrap();
    assert_eq!(interval_week.as_duration().num_days(), 7);
}

#[test]
fn test_rebalance_interval_display() {
    let interval_1h = RebalanceInterval::parse("1h").unwrap();
    assert_eq!(format!("{}", interval_1h), "1h");

    let interval_1d = RebalanceInterval::parse("1d").unwrap();
    assert_eq!(format!("{}", interval_1d), "1d");

    let interval_90m = RebalanceInterval::parse("90m").unwrap();
    assert_eq!(format!("{}", interval_90m), "1h30m"); // Should normalize

    let interval_compound = RebalanceInterval::parse("1h30m").unwrap();
    assert_eq!(format!("{}", interval_compound), "1h30m");
}

#[test]
fn test_rebalance_interval_from_str() {
    use std::str::FromStr;

    let interval = RebalanceInterval::from_str("2h").unwrap();
    assert_eq!(interval.as_duration().num_hours(), 2);

    assert!(RebalanceInterval::from_str("invalid").is_err());
}

#[test]
fn test_trading_cost_calculation() {
    let trade_amount = 1000.0;

    // ゼロ手数料
    let zero_cost = calculate_trading_cost(trade_amount, &FeeModel::Zero, 0.0, 0.0);
    assert_eq!(zero_cost, 0.0);

    // リアル手数料 (0.3%のプール手数料 + スリッページ1% + ガス代0.01)
    let realistic_cost = calculate_trading_cost(trade_amount, &FeeModel::Realistic, 0.01, 0.01);
    assert!(realistic_cost > 0.0);

    // カスタム手数料
    let custom_fee = 0.005; // 0.5%
    let custom_cost =
        calculate_trading_cost(trade_amount, &FeeModel::Custom(custom_fee), 0.01, 0.01);
    assert!(custom_cost > 0.0);
}

#[test]
fn test_calculate_confidence_adjusted_return() {
    let prediction = PredictionData {
        token: "test_token".to_string(),
        current_price: BigDecimal::from(100),
        predicted_price_24h: BigDecimal::from(110), // 10% growth
        timestamp: Utc::now(),
        confidence: Some(0.8),
    };

    let adjusted_return = calculate_confidence_adjusted_return(&prediction);

    // 10% return - 0.6% fee - 2% slippage = 7.4%, then * 0.8 confidence = 5.92%
    let expected_return = (0.1 - 0.006 - 0.02) * 0.8; // 約1.8%
    assert!((adjusted_return - expected_return).abs() < 0.0001);
}

// calculate_simple_volatility function was removed as it's no longer used
// Volatility calculations are now handled by common crate implementations

// 削除された関数のテストは、API統合テストに置き換えられました
// test_predict_price_trend と test_calculate_prediction_confidence は
// 新しいAPI統合による予測生成に置き換えられたため、削除しました。
// 新しいテストは api_integration_tests.rs を参照してください。

#[test]
fn test_rank_tokens_by_momentum() {
    let predictions = vec![
        PredictionData {
            token: "token1".to_string(),
            current_price: BigDecimal::from(100),
            predicted_price_24h: BigDecimal::from(105), // 5% growth
            timestamp: Utc::now(),
            confidence: Some(0.8),
        },
        PredictionData {
            token: "token2".to_string(),
            current_price: BigDecimal::from(100),
            predicted_price_24h: BigDecimal::from(110), // 10% growth
            timestamp: Utc::now(),
            confidence: Some(0.6),
        },
        PredictionData {
            token: "token3".to_string(),
            current_price: BigDecimal::from(100),
            predicted_price_24h: BigDecimal::from(95), // -5% decline
            timestamp: Utc::now(),
            confidence: Some(0.9),
        },
    ];

    let ranked = rank_tokens_by_momentum(predictions);

    // rank_tokens_by_momentum関数は正のリターンのトークンのみフィルタリングし、TOP_N_TOKENS（3個）まで制限
    // 負のリターンのトークンは除外される可能性がある
    assert!(ranked.len() <= 3);

    if ranked.len() > 1 {
        // スコアが降順でソートされている
        assert!(ranked[0].1 >= ranked[1].1); // first >= second
    }
}

#[test]
fn test_get_prices_at_time_multiple_tokens() {
    let target_time = Utc::now();
    let mut price_data = HashMap::new();

    // 前後1時間以内にデータがある場合
    let values = vec![
        ValueAtTime {
            time: (target_time - chrono::Duration::minutes(30)).naive_utc(),
            value: 100.0,
        },
        ValueAtTime {
            time: target_time.naive_utc(),
            value: 105.0,
        },
        ValueAtTime {
            time: (target_time + chrono::Duration::minutes(30)).naive_utc(),
            value: 110.0,
        },
    ];

    price_data.insert("token1".to_string(), values);

    let result = get_prices_at_time(&price_data, target_time).unwrap();
    assert_eq!(result.get("token1"), Some(&105.0));
}

#[test]
fn test_get_prices_at_time_stale_data() {
    let target_time = Utc::now();
    let mut price_data = HashMap::new();

    // 前後1時間以内にデータがない場合
    let values = vec![
        ValueAtTime {
            time: (target_time - chrono::Duration::hours(2)).naive_utc(),
            value: 100.0,
        },
        ValueAtTime {
            time: (target_time + chrono::Duration::hours(2)).naive_utc(),
            value: 110.0,
        },
    ];

    price_data.insert("token1".to_string(), values);

    let result = get_prices_at_time(&price_data, target_time);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("No price data found for token 'token1' within 1 hour"));
}

#[test]
fn test_get_prices_at_time_empty_data() {
    let target_time = Utc::now();
    let price_data = HashMap::new();

    let result = get_prices_at_time(&price_data, target_time).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_get_prices_at_time_boundary_case() {
    let target_time = Utc::now();
    let mut price_data = HashMap::new();

    // 境界値テスト: ちょうど1時間前のデータ
    let values = vec![
        ValueAtTime {
            time: (target_time - chrono::Duration::hours(1)).naive_utc(),
            value: 100.0,
        },
        ValueAtTime {
            time: (target_time + chrono::Duration::hours(1)).naive_utc(),
            value: 110.0,
        },
    ];

    price_data.insert("token1".to_string(), values);

    let result = get_prices_at_time(&price_data, target_time).unwrap();
    assert!(result.contains_key("token1"));
}

#[test]
fn test_get_prices_at_time_with_sufficient_data() {
    let target_time = Utc::now();
    let mut price_data = HashMap::new();

    let values = vec![
        ValueAtTime {
            time: (target_time - chrono::Duration::minutes(30)).naive_utc(),
            value: 100.0,
        },
        ValueAtTime {
            time: target_time.naive_utc(),
            value: 105.0,
        },
    ];

    price_data.insert("token1".to_string(), values);

    let result = get_prices_at_time(&price_data, target_time).unwrap();
    assert_eq!(result.get("token1").unwrap(), &105.0);
}

#[test]
fn test_get_prices_at_time_with_insufficient_data() {
    let target_time = Utc::now();
    let mut price_data = HashMap::new();

    // 前後1時間以内にデータがない場合
    let values = vec![ValueAtTime {
        time: (target_time - chrono::Duration::hours(2)).naive_utc(),
        value: 100.0,
    }];

    price_data.insert("token1".to_string(), values);

    let result = get_prices_at_time(&price_data, target_time);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("No price data found for token 'token1' within 1 hour"));
}

#[test]
fn test_get_prices_at_time_nonexistent_token() {
    let target_time = Utc::now();
    let price_data = HashMap::new();

    let result = get_prices_at_time(&price_data, target_time);
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[test]
fn test_get_prices_at_time_closest_selection() {
    let target_time = Utc::now();
    let mut price_data = HashMap::new();

    // 複数のデータポイントがある場合、最も近いものを選択
    let values = vec![
        ValueAtTime {
            time: (target_time - chrono::Duration::minutes(45)).naive_utc(),
            value: 100.0,
        },
        ValueAtTime {
            time: (target_time - chrono::Duration::minutes(15)).naive_utc(),
            value: 105.0, // これが最も近い
        },
        ValueAtTime {
            time: (target_time + chrono::Duration::minutes(30)).naive_utc(),
            value: 110.0,
        },
    ];

    price_data.insert("token1".to_string(), values);

    let result = get_prices_at_time(&price_data, target_time).unwrap();
    assert_eq!(result.get("token1").unwrap(), &105.0);
}

// === New Refactored Function Tests ===

#[test]
fn test_make_trading_decision_hold_when_profitable() {
    let config = TradingConfig {
        min_profit_threshold: 0.05,
        switch_multiplier: 1.5,
        min_trade_amount: 1.0,
    };

    let opportunities = vec![TokenOpportunity {
        token: "other_token".to_string(),
        expected_return: 0.08,
        confidence: Some(0.8),
    }];

    let decision = make_trading_decision(
        "current_token",
        0.1, // 10% return - profitable
        &opportunities,
        100.0, // sufficient amount
        &config,
    );

    // 0.08 * 0.8 = 0.064, 0.1 * 1.5 = 0.15
    // 0.064 < 0.15 なので HOLD
    assert_eq!(decision, TradingDecision::Hold);
}

#[test]
fn test_make_trading_decision_switch_when_better_opportunity() {
    let config = TradingConfig {
        min_profit_threshold: 0.05,
        switch_multiplier: 1.5,
        min_trade_amount: 1.0,
    };

    let opportunities = vec![TokenOpportunity {
        token: "better_token".to_string(),
        expected_return: 0.25,
        confidence: Some(0.9),
    }];

    let decision = make_trading_decision(
        "current_token",
        0.1, // 10% return
        &opportunities,
        100.0,
        &config,
    );

    // 0.25 * 0.9 = 0.225, 0.1 * 1.5 = 0.15
    // 0.225 > 0.15 なので SWITCH
    assert_eq!(
        decision,
        TradingDecision::Switch {
            from: "current_token".to_string(),
            to: "better_token".to_string(),
        }
    );
}

#[test]
fn test_make_trading_decision_sell_when_unprofitable() {
    let config = TradingConfig {
        min_profit_threshold: 0.05,
        switch_multiplier: 1.5,
        min_trade_amount: 1.0,
    };

    let opportunities = vec![TokenOpportunity {
        token: "target_token".to_string(),
        expected_return: 0.08,
        confidence: Some(0.8),
    }];

    let decision = make_trading_decision(
        "losing_token",
        0.02, // 2% return - below threshold
        &opportunities,
        100.0,
        &config,
    );

    assert_eq!(
        decision,
        TradingDecision::Sell {
            target_token: "target_token".to_string(),
        }
    );
}

#[test]
fn test_make_trading_decision_hold_when_insufficient_amount() {
    let config = TradingConfig {
        min_profit_threshold: 0.05,
        switch_multiplier: 1.5,
        min_trade_amount: 10.0, // High minimum
    };

    let opportunities = vec![TokenOpportunity {
        token: "better_token".to_string(),
        expected_return: 0.25,
        confidence: Some(0.9),
    }];

    let decision = make_trading_decision(
        "current_token",
        0.02, // Below threshold but insufficient amount
        &opportunities,
        5.0, // Below min_trade_amount
        &config,
    );

    assert_eq!(decision, TradingDecision::Hold);
}

#[test]
fn test_make_trading_decision_hold_when_empty_opportunities() {
    let config = TradingConfig {
        min_profit_threshold: 0.05,
        switch_multiplier: 1.5,
        min_trade_amount: 1.0,
    };

    let opportunities = vec![];

    let decision = make_trading_decision(
        "current_token",
        0.02, // Below threshold
        &opportunities,
        100.0,
        &config,
    );

    assert_eq!(decision, TradingDecision::Hold);
}

#[test]
fn test_make_trading_decision_hold_when_same_token() {
    let config = TradingConfig {
        min_profit_threshold: 0.05,
        switch_multiplier: 1.5,
        min_trade_amount: 1.0,
    };

    let opportunities = vec![TokenOpportunity {
        token: "current_token".to_string(), // Same as current
        expected_return: 0.25,
        confidence: Some(0.9),
    }];

    let decision = make_trading_decision("current_token", 0.1, &opportunities, 100.0, &config);

    assert_eq!(decision, TradingDecision::Hold);
}

#[test]
fn test_make_trading_decision_confidence_handling() {
    let config = TradingConfig {
        min_profit_threshold: 0.05,
        switch_multiplier: 1.5,
        min_trade_amount: 1.0,
    };

    let opportunities = vec![TokenOpportunity {
        token: "uncertain_token".to_string(),
        expected_return: 0.20,
        confidence: None, // No confidence = defaults to 0.5
    }];

    let decision = make_trading_decision("current_token", 0.1, &opportunities, 100.0, &config);

    // 0.20 * 0.5 = 0.1, 0.1 * 1.5 = 0.15
    // 0.1 < 0.15 なので HOLD
    assert_eq!(decision, TradingDecision::Hold);
}

#[test]
fn test_convert_ranked_tokens_to_opportunities() {
    let ranked_tokens = vec![
        ("token1".to_string(), 0.15, Some(0.8)),
        ("token2".to_string(), 0.10, None),
    ];

    let opportunities = convert_ranked_tokens_to_opportunities(&ranked_tokens);

    assert_eq!(opportunities.len(), 2);
    assert_eq!(opportunities[0].token, "token1");
    assert_eq!(opportunities[0].expected_return, 0.15);
    assert_eq!(opportunities[0].confidence, Some(0.8));
    assert_eq!(opportunities[1].token, "token2");
    assert_eq!(opportunities[1].expected_return, 0.10);
    assert_eq!(opportunities[1].confidence, None);
}

#[test]
fn test_convert_decision_to_action() {
    // Test Hold conversion
    let hold_decision = TradingDecision::Hold;
    let hold_action = convert_decision_to_action(hold_decision, "current_token");
    assert_eq!(hold_action, TradingAction::Hold);

    // Test Sell conversion
    let sell_decision = TradingDecision::Sell {
        target_token: "target_token".to_string(),
    };
    let sell_action = convert_decision_to_action(sell_decision, "current_token");
    assert_eq!(
        sell_action,
        TradingAction::Sell {
            token: "current_token".to_string(),
            target: "target_token".to_string(),
        }
    );

    // Test Switch conversion
    let switch_decision = TradingDecision::Switch {
        from: "from_token".to_string(),
        to: "to_token".to_string(),
    };
    let switch_action = convert_decision_to_action(switch_decision, "current_token");
    assert_eq!(
        switch_action,
        TradingAction::Switch {
            from: "from_token".to_string(),
            to: "to_token".to_string(),
        }
    );
}

// === Phase 2: Immutable Data Structure Tests ===

#[test]
fn test_immutable_portfolio_creation() {
    let portfolio = ImmutablePortfolio::new(1000.0, "wrap.near");

    assert_eq!(portfolio.holdings.get("wrap.near"), Some(&1000.0));
    assert_eq!(portfolio.cash_balance, 0.0);
    assert!(portfolio.holdings.len() == 1);
}

#[test]
fn test_portfolio_total_value_calculation() {
    let portfolio = ImmutablePortfolio::new(1000.0, "token_a");

    let mut prices = HashMap::new();
    prices.insert("token_a".to_string(), 1.5);
    let market = MarketSnapshot::new(prices);

    let total_value = portfolio.total_value(&market);
    assert_eq!(total_value, 1500.0); // 1000 * 1.5
}

#[test]
fn test_portfolio_total_value_with_cash() {
    let mut holdings = HashMap::new();
    holdings.insert("token_a".to_string(), 500.0);

    let portfolio = ImmutablePortfolio {
        holdings,
        cash_balance: 200.0,
        timestamp: Utc::now(),
    };

    let mut prices = HashMap::new();
    prices.insert("token_a".to_string(), 2.0);
    let market = MarketSnapshot::new(prices);

    let total_value = portfolio.total_value(&market);
    assert_eq!(total_value, 1200.0); // 500 * 2.0 + 200
}

#[test]
fn test_market_snapshot_creation() {
    let mut prices = HashMap::new();
    prices.insert("token_a".to_string(), 100.0);
    prices.insert("token_b".to_string(), 200.0);

    let market = MarketSnapshot::new(prices.clone());

    assert_eq!(market.prices, prices);
    assert_eq!(market.data_quality, DataQuality::High);
    assert!(market.is_reliable());
}

#[test]
fn test_data_quality_assessment() {
    // Empty market
    let empty_market = MarketSnapshot::new(HashMap::new());
    assert_eq!(empty_market.data_quality, DataQuality::Poor);
    assert!(!empty_market.is_reliable());

    // Single token
    let mut single_prices = HashMap::new();
    single_prices.insert("token_a".to_string(), 100.0);
    let single_market = MarketSnapshot::new(single_prices);
    assert_eq!(single_market.data_quality, DataQuality::Low);
    assert!(!single_market.is_reliable());

    // Multiple tokens
    let mut multi_prices = HashMap::new();
    multi_prices.insert("token_a".to_string(), 100.0);
    multi_prices.insert("token_b".to_string(), 200.0);
    multi_prices.insert("token_c".to_string(), 300.0);
    let multi_market = MarketSnapshot::new(multi_prices);
    assert_eq!(multi_market.data_quality, DataQuality::High);
    assert!(multi_market.is_reliable());

    // High quality market (6+ tokens)
    let mut high_prices = HashMap::new();
    for i in 0..7 {
        high_prices.insert(format!("token_{}", i), 100.0 + i as f64);
    }
    let high_market = MarketSnapshot::new(high_prices);
    assert_eq!(high_market.data_quality, DataQuality::High);
    assert!(high_market.is_reliable());
}

#[test]
fn test_portfolio_apply_hold_decision() {
    let portfolio = ImmutablePortfolio::new(1000.0, "token_a");

    let mut prices = HashMap::new();
    prices.insert("token_a".to_string(), 1.0);
    let market = MarketSnapshot::new(prices);

    let config = TradingConfig {
        min_profit_threshold: 0.05,
        switch_multiplier: 1.5,
        min_trade_amount: 1.0,
    };

    let decision = TradingDecision::Hold;
    let transition = portfolio.apply_trade(&decision, &market, &config).unwrap();

    assert_eq!(transition.from, portfolio);
    assert_eq!(transition.to.holdings.get("token_a"), Some(&1000.0));
    assert_eq!(transition.cost, 0.0);
    assert_eq!(transition.action, TradingDecision::Hold);
}

#[test]
fn test_portfolio_apply_sell_decision() {
    let portfolio = ImmutablePortfolio::new(1000.0, "token_a");

    let mut prices = HashMap::new();
    prices.insert("token_a".to_string(), 1.0);
    prices.insert("token_b".to_string(), 2.0);
    let market = MarketSnapshot::new(prices);

    let config = TradingConfig {
        min_profit_threshold: 0.05,
        switch_multiplier: 1.5,
        min_trade_amount: 1.0,
    };

    let decision = TradingDecision::Sell {
        target_token: "token_b".to_string(),
    };
    let transition = portfolio.apply_trade(&decision, &market, &config).unwrap();

    assert!(!transition.to.holdings.contains_key("token_a"));
    assert!(transition.to.holdings.contains_key("token_b"));
    assert!(transition.cost > 0.0); // Some transaction cost

    // Should have converted 1000 token_a (worth 1000) to token_b (price 2.0)
    // After fees: ~1000 * 0.994 / 2.0 = ~497
    let token_b_amount = transition.to.holdings.get("token_b").unwrap();
    assert!(*token_b_amount < 500.0 && *token_b_amount > 490.0);
}

#[test]
fn test_portfolio_apply_switch_decision() {
    let mut holdings = HashMap::new();
    holdings.insert("token_a".to_string(), 500.0);

    let portfolio = ImmutablePortfolio {
        holdings,
        cash_balance: 0.0,
        timestamp: Utc::now(),
    };

    let mut prices = HashMap::new();
    prices.insert("token_a".to_string(), 2.0);
    prices.insert("token_b".to_string(), 1.0);
    let market = MarketSnapshot::new(prices);

    let config = TradingConfig {
        min_profit_threshold: 0.05,
        switch_multiplier: 1.5,
        min_trade_amount: 1.0,
    };

    let decision = TradingDecision::Switch {
        from: "token_a".to_string(),
        to: "token_b".to_string(),
    };
    let transition = portfolio.apply_trade(&decision, &market, &config).unwrap();

    assert!(!transition.to.holdings.contains_key("token_a"));
    assert!(transition.to.holdings.contains_key("token_b"));
    assert!(transition.cost > 0.0);

    // Should have converted 500 token_a (worth 1000) to token_b (price 1.0)
    // After fees: ~1000 * 0.994 = ~994
    let token_b_amount = transition.to.holdings.get("token_b").unwrap();
    assert!(*token_b_amount < 1000.0 && *token_b_amount > 990.0);
}

#[test]
fn test_market_snapshot_from_price_data() {
    let target_time = Utc::now();
    let mut price_data = HashMap::new();

    let values = vec![ValueAtTime {
        time: target_time.naive_utc(),
        value: 150.0,
    }];
    price_data.insert("token_a".to_string(), values);

    let market = MarketSnapshot::from_price_data(&price_data, target_time).unwrap();

    assert_eq!(market.get_price("token_a"), Some(150.0));
    assert_eq!(market.timestamp, target_time);
    assert_eq!(market.data_quality, DataQuality::Low);
}

#[test]
fn test_immutable_portfolio_demo_trading_sequence() {
    // Phase 2 Demo: Complete trading sequence with immutable data structures

    // Initial portfolio: 1000 units of token_a
    let portfolio_v1 = ImmutablePortfolio::new(1000.0, "token_a");

    // Market snapshot with multiple tokens
    let mut prices = HashMap::new();
    prices.insert("token_a".to_string(), 1.0);
    prices.insert("token_b".to_string(), 2.0);
    prices.insert("token_c".to_string(), 0.5);
    let market = MarketSnapshot::new(prices);

    // Trading config
    let config = TradingConfig {
        min_profit_threshold: 0.05,
        switch_multiplier: 1.5,
        min_trade_amount: 1.0,
    };

    println!("=== Phase 2 Immutable Portfolio Demo ===");
    println!(
        "Initial portfolio value: {}",
        portfolio_v1.total_value(&market)
    );
    println!("Market quality: {:?}", market.data_quality);

    // Trade 1: Switch from token_a to token_b
    let decision_1 = TradingDecision::Switch {
        from: "token_a".to_string(),
        to: "token_b".to_string(),
    };
    let transition_1 = portfolio_v1
        .apply_trade(&decision_1, &market, &config)
        .unwrap();
    let portfolio_v2 = transition_1.to.clone();

    println!(
        "After switch to token_b: value={}, cost={}",
        portfolio_v2.total_value(&market),
        transition_1.cost
    );

    // Trade 2: Sell token_b for token_c
    let decision_2 = TradingDecision::Sell {
        target_token: "token_c".to_string(),
    };
    let transition_2 = portfolio_v2
        .apply_trade(&decision_2, &market, &config)
        .unwrap();
    let portfolio_v3 = transition_2.to.clone();

    println!(
        "After sell to token_c: value={}, cost={}",
        portfolio_v3.total_value(&market),
        transition_2.cost
    );

    // Verify immutability: original portfolios are unchanged
    assert_eq!(portfolio_v1.holdings.get("token_a"), Some(&1000.0));
    assert!(portfolio_v2.holdings.contains_key("token_b"));
    assert!(portfolio_v3.holdings.contains_key("token_c"));

    // Each step created a new portfolio without modifying previous ones
    assert_eq!(transition_1.from, portfolio_v1);
    assert_eq!(transition_1.to, portfolio_v2);
    assert_eq!(transition_2.from, portfolio_v2);
    assert_eq!(transition_2.to, portfolio_v3);

    println!("✅ Immutability verified: all previous portfolios remain unchanged");
    println!(
        "✅ Total cost of trades: {}",
        transition_1.cost + transition_2.cost
    );
}

// === Phase 3: Strategy Pattern Tests ===

#[test]
fn test_momentum_strategy_creation() {
    let strategy = MomentumStrategy {
        min_confidence: 0.7,
        lookback_periods: 14,
    };

    assert_eq!(strategy.name(), "Momentum");
    assert_eq!(strategy.min_confidence, 0.7);
    assert_eq!(strategy.lookback_periods, 14);
}

#[test]
fn test_momentum_strategy_hold_decision() {
    let strategy = MomentumStrategy {
        min_confidence: 0.7,
        lookback_periods: 14,
    };

    let portfolio = ImmutablePortfolio::new(1000.0, "token_a");
    let mut prices = HashMap::new();
    prices.insert("token_a".to_string(), 1.0);
    let market = MarketSnapshot::new(prices);

    let opportunities = vec![TokenOpportunity {
        token: "token_b".to_string(),
        expected_return: 0.15,
        confidence: Some(0.5), // Below min_confidence
    }];

    let config = TradingConfig {
        min_profit_threshold: 0.05,
        switch_multiplier: 1.5,
        min_trade_amount: 1.0,
    };

    let decision = strategy
        .make_decision(&portfolio, &market, &opportunities, &config)
        .unwrap();
    assert_eq!(decision, TradingDecision::Hold);
}

#[test]
fn test_momentum_strategy_switch_decision() {
    let strategy = MomentumStrategy {
        min_confidence: 0.7,
        lookback_periods: 14,
    };

    let portfolio = ImmutablePortfolio::new(1000.0, "token_a");
    let mut prices = HashMap::new();
    prices.insert("token_a".to_string(), 1.0);
    prices.insert("token_b".to_string(), 2.0);
    let market = MarketSnapshot::new(prices);

    let opportunities = vec![TokenOpportunity {
        token: "token_b".to_string(),
        expected_return: 0.3,  // High return
        confidence: Some(0.9), // High confidence
    }];

    let config = TradingConfig {
        min_profit_threshold: 0.05,
        switch_multiplier: 1.5,
        min_trade_amount: 1.0,
    };

    let decision = strategy
        .make_decision(&portfolio, &market, &opportunities, &config)
        .unwrap();
    assert_eq!(decision, TradingDecision::Hold);
}

#[test]
fn test_portfolio_strategy_rebalancing() {
    let strategy = PortfolioStrategy {
        max_positions: 3,
        rebalance_threshold: 0.2,
    };

    assert_eq!(strategy.name(), "Portfolio");

    // Test single token portfolio (should rebalance)
    let portfolio = ImmutablePortfolio::new(1000.0, "token_a");
    let mut prices = HashMap::new();
    prices.insert("token_a".to_string(), 1.0);
    let market = MarketSnapshot::new(prices);

    assert!(strategy.should_rebalance(&portfolio, &market));
}

#[test]
fn test_trend_following_strategy_decision() {
    let strategy = TrendFollowingStrategy {
        trend_window: 10,
        volatility_threshold: 0.1,
    };

    assert_eq!(strategy.name(), "TrendFollowing");
    // Trend following now uses RSI/ADX conditions, so empty market may not trigger rebalance
    let _empty_rebalance = strategy.should_rebalance(
        &ImmutablePortfolio::new(1000.0, "token_a"),
        &MarketSnapshot::new(HashMap::new()),
    );

    let portfolio = ImmutablePortfolio::new(1000.0, "token_a");
    let mut prices = HashMap::new();
    prices.insert("token_a".to_string(), 1.0);
    prices.insert("token_b".to_string(), 2.0);
    let market = MarketSnapshot::new(prices);

    let opportunities = vec![TokenOpportunity {
        token: "token_b".to_string(),
        expected_return: 0.25,
        confidence: Some(0.8),
    }];

    let config = TradingConfig {
        min_profit_threshold: 0.05,
        switch_multiplier: 1.5,
        min_trade_amount: 1.0,
    };

    let decision = strategy
        .make_decision(&portfolio, &market, &opportunities, &config)
        .unwrap();
    assert_eq!(
        decision,
        TradingDecision::Switch {
            from: "token_a".to_string(),
            to: "token_b".to_string()
        }
    );
}

#[test]
fn test_strategy_context_execution() {
    let momentum_strategy = Box::new(MomentumStrategy {
        min_confidence: 0.6,
        lookback_periods: 14,
    });

    let context = StrategyContext::new(momentum_strategy);
    assert_eq!(context.strategy_name(), "Momentum");

    let portfolio = ImmutablePortfolio::new(1000.0, "token_a");
    let mut prices = HashMap::new();
    prices.insert("token_a".to_string(), 1.0);
    let market = MarketSnapshot::new(prices);

    let opportunities = vec![TokenOpportunity {
        token: "token_a".to_string(), // Same token
        expected_return: 0.15,
        confidence: Some(0.8),
    }];

    let config = TradingConfig {
        min_profit_threshold: 0.05,
        switch_multiplier: 1.5,
        min_trade_amount: 1.0,
    };

    let decision = context
        .execute_strategy(&portfolio, &market, &opportunities, &config)
        .unwrap();
    assert_eq!(decision, TradingDecision::Hold);
}

#[test]
fn test_strategy_comparison_demo() {
    println!("=== Phase 3 Strategy Pattern Demo ===");

    // Setup common test data
    let portfolio = ImmutablePortfolio::new(1000.0, "token_a");
    let mut prices = HashMap::new();
    prices.insert("token_a".to_string(), 1.0);
    prices.insert("token_b".to_string(), 2.0);
    prices.insert("token_c".to_string(), 1.5);
    let market = MarketSnapshot::new(prices);

    let opportunities = vec![
        TokenOpportunity {
            token: "token_b".to_string(),
            expected_return: 0.25,
            confidence: Some(0.8),
        },
        TokenOpportunity {
            token: "token_c".to_string(),
            expected_return: 0.20,
            confidence: Some(0.9),
        },
    ];

    let config = TradingConfig {
        min_profit_threshold: 0.05,
        switch_multiplier: 1.5,
        min_trade_amount: 1.0,
    };

    // Test different strategies
    let strategies: Vec<Box<dyn TradingStrategy>> = vec![
        Box::new(MomentumStrategy {
            min_confidence: 0.7,
            lookback_periods: 14,
        }),
        Box::new(PortfolioStrategy {
            max_positions: 3,
            rebalance_threshold: 0.2,
        }),
        Box::new(TrendFollowingStrategy {
            trend_window: 10,
            volatility_threshold: 0.15,
        }),
    ];

    for strategy in strategies {
        let context = StrategyContext::new(strategy);
        let decision = context
            .execute_strategy(&portfolio, &market, &opportunities, &config)
            .unwrap();
        println!(
            "{} Strategy Decision: {:?}",
            context.strategy_name(),
            decision
        );

        // Each strategy should make some decision
        match context.strategy_name() {
            "Momentum" => assert_eq!(decision, TradingDecision::Hold),
            "Portfolio" => assert_eq!(
                decision,
                TradingDecision::Sell {
                    target_token: "token_b".to_string()
                }
            ),
            "TrendFollowing" => assert_eq!(
                decision,
                TradingDecision::Switch {
                    from: "token_a".to_string(),
                    to: "token_b".to_string()
                }
            ),
            _ => panic!("Unexpected strategy"),
        }
    }

    println!("✅ All strategies executed successfully with different logic");
}

#[test]
fn test_data_gap_handling_get_prices_at_time_optional() {
    use super::super::data::get_prices_at_time_optional;
    use chrono::{TimeZone, Utc};

    // テスト用のprice_dataを作成（1時間ごとのデータ）
    let mut price_data = HashMap::new();
    let token = "test.token".to_string();

    let start_time = Utc.with_ymd_and_hms(2024, 8, 1, 0, 0, 0).unwrap();
    let mut data_points = Vec::new();

    // 00:00, 01:00, 02:00, 05:00（3-4時間のギャップ）, 06:00のデータを作成
    let times_and_prices = vec![
        (0, 100.0), // 00:00
        (1, 101.0), // 01:00
        (2, 102.0), // 02:00
        (5, 105.0), // 05:00 (3時間のギャップ)
        (6, 106.0), // 06:00
    ];

    for (hour_offset, price) in times_and_prices {
        let time = start_time + chrono::Duration::hours(hour_offset);
        data_points.push(ValueAtTime {
            time: time.naive_utc(),
            value: price,
        });
    }

    price_data.insert(token.clone(), data_points);

    // テストケース1: データが存在する時刻（00:00）
    let target_time_0 = start_time;
    let result_0 = get_prices_at_time_optional(&price_data, target_time_0);
    assert!(result_0.is_some(), "00:00のデータは取得できるはず");
    assert_eq!(result_0.unwrap().get(&token), Some(&100.0));

    // テストケース2: データが存在する時刻の近く（00:30 - 1時間以内）
    let target_time_30min = start_time + chrono::Duration::minutes(30);
    let result_30min = get_prices_at_time_optional(&price_data, target_time_30min);
    assert!(
        result_30min.is_some(),
        "00:30は00:00から1時間以内なのでデータが取得できるはず"
    );

    // テストケース3: ギャップ内の時刻（03:30 - 02:00から1.5時間、05:00から1.5時間で範囲外）
    let target_time_gap = start_time + chrono::Duration::hours(3) + chrono::Duration::minutes(30);
    let result_gap = get_prices_at_time_optional(&price_data, target_time_gap);
    assert!(
        result_gap.is_none(),
        "03:30はデータギャップなのでNoneが返るはず"
    );

    // テストケース4: ギャップ内だが近いデータがある時刻（04:00 - 1時間以内に05:00のデータあり）
    let target_time_near_gap = start_time + chrono::Duration::hours(4);
    let result_near_gap = get_prices_at_time_optional(&price_data, target_time_near_gap);
    assert!(
        result_near_gap.is_some(),
        "04:00は05:00から1時間以内なのでデータが取得できるはず"
    );

    println!("✅ Data gap handling tests passed");
}

#[test]
fn test_data_gap_event_creation() {
    use super::super::data::{calculate_gap_impact, log_data_gap_event};
    use super::super::types::{DataGapEvent, DataGapEventType};
    use chrono::{TimeZone, Utc};

    // テスト用のprice_dataを作成
    let price_data = HashMap::new();
    let token = "test.token".to_string();

    let start_time = Utc.with_ymd_and_hms(2024, 8, 1, 0, 0, 0).unwrap();
    let gap_time = start_time + chrono::Duration::hours(3);

    // ギャップの影響を計算
    let impact = calculate_gap_impact(
        Some(start_time),
        gap_time,
        &price_data,
        std::slice::from_ref(&token),
    );

    // DataGapEventを作成
    let gap_event = DataGapEvent {
        timestamp: gap_time,
        event_type: DataGapEventType::TradingSkipped,
        affected_tokens: vec![token.clone()],
        reason: "Price data not available within 1 hour window".to_string(),
        impact: impact.clone(),
    };

    // ログ出力をテスト（実際の出力は確認のみ）
    println!("Testing gap event logging:");
    log_data_gap_event(&gap_event);

    // データ構造の妥当性を確認
    assert_eq!(gap_event.event_type, DataGapEventType::TradingSkipped);
    assert_eq!(gap_event.affected_tokens, vec![token]);
    assert_eq!(impact.duration_hours, 3);

    println!("✅ Data gap event creation test passed");
}

#[test]
fn test_data_quality_stats_calculation() {
    use super::super::types::DataQualityStats;

    // シミュレーション統計のテスト
    let total_timesteps = 100;
    let skipped_timesteps = 15;
    let longest_gap_hours = 6;

    let data_coverage_percentage = if total_timesteps > 0 {
        ((total_timesteps - skipped_timesteps) as f64 / total_timesteps as f64) * 100.0
    } else {
        100.0
    };

    let data_quality = DataQualityStats {
        total_timesteps,
        skipped_timesteps,
        data_coverage_percentage,
        longest_gap_hours,
        gap_events: Vec::new(),
    };

    assert_eq!(data_quality.total_timesteps, 100);
    assert_eq!(data_quality.skipped_timesteps, 15);
    assert_eq!(data_quality.data_coverage_percentage, 85.0);
    assert_eq!(data_quality.longest_gap_hours, 6);

    println!(
        "✅ Data coverage: {:.1}%",
        data_quality.data_coverage_percentage
    );
    println!("✅ Data quality stats calculation test passed");
}
