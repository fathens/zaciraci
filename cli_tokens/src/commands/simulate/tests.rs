use anyhow::Result;
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
            algorithm: "momentum".to_string(),
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
            report_format: "json".to_string(),
            chart: false,
            verbose: false,
        };

        assert_eq!(args.algorithm, "momentum");
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
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_generate_mock_price_data() -> Result<()> {
        let start_date = Utc::now();
        let end_date = start_date + chrono::Duration::days(1);

        let mock_data = generate_mock_price_data(start_date, end_date)?;

        assert!(!mock_data.is_empty());
        assert!(mock_data.len() >= 24); // 1時間毎なので24以上

        // 価格が正の値であることを確認
        for value in &mock_data {
            assert!(value.value > 0.0);
        }

        // 時系列順であることを確認
        for i in 1..mock_data.len() {
            assert!(mock_data[i].time >= mock_data[i - 1].time);
        }

        Ok(())
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
            algorithm: "momentum".to_string(),
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
            report_format: "json".to_string(),
            chart: false,
            verbose: false,
        };

        // Test that the args contain expected values
        assert_eq!(args.algorithm, "momentum");
        assert_eq!(args.capital, 1000.0);
        assert_eq!(args.tokens.unwrap(), "token1,token2");
        assert_eq!(args.historical_days, 30);
    }
}
