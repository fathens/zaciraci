use super::*;
use std::str::FromStr;

fn token_out(s: &str) -> TokenOutAccount {
    s.parse().unwrap()
}

// ==================== TradingAction のテスト ====================

#[test]
fn test_trading_action_hold() {
    let action = TradingAction::Hold;
    assert_eq!(action, TradingAction::Hold);
}

#[test]
fn test_trading_action_sell() {
    let action = TradingAction::Sell {
        token: "token1".parse().unwrap(),
        target: "token2".parse().unwrap(),
    };
    match action {
        TradingAction::Sell { token, target } => {
            assert_eq!(token.to_string(), "token1");
            assert_eq!(target.to_string(), "token2");
        }
        _ => panic!("Expected Sell action"),
    }
}

#[test]
fn test_trading_action_rebalance() {
    let mut weights = BTreeMap::new();
    weights.insert(token_out("token1"), 0.5);
    weights.insert(token_out("token2"), 0.5);

    let action = TradingAction::Rebalance {
        target_weights: weights.clone(),
    };

    match action {
        TradingAction::Rebalance { target_weights } => {
            assert_eq!(target_weights.len(), 2);
            assert_eq!(target_weights.get(&token_out("token1")), Some(&0.5));
        }
        _ => panic!("Expected Rebalance action"),
    }
}

#[test]
fn test_trading_action_clone() {
    let action1 = TradingAction::Hold;
    let action2 = action1.clone();
    assert_eq!(action1, action2);
}

// ==================== ExecutionReport のテスト ====================

#[test]
fn test_execution_report_new() {
    let actions = vec![TradingAction::Hold];
    let report = ExecutionReport::new(actions.clone(), AlgorithmType::Momentum);

    assert_eq!(report.actions.len(), 1);
    assert_eq!(report.total_trades, 1);
    assert_eq!(report.success_count, 0);
    assert_eq!(report.failed_count, 0);
    assert_eq!(report.skipped_count, 0);
}

#[test]
fn test_execution_report_mark_success() {
    let actions = vec![TradingAction::Hold];
    let mut report = ExecutionReport::new(actions, AlgorithmType::Portfolio);

    report.mark_success();
    assert_eq!(report.success_count, 1);
}

#[test]
fn test_execution_report_mark_failed() {
    let actions = vec![];
    let mut report = ExecutionReport::new(actions, AlgorithmType::Momentum);

    report.mark_failed();
    assert_eq!(report.failed_count, 1);
}

#[test]
fn test_execution_report_mark_skipped() {
    let actions = vec![];
    let mut report = ExecutionReport::new(actions, AlgorithmType::Portfolio);

    report.mark_skipped();
    assert_eq!(report.skipped_count, 1);
}

#[test]
fn test_execution_report_multiple_marks() {
    let actions = vec![TradingAction::Hold, TradingAction::Hold];
    let mut report = ExecutionReport::new(actions, AlgorithmType::Momentum);

    report.mark_success();
    report.mark_success();
    report.mark_failed();
    report.mark_skipped();

    assert_eq!(report.success_count, 2);
    assert_eq!(report.failed_count, 1);
    assert_eq!(report.skipped_count, 1);
}

// ==================== PredictionData のテスト ====================

#[test]
fn test_prediction_data_creation() {
    let token: TokenOutAccount = "test.tkn.near".parse().unwrap();
    let prediction = PredictionData {
        token: token.clone(),
        current_price: TokenPrice::from_near_per_token(BigDecimal::from_str("100.0").unwrap()),
        predicted_price_24h: TokenPrice::from_near_per_token(
            BigDecimal::from_str("120.0").unwrap(),
        ),
        timestamp: Utc::now(),
        confidence: Some(BigDecimal::from_str("0.85").unwrap()),
    };

    assert_eq!(prediction.token, token);
    assert!(prediction.confidence.is_some());
}

#[test]
fn test_prediction_data_without_confidence() {
    let prediction = PredictionData {
        token: "test.tkn.near".parse().unwrap(),
        current_price: TokenPrice::from_near_per_token(BigDecimal::from_str("100.0").unwrap()),
        predicted_price_24h: TokenPrice::from_near_per_token(
            BigDecimal::from_str("120.0").unwrap(),
        ),
        timestamp: Utc::now(),
        confidence: None,
    };

    assert!(prediction.confidence.is_none());
}

// ==================== TradeType のテスト ====================

#[test]
fn test_trade_type_equality() {
    assert_eq!(TradeType::Buy, TradeType::Buy);
    assert_ne!(TradeType::Buy, TradeType::Sell);
}

#[test]
fn test_trade_type_clone() {
    let trade_type = TradeType::Swap;
    let cloned = trade_type.clone();
    assert_eq!(trade_type, cloned);
}

// ==================== TrendDirection のテスト ====================

#[test]
fn test_trend_direction_equality() {
    assert_eq!(TrendDirection::Upward, TrendDirection::Upward);
    assert_ne!(TrendDirection::Upward, TrendDirection::Downward);
}

#[test]
fn test_trend_direction_clone() {
    let direction = TrendDirection::Sideways;
    let cloned = direction.clone();
    assert_eq!(direction, cloned);
}

// ==================== TrendStrength のテスト ====================

#[test]
fn test_trend_strength_values() {
    let strengths = [
        TrendStrength::Strong,
        TrendStrength::Moderate,
        TrendStrength::Weak,
        TrendStrength::NoTrend,
    ];

    assert_eq!(strengths.len(), 4);
}

// ==================== PerformanceMetrics のテスト ====================

#[test]
fn test_performance_metrics_creation() {
    let metrics = PerformanceMetrics {
        total_return: 0.15,
        sharpe_ratio: 1.5,
        max_drawdown: 0.1,
        win_rate: 0.6,
        total_trades: 100,
        annualized_return: Some(0.25),
        volatility: Some(0.2),
        sortino_ratio: Some(2.0),
        calmar_ratio: Some(1.5),
    };

    assert_eq!(metrics.total_return, 0.15);
    assert_eq!(metrics.total_trades, 100);
    assert!(metrics.annualized_return.is_some());
}

// ==================== TokenData のテスト ====================

#[test]
fn test_token_data_creation() {
    let token = TokenData {
        symbol: "near".parse().unwrap(),
        current_rate: ExchangeRate::from_raw_rate(
            BigDecimal::from_str("1000000000000000000000000").unwrap(),
            24,
        ),
        historical_volatility: 0.3,
        liquidity_score: Some(0.8),
        market_cap: Some(NearValue::from_near(BigDecimal::from(1000000))),
    };

    assert_eq!(token.symbol.to_string(), "near");
    assert_eq!(token.current_rate.decimals(), 24);
}

// ==================== シリアライゼーションのテスト ====================

#[test]
fn test_trading_action_serialization() {
    let action = TradingAction::Hold;
    let serialized = serde_json::to_string(&action).unwrap();
    let deserialized: TradingAction = serde_json::from_str(&serialized).unwrap();
    assert_eq!(action, deserialized);
}

#[test]
fn test_prediction_data_serialization() {
    let prediction = PredictionData {
        token: "test".parse().unwrap(),
        current_price: TokenPrice::from_near_per_token(BigDecimal::from_str("100.0").unwrap()),
        predicted_price_24h: TokenPrice::from_near_per_token(
            BigDecimal::from_str("120.0").unwrap(),
        ),
        timestamp: Utc::now(),
        confidence: Some(BigDecimal::from_str("0.9").unwrap()),
    };

    let serialized = serde_json::to_string(&prediction).unwrap();
    let deserialized: PredictionData = serde_json::from_str(&serialized).unwrap();
    assert_eq!(prediction.token, deserialized.token);
}

// ==================== PricePoint のテスト ====================

#[test]
fn test_price_point_creation() {
    let price_point = PricePoint {
        timestamp: Utc::now(),
        price: TokenPrice::from_near_per_token(BigDecimal::from_str("123.456").unwrap()),
        volume: Some(BigDecimal::from(1000)),
    };

    assert_eq!(
        price_point.price,
        TokenPrice::from_near_per_token(BigDecimal::from_str("123.456").unwrap())
    );
    assert!(price_point.volume.is_some());
}

#[test]
fn test_price_point_serialization() {
    let price_point = PricePoint {
        timestamp: Utc::now(),
        price: TokenPrice::from_near_per_token(BigDecimal::from_str("999.123456789").unwrap()),
        volume: Some(BigDecimal::from(5000)),
    };

    let serialized = serde_json::to_string(&price_point).unwrap();
    let deserialized: PricePoint = serde_json::from_str(&serialized).unwrap();

    assert_eq!(price_point.price, deserialized.price);
    assert_eq!(price_point.volume, deserialized.volume);
}

#[test]
fn test_price_point_serialization_without_volume() {
    let price_point = PricePoint {
        timestamp: Utc::now(),
        price: TokenPrice::from_near_per_token(BigDecimal::from(100)),
        volume: None,
    };

    let serialized = serde_json::to_string(&price_point).unwrap();
    let deserialized: PricePoint = serde_json::from_str(&serialized).unwrap();

    assert_eq!(price_point.price, deserialized.price);
    assert!(deserialized.volume.is_none());
}

// ==================== TokenData のシリアライゼーションテスト ====================

#[test]
fn test_token_data_serialization() {
    let token = TokenData {
        symbol: "near".parse().unwrap(),
        current_rate: ExchangeRate::from_raw_rate(
            BigDecimal::from_str("1000000000000000000000000").unwrap(),
            24,
        ),
        historical_volatility: 0.3,
        liquidity_score: Some(0.8),
        market_cap: Some(NearValue::from_near(BigDecimal::from(1000000))),
    };

    let serialized = serde_json::to_string(&token).unwrap();
    let deserialized: TokenData = serde_json::from_str(&serialized).unwrap();

    assert_eq!(token.symbol, deserialized.symbol);
    assert_eq!(token.current_rate, deserialized.current_rate);
    assert_eq!(
        token.historical_volatility,
        deserialized.historical_volatility
    );
}

// ==================== PriceHistory のテスト ====================

#[test]
fn test_price_history_serialization() {
    let history = PriceHistory {
        token: "test.token".parse().unwrap(),
        quote_token: "wrap.near".parse().unwrap(),
        prices: vec![
            PricePoint {
                timestamp: Utc::now(),
                price: TokenPrice::from_near_per_token(BigDecimal::from(100)),
                volume: Some(BigDecimal::from(1000)),
            },
            PricePoint {
                timestamp: Utc::now(),
                price: TokenPrice::from_near_per_token(BigDecimal::from(110)),
                volume: Some(BigDecimal::from(2000)),
            },
        ],
    };

    let serialized = serde_json::to_string(&history).unwrap();
    let deserialized: PriceHistory = serde_json::from_str(&serialized).unwrap();

    assert_eq!(history.token, deserialized.token);
    assert_eq!(history.prices.len(), deserialized.prices.len());
    assert_eq!(history.prices[0].price, deserialized.prices[0].price);
}

// ==================== TokenPrice 型のJSON形式テスト ====================

#[test]
fn test_token_price_json_format() {
    // TokenPrice 型が正しくJSONにシリアライズされることを確認
    let price = TokenPrice::from_near_per_token(BigDecimal::from_str("123.456789").unwrap());
    let json = serde_json::to_string(&price).unwrap();

    // BigDecimal単体のシリアライズ形式と比較
    let bd = BigDecimal::from_str("123.456789").unwrap();
    let bd_json = serde_json::to_string(&bd).unwrap();

    // TokenPrice と BigDecimal は同じJSON形式でシリアライズされることを確認
    assert_eq!(json, bd_json);

    // デシリアライズの往復確認
    let deserialized: TokenPrice = serde_json::from_str(&json).unwrap();
    assert_eq!(price, deserialized);
}

#[test]
fn test_exchange_rate_comparison_in_structures() {
    // ExchangeRate 型を含む構造体の比較が正しく動作することを確認
    let token1 = TokenData {
        symbol: "test.near".parse().unwrap(),
        current_rate: ExchangeRate::from_raw_rate(BigDecimal::from(100), 6),
        historical_volatility: 0.2,
        liquidity_score: None,
        market_cap: None,
    };

    let token2 = TokenData {
        symbol: "test.near".parse().unwrap(),
        current_rate: ExchangeRate::from_raw_rate(BigDecimal::from(100), 6),
        historical_volatility: 0.2,
        liquidity_score: None,
        market_cap: None,
    };

    let token3 = TokenData {
        symbol: "test.near".parse().unwrap(),
        current_rate: ExchangeRate::from_raw_rate(BigDecimal::from(200), 6), // 異なるレート
        historical_volatility: 0.2,
        liquidity_score: None,
        market_cap: None,
    };

    assert_eq!(token1.current_rate, token2.current_rate);
    assert_ne!(token1.current_rate, token3.current_rate);
}
