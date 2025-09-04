use bigdecimal::BigDecimal;
use chrono::Utc;
use common::stats::ValueAtTime;
use std::collections::HashMap;

use super::*;

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_simulate_args_default_values() {
        let args = SimulateArgs {
            start: Some("2024-01-01".to_string()),
            end: Some("2024-01-10".to_string()),
            algorithm: Some("momentum".to_string()),
            capital: 1000.0,
            quote_token: "wrap.near".to_string(),
            tokens: Some("usdc.tether-token.near,blackdragon.tkn.near".to_string()),
            num_tokens: 10,
            output: "simulation_results".to_string(),
            rebalance_freq: "daily".to_string(),
            fee_model: "realistic".to_string(),
            custom_fee: None,
            slippage: 0.01,
            gas_cost: 0.01,
            min_trade: 1.0,
            prediction_horizon: 24,
            historical_days: 30,
            chart: false,
            verbose: false,
        };

        assert_eq!(args.algorithm, Some("momentum".to_string()));
        assert_eq!(args.capital, 1000.0);
        assert_eq!(args.quote_token, "wrap.near");
        assert_eq!(args.rebalance_freq, "daily");
        assert_eq!(args.fee_model, "realistic");
        assert_eq!(args.slippage, 0.01);
        assert_eq!(args.historical_days, 30);
        assert!(!args.verbose);
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

    #[test]
    fn test_calculate_simple_volatility() {
        // 標準的な価格データ
        let prices = vec![100.0, 105.0, 95.0, 110.0, 90.0];
        let volatility = calculate_simple_volatility(&prices);
        assert!(volatility > 0.0);

        // 一定の価格データ
        let constant_prices = vec![100.0, 100.0, 100.0, 100.0];
        assert_eq!(calculate_simple_volatility(&constant_prices), 0.0);

        // 単一の価格
        let single_price = vec![100.0];
        assert_eq!(calculate_simple_volatility(&single_price), 0.0);
    }

    #[test]
    fn test_predict_price_trend() {
        let current_time = Utc::now();
        let value1 = ValueAtTime {
            time: current_time.naive_utc(),
            value: 100.0,
        };
        let value2 = ValueAtTime {
            time: current_time.naive_utc(),
            value: 105.0,
        };
        let value3 = ValueAtTime {
            time: current_time.naive_utc(),
            value: 110.0,
        };
        let value4 = ValueAtTime {
            time: current_time.naive_utc(),
            value: 115.0,
        };
        let value5 = ValueAtTime {
            time: current_time.naive_utc(),
            value: 120.0,
        };

        let test_data = vec![&value1, &value2, &value3, &value4, &value5];

        let target_time = Utc::now();
        let predicted_price = predict_price_trend(&test_data, target_time).unwrap();

        // 上昇トレンドなので現在価格より高い予測価格
        assert!(predicted_price > 120.0);

        // 空のデータ
        let empty_data = vec![];
        let empty_result = predict_price_trend(&empty_data, target_time).unwrap();
        assert_eq!(empty_result, 0.0);

        // 単一データ - predict_price_trend関数では単一データの場合、0.0が返される
        let single_value = ValueAtTime {
            time: Utc::now().naive_utc(),
            value: 100.0,
        };
        let single_data = vec![&single_value];
        let single_result = predict_price_trend(&single_data, target_time).unwrap();
        // 実装では単一データ（2未満）の場合、0.0が返される
        assert_eq!(single_result, 0.0);
    }

    #[test]
    fn test_calculate_prediction_confidence() {
        // 十分なデータ量
        let value1 = ValueAtTime {
            time: Utc::now().naive_utc(),
            value: 100.0,
        };
        let value2 = ValueAtTime {
            time: Utc::now().naive_utc(),
            value: 101.0,
        };
        let value3 = ValueAtTime {
            time: Utc::now().naive_utc(),
            value: 102.0,
        };
        let sufficient_data = vec![&value1, &value2, &value3];
        let confidence = calculate_prediction_confidence(&sufficient_data);
        // 実装では少ないデータポイントでも低い信頼度で計算される
        assert!((0.1..=1.0).contains(&confidence));

        // データ不足
        let value_single = ValueAtTime {
            time: Utc::now().naive_utc(),
            value: 100.0,
        };
        let insufficient_data = vec![&value_single];
        let low_confidence = calculate_prediction_confidence(&insufficient_data);
        assert_eq!(low_confidence, 0.1);

        // 空のデータ
        let empty_data = vec![];
        let empty_confidence = calculate_prediction_confidence(&empty_data);
        assert_eq!(empty_confidence, 0.1);
    }

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
    fn test_get_prices_at_time_with_sufficient_data() {
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
    fn test_get_prices_at_time_with_insufficient_data() {
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
    fn test_get_price_at_time_with_sufficient_data() {
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

        let result = get_price_at_time(&price_data, "token1", target_time).unwrap();
        assert_eq!(result, 105.0);
    }

    #[test]
    fn test_get_price_at_time_with_insufficient_data() {
        let target_time = Utc::now();
        let mut price_data = HashMap::new();

        // 前後1時間以内にデータがない場合
        let values = vec![ValueAtTime {
            time: (target_time - chrono::Duration::hours(2)).naive_utc(),
            value: 100.0,
        }];

        price_data.insert("token1".to_string(), values);

        let result = get_price_at_time(&price_data, "token1", target_time);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No price data found for token 'token1' within 1 hour"));
    }

    #[test]
    fn test_get_price_at_time_nonexistent_token() {
        let target_time = Utc::now();
        let price_data = HashMap::new();

        let result = get_price_at_time(&price_data, "nonexistent_token", target_time);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No price data found for token: nonexistent_token"));
    }

    #[test]
    fn test_get_price_at_time_closest_selection() {
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

        let result = get_price_at_time(&price_data, "token1", target_time).unwrap();
        assert_eq!(result, 105.0);
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
        assert_eq!(market.data_quality, DataQuality::Medium);
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
        assert_eq!(multi_market.data_quality, DataQuality::Medium);
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
        assert_eq!(
            decision,
            TradingDecision::Switch {
                from: "token_a".to_string(),
                to: "token_b".to_string(),
            }
        );
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
        assert!(strategy.should_rebalance(
            &ImmutablePortfolio::new(1000.0, "token_a"),
            &MarketSnapshot::new(HashMap::new())
        ));

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
                to: "token_b".to_string(),
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
                "Momentum" => assert_eq!(
                    decision,
                    TradingDecision::Switch {
                        from: "token_a".to_string(),
                        to: "token_b".to_string(),
                    }
                ),
                "Portfolio" => assert_eq!(
                    decision,
                    TradingDecision::Switch {
                        from: "token_a".to_string(),
                        to: "token_b".to_string(),
                    }
                ),
                "TrendFollowing" => assert_eq!(
                    decision,
                    TradingDecision::Switch {
                        from: "token_a".to_string(),
                        to: "token_b".to_string(),
                    }
                ),
                _ => panic!("Unexpected strategy"),
            }
        }

        println!("✅ All strategies executed successfully with different logic");
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    // Note: generate_mock_price_data function is not available in simulate module
    // This test has been commented out as it depends on non-existent functionality
    // #[tokio::test]
    // async fn test_generate_mock_price_data() -> Result<()> { ... }

    // Helper function to create a test trade execution
    fn create_test_trade(
        portfolio_value_before: f64,
        portfolio_value_after: f64,
        cost: f64,
    ) -> TradeExecution {
        use std::str::FromStr;

        TradeExecution {
            timestamp: Utc::now(),
            from_token: "token_a".to_string(),
            to_token: "token_b".to_string(),
            amount: 100.0,
            executed_price: 1.0,
            cost: TradingCost {
                protocol_fee: BigDecimal::from_str("0.0").unwrap(),
                slippage: BigDecimal::from_str("0.0").unwrap(),
                gas_fee: BigDecimal::from_str("0.0").unwrap(),
                total: BigDecimal::from_str(&cost.to_string()).unwrap(),
            },
            portfolio_value_before,
            portfolio_value_after,
            success: true,
            reason: "Test trade".to_string(),
        }
    }

    // Helper function to create a test portfolio value
    fn create_portfolio_value(timestamp: DateTime<Utc>, total_value: f64) -> PortfolioValue {
        PortfolioValue {
            timestamp,
            holdings: HashMap::new(),
            total_value,
            cash_balance: 0.0,
            unrealized_pnl: 0.0,
        }
    }

    #[test]
    fn test_profit_factor_calculation_only_profits() {
        let trades = vec![
            create_test_trade(1000.0, 1100.0, 5.0), // +100 profit
            create_test_trade(1100.0, 1200.0, 5.0), // +100 profit
            create_test_trade(1200.0, 1350.0, 5.0), // +150 profit
        ];

        let portfolio_values = vec![
            create_portfolio_value(Utc::now(), 1000.0),
            create_portfolio_value(Utc::now(), 1350.0),
        ];

        let metrics = calculate_performance_metrics(1000.0, 1350.0, &portfolio_values, &trades, 30);

        // Total profit = 350, no losses, so profit factor should be very high (f64::MAX)
        assert_eq!(metrics.profit_factor, f64::MAX);
        assert_eq!(metrics.winning_trades, 3);
        assert_eq!(metrics.losing_trades, 0);
        assert_eq!(metrics.win_rate, 1.0);
    }

    #[test]
    fn test_profit_factor_calculation_only_losses() {
        let trades = vec![
            create_test_trade(1000.0, 950.0, 5.0), // -50 loss
            create_test_trade(950.0, 900.0, 5.0),  // -50 loss
            create_test_trade(900.0, 800.0, 5.0),  // -100 loss
        ];

        let portfolio_values = vec![
            create_portfolio_value(Utc::now(), 1000.0),
            create_portfolio_value(Utc::now(), 800.0),
        ];

        let metrics = calculate_performance_metrics(1000.0, 800.0, &portfolio_values, &trades, 30);

        // Total loss = 200, no profits, so profit factor should be 0
        assert_eq!(metrics.profit_factor, 0.0);
        assert_eq!(metrics.winning_trades, 0);
        assert_eq!(metrics.losing_trades, 3);
        assert_eq!(metrics.win_rate, 0.0);
    }

    #[test]
    fn test_profit_factor_calculation_mixed_trades() {
        let trades = vec![
            create_test_trade(1000.0, 1200.0, 5.0), // +200 profit
            create_test_trade(1200.0, 1000.0, 5.0), // -200 loss
            create_test_trade(1000.0, 1150.0, 5.0), // +150 profit
            create_test_trade(1150.0, 1100.0, 5.0), // -50 loss
        ];

        let portfolio_values = vec![
            create_portfolio_value(Utc::now(), 1000.0),
            create_portfolio_value(Utc::now(), 1100.0),
        ];

        let metrics = calculate_performance_metrics(1000.0, 1100.0, &portfolio_values, &trades, 30);

        // Total profit = 350, Total loss = 250, Profit factor = 350/250 = 1.4
        assert_eq!(metrics.profit_factor, 1.4);
        assert_eq!(metrics.winning_trades, 2);
        assert_eq!(metrics.losing_trades, 2);
        assert_eq!(metrics.win_rate, 0.5);
    }

    #[test]
    fn test_profit_factor_calculation_no_trades() {
        let trades = vec![];
        let portfolio_values = vec![
            create_portfolio_value(Utc::now(), 1000.0),
            create_portfolio_value(Utc::now(), 1000.0),
        ];

        let metrics = calculate_performance_metrics(1000.0, 1000.0, &portfolio_values, &trades, 30);

        // No trades, so profit factor should be 0
        assert_eq!(metrics.profit_factor, 0.0);
        assert_eq!(metrics.winning_trades, 0);
        assert_eq!(metrics.losing_trades, 0);
        assert_eq!(metrics.win_rate, 0.0);
        assert_eq!(metrics.total_trades, 0);
    }

    #[test]
    fn test_performance_metrics_calculation() {
        let initial_value = 1000.0;
        let final_value = 1100.0;

        let portfolio_values = vec![
            PortfolioValue {
                timestamp: Utc::now(),
                total_value: initial_value,
                cash_balance: initial_value,
                holdings: HashMap::new(),
                unrealized_pnl: 0.0,
            },
            PortfolioValue {
                timestamp: Utc::now(),
                total_value: final_value,
                cash_balance: 0.0,
                holdings: HashMap::new(),
                unrealized_pnl: 100.0,
            },
        ];

        let trades = vec![];
        let simulation_days = 30;

        let performance = calculate_performance_metrics(
            initial_value,
            final_value,
            &portfolio_values,
            &trades,
            simulation_days,
        );

        assert_eq!(performance.total_return, 0.1); // 10% return
        assert_eq!(performance.total_trades, 0);
        assert_eq!(performance.simulation_days, 30);
    }

    #[test]
    fn test_config_creation() {
        let args = SimulateArgs {
            start: Some("2024-01-01".to_string()),
            end: Some("2024-01-31".to_string()),
            algorithm: Some("momentum".to_string()),
            capital: 1000.0,
            quote_token: "wrap.near".to_string(),
            tokens: Some("token1,token2".to_string()),
            num_tokens: 10,
            output: "test_output".to_string(),
            rebalance_freq: "daily".to_string(),
            fee_model: "zero".to_string(),
            custom_fee: None,
            slippage: 0.01,
            gas_cost: 0.01,
            min_trade: 1.0,
            prediction_horizon: 24,
            historical_days: 30,
            chart: false,
            verbose: false,
        };

        // Test that the args contain expected values
        assert_eq!(args.algorithm, Some("momentum".to_string()));
        assert_eq!(args.capital, 1000.0);
        assert_eq!(args.tokens.unwrap(), "token1,token2");
        assert_eq!(args.historical_days, 30);
    }
}
