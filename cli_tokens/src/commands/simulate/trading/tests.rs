use super::*;
use bigdecimal::BigDecimal;
use chrono::Utc;
use common::algorithm::{PredictionData, TradingAction};
use common::types::Price;
use std::str::FromStr;

fn price(s: &str) -> Price {
    Price::new(BigDecimal::from_str(s).unwrap())
}

// Note: trading.rsの関数は外部API(Chronos)への依存が多いため、
// ここではロジックとデータ構造のユニットテストのみを実施します。
// 実際のAPI呼び出しテストは統合テストで実施する必要があります。

#[test]
fn test_trading_action_execution_structure() {
    // TradingActionの構造テスト
    let hold = TradingAction::Hold;
    match hold {
        TradingAction::Hold => {} // Success
        _ => panic!("Expected Hold action"),
    }

    let sell = TradingAction::Sell {
        token: "token1.near".to_string(),
        target: "token2.near".to_string(),
    };
    match sell {
        TradingAction::Sell { token, target } => {
            assert_eq!(token, "token1.near");
            assert_eq!(target, "token2.near");
        }
        _ => panic!("Expected Sell action"),
    }
}

#[test]
fn test_prediction_data_structure() {
    let prediction = PredictionData {
        token: "test.tkn.near".to_string(),
        current_price: price("1.5"),
        predicted_price_24h: price("1.8"),
        timestamp: Utc::now(),
        confidence: Some(BigDecimal::from_str("0.85").unwrap()),
    };

    assert_eq!(prediction.token, "test.tkn.near");
    assert_eq!(prediction.current_price, price("1.5"));
    assert_eq!(prediction.predicted_price_24h, price("1.8"));
    assert!(prediction.confidence.is_some());
}

#[test]
fn test_prediction_data_expected_return_calculation() {
    // 期待リターンの計算ロジックテスト
    let current_price = BigDecimal::from_str("1.0").unwrap();
    let predicted_price = BigDecimal::from_str("1.2").unwrap();

    // expected_return = (predicted_price - current_price) / current_price
    let expected_return = (&predicted_price - &current_price) / &current_price;

    assert_eq!(expected_return, BigDecimal::from_str("0.2").unwrap());
}

#[test]
fn test_prediction_data_negative_return() {
    // 負のリターンケース
    let current_price = BigDecimal::from_str("1.2").unwrap();
    let predicted_price = BigDecimal::from_str("1.0").unwrap();

    let expected_return = (&predicted_price - &current_price) / &current_price;

    assert!(expected_return < BigDecimal::from(0));
}

#[test]
fn test_cache_params_structure() {
    let hist_start = Utc::now();
    let hist_end = Utc::now();
    let pred_start = Utc::now();
    let pred_end = Utc::now();

    let cache_params = PredictionCacheParams {
        model_name: "chronos_default",
        quote_token: "wrap.near",
        base_token: "test.tkn.near",
        hist_start,
        hist_end,
        pred_start,
        pred_end,
    };

    assert_eq!(cache_params.model_name, "chronos_default");
    assert_eq!(cache_params.quote_token, "wrap.near");
    assert_eq!(cache_params.base_token, "test.tkn.near");
}

#[test]
fn test_bigdecimal_price_calculations() {
    // BigDecimalを使用した価格計算のテスト
    let price1 = BigDecimal::from_str("1.234567890123456789").unwrap();
    let price2 = BigDecimal::from_str("2.345678901234567890").unwrap();

    let sum = &price1 + &price2;
    let diff = &price2 - &price1;
    let product = &price1 * &price2;
    let quotient = &price2 / &price1;

    assert!(sum > price1);
    assert!(sum > price2);
    assert!(diff > BigDecimal::from(0));
    assert!(product > BigDecimal::from(0));
    assert!(quotient > BigDecimal::from(1));
}

#[test]
fn test_trading_cost_calculation_zero_value() {
    use crate::commands::simulate::types::FeeModel;

    // ゼロ値でのコスト計算
    let value = BigDecimal::from(0);
    let fee_model = FeeModel::Realistic;
    let slippage_rate = BigDecimal::from_str("0.001").unwrap(); // 0.1%
    let gas_cost = BigDecimal::from(0);

    let cost =
        calculate_trading_cost_by_value_yocto_bd(&value, &fee_model, &slippage_rate, &gas_cost);

    assert_eq!(cost, BigDecimal::from(0));
}

#[test]
fn test_trading_cost_calculation_positive_value() {
    use crate::commands::simulate::types::FeeModel;

    // 正の値でのコスト計算
    let value = BigDecimal::from(1000);
    let fee_model = FeeModel::Realistic; // 0.3%
    let slippage_rate = BigDecimal::from_str("0.001").unwrap(); // 0.1%
    let gas_cost = BigDecimal::from(10);

    let cost =
        calculate_trading_cost_by_value_yocto_bd(&value, &fee_model, &slippage_rate, &gas_cost);

    // コストは protocol_fee + slippage + gas
    // protocol_fee = 1000 * 0.003 = 3
    // slippage = 1000 * 0.001 = 1
    // gas = 10
    // total = 約14 (精度の問題により正確な値は異なる)
    assert!(cost > BigDecimal::from(13));
    assert!(cost < BigDecimal::from(15));
}

#[test]
fn test_trading_cost_calculation_zero_fee_model() {
    use crate::commands::simulate::types::FeeModel;

    // ゼロ手数料モデルでのテスト
    let value = BigDecimal::from(1000);
    let fee_model = FeeModel::Zero;
    let slippage_rate = BigDecimal::from_str("0.001").unwrap();
    let gas_cost = BigDecimal::from(10);

    let cost =
        calculate_trading_cost_by_value_yocto_bd(&value, &fee_model, &slippage_rate, &gas_cost);

    // protocol_fee = 0 (Zero model)
    // slippage = 1000 * 0.001 = 1
    // gas = 10
    // total = 11
    let expected_cost = BigDecimal::from(11);
    assert_eq!(cost, expected_cost);
}

#[test]
fn test_trading_cost_calculation_custom_fee_model() {
    use crate::commands::simulate::types::FeeModel;

    // カスタム手数料モデルでのテスト
    let value = BigDecimal::from(1000);
    let fee_model = FeeModel::Custom(0.005); // 0.5%
    let slippage_rate = BigDecimal::from_str("0.001").unwrap();
    let gas_cost = BigDecimal::from(10);

    let cost =
        calculate_trading_cost_by_value_yocto_bd(&value, &fee_model, &slippage_rate, &gas_cost);

    // protocol_fee = 1000 * 0.005 = 5
    // slippage = 1000 * 0.001 = 1
    // gas = 10
    // total = 約16 (精度の問題により正確な値は異なる)
    assert!(cost > BigDecimal::from(15));
    assert!(cost < BigDecimal::from(17));
}

// 統合テスト用のノート
// Note: 以下の関数は外部API依存のため、ここではテストしません：
// - generate_api_predictions() - Chronos API呼び出しが必要
// - try_load_from_cache() - ファイルシステム操作が必要
// - save_to_cache() - ファイルシステム操作が必要
//
// これらは統合テストまたはモックを使用したテストで検証します。
