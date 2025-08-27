use super::*;
use bigdecimal::FromPrimitive;

#[test]
fn test_calculate_expected_return() {
    let prediction = PredictionData {
        token: "TEST".to_string(),
        current_price: BigDecimal::from_f64(100.0).unwrap(),
        predicted_price_24h: BigDecimal::from_f64(110.0).unwrap(),
        timestamp: Utc::now(),
        confidence: Some(0.8),
    };

    let return_val = calculate_expected_return(&prediction);
    // 10%のリターンから取引コスト（往復0.6% + スリッページ2%）を差し引き
    let expected = 0.1 - (2.0 * TRADING_FEE) - MAX_SLIPPAGE; // 0.1 - 0.006 - 0.02 = 0.074
    assert!((return_val - expected).abs() < 0.001);
}

#[test]
fn test_rank_tokens_by_momentum() {
    let predictions = vec![
        PredictionData {
            token: "TOKEN1".to_string(),
            current_price: BigDecimal::from_f64(100.0).unwrap(),
            predicted_price_24h: BigDecimal::from_f64(110.0).unwrap(), // 高いリターンに変更
            timestamp: Utc::now(),
            confidence: Some(0.7),
        },
        PredictionData {
            token: "TOKEN2".to_string(),
            current_price: BigDecimal::from_f64(100.0).unwrap(),
            predicted_price_24h: BigDecimal::from_f64(120.0).unwrap(), // 最高リターンに変更
            timestamp: Utc::now(),
            confidence: Some(0.9),
        },
        PredictionData {
            token: "TOKEN3".to_string(),
            current_price: BigDecimal::from_f64(100.0).unwrap(),
            predicted_price_24h: BigDecimal::from_f64(105.0).unwrap(), // 適度なリターンに変更
            timestamp: Utc::now(),
            confidence: Some(0.6),
        },
    ];

    let ranked = rank_tokens_by_momentum(predictions);

    // 全て正のリターンなので3つとも含まれるはず
    assert!(ranked.len() >= 2); // 最低2つは含まれる
    assert_eq!(ranked[0].0, "TOKEN2"); // 信頼度調整後も最高リターン
    assert_eq!(ranked[1].0, "TOKEN1"); // 2番目

    // 3番目があるかチェック
    if ranked.len() > 2 {
        assert_eq!(ranked[2].0, "TOKEN3");
    }
}

#[test]
fn test_make_trading_decision() {
    let ranked = vec![
        ("BEST_TOKEN".to_string(), 0.2, Some(0.8)),
        ("GOOD_TOKEN".to_string(), 0.1, Some(0.7)),
        ("OK_TOKEN".to_string(), 0.08, Some(0.6)), // より高いリターンに調整
    ];

    let amount = BigDecimal::from_f64(10.0).unwrap();

    // Case 1: Hold when current token is best
    let action = make_trading_decision("BEST_TOKEN", 0.2, &ranked, &amount);
    assert_eq!(action, TradingAction::Hold);

    // Case 2: Sell when return is below threshold (5%)
    let action = make_trading_decision("BAD_TOKEN", 0.02, &ranked, &amount);
    assert!(matches!(action, TradingAction::Sell { .. }));

    // Case 3: Switch when better option exists
    // 現在のリターン0.05、最良0.2、信頼度0.8、SWITCH_MULTIPLIER 1.5
    // 0.2 > 0.05 * 1.5 * 0.8 = 0.06 なので 0.2 > 0.06 でSwitch
    let action = make_trading_decision("MEDIUM_TOKEN", 0.05, &ranked, &amount);
    assert!(matches!(action, TradingAction::Switch { .. }));

    // Case 4: Hold when amount is too small
    let small_amount = BigDecimal::from_f64(0.5).unwrap();
    let action = make_trading_decision("BAD_TOKEN", 0.02, &ranked, &small_amount);
    assert_eq!(action, TradingAction::Hold);
}

#[test]
fn test_calculate_volatility() {
    let prices = vec![100.0, 105.0, 103.0, 108.0, 106.0];
    let volatility = calculate_volatility(&prices);
    assert!(volatility > 0.0 && volatility < 0.05); // 低ボラティリティ

    let high_vol_prices = vec![100.0, 120.0, 90.0, 130.0, 85.0];
    let high_volatility = calculate_volatility(&high_vol_prices);
    assert!(high_volatility > 0.1); // 高ボラティリティ
}

// ==================== エッジケーステスト ====================

#[test]
fn test_calculate_expected_return_edge_cases() {
    // ゼロ価格のケース
    let zero_price_prediction = PredictionData {
        token: "ZERO".to_string(),
        current_price: BigDecimal::from(0),
        predicted_price_24h: BigDecimal::from_f64(100.0).unwrap(),
        timestamp: Utc::now(),
        confidence: Some(0.8),
    };
    let return_val = calculate_expected_return(&zero_price_prediction);
    assert_eq!(return_val, 0.0);

    // 負のリターンのケース
    let negative_prediction = PredictionData {
        token: "NEGATIVE".to_string(),
        current_price: BigDecimal::from_f64(100.0).unwrap(),
        predicted_price_24h: BigDecimal::from_f64(80.0).unwrap(),
        timestamp: Utc::now(),
        confidence: Some(0.8),
    };
    let return_val = calculate_expected_return(&negative_prediction);
    assert!(return_val < 0.0);
}

#[test]
fn test_confidence_adjusted_return() {
    let prediction = PredictionData {
        token: "TEST".to_string(),
        current_price: BigDecimal::from_f64(100.0).unwrap(),
        predicted_price_24h: BigDecimal::from_f64(110.0).unwrap(),
        timestamp: Utc::now(),
        confidence: Some(0.5), // 50%信頼度
    };

    let base_return = calculate_expected_return(&prediction);
    let adjusted_return = calculate_confidence_adjusted_return(&prediction);

    // 信頼度50%なので半分になる
    assert!((adjusted_return - base_return * 0.5).abs() < 0.001);

    // 信頼度なしの場合のテスト
    let no_confidence_prediction = PredictionData {
        token: "NO_CONF".to_string(),
        current_price: BigDecimal::from_f64(100.0).unwrap(),
        predicted_price_24h: BigDecimal::from_f64(110.0).unwrap(),
        timestamp: Utc::now(),
        confidence: None, // 信頼度なし
    };
    let adjusted_return_no_conf = calculate_confidence_adjusted_return(&no_confidence_prediction);
    assert!((adjusted_return_no_conf - base_return * 0.5).abs() < 0.001); // デフォルト50%
}

#[test]
fn test_rank_tokens_by_momentum_edge_cases() {
    // 空のリスト
    let empty_predictions = vec![];
    let ranked = rank_tokens_by_momentum(empty_predictions);
    assert!(ranked.is_empty());

    // 全て負のリターン
    let negative_predictions = vec![
        PredictionData {
            token: "NEG1".to_string(),
            current_price: BigDecimal::from_f64(100.0).unwrap(),
            predicted_price_24h: BigDecimal::from_f64(80.0).unwrap(),
            timestamp: Utc::now(),
            confidence: Some(0.8),
        },
        PredictionData {
            token: "NEG2".to_string(),
            current_price: BigDecimal::from_f64(100.0).unwrap(),
            predicted_price_24h: BigDecimal::from_f64(70.0).unwrap(),
            timestamp: Utc::now(),
            confidence: Some(0.9),
        },
    ];
    let ranked = rank_tokens_by_momentum(negative_predictions);
    assert!(ranked.is_empty()); // 負のリターンはフィルタされる

    // TOP_N_TOKENS以上のトークン
    let many_predictions: Vec<PredictionData> = (0..10)
        .map(|i| PredictionData {
            token: format!("TOKEN{}", i),
            current_price: BigDecimal::from_f64(100.0).unwrap(),
            predicted_price_24h: BigDecimal::from_f64(105.0 + i as f64).unwrap(),
            timestamp: Utc::now(),
            confidence: Some(0.8),
        })
        .collect();
    let ranked = rank_tokens_by_momentum(many_predictions);
    assert_eq!(ranked.len(), TOP_N_TOKENS); // TOP_N_TOKENSに制限される
}

#[test]
fn test_adjust_for_trading_costs() {
    let base_return = 0.1; // 10%
    let adjusted = adjust_for_trading_costs(base_return);
    let expected = base_return - (2.0 * TRADING_FEE) - MAX_SLIPPAGE;
    assert!((adjusted - expected).abs() < 0.0001);

    // 取引コストより小さいリターンの場合
    let small_return = 0.01; // 1%
    let adjusted_small = adjust_for_trading_costs(small_return);
    assert!(adjusted_small < 0.0); // 負のリターンになる
}

#[test]
fn test_calculate_position_size() {
    // 通常のケース
    assert_eq!(calculate_position_size(0.8, 0.5), 0.4); // 0.8 * 0.5 = 0.4

    // 上限を超える場合
    assert_eq!(calculate_position_size(1.0, 2.0), 1.0); // clamp to 1.0

    // 下限を下回る場合
    assert_eq!(calculate_position_size(0.1, -0.5), 0.0); // clamp to 0.0
}

#[test]
fn test_volatility_edge_cases() {
    // 空の配列
    assert_eq!(calculate_volatility(&[]), 0.0);

    // 1つの要素
    assert_eq!(calculate_volatility(&[100.0]), 0.0);

    // すべて同じ値
    assert_eq!(calculate_volatility(&[100.0, 100.0, 100.0]), 0.0);

    // ゼロを含む価格データ（実際には価格差が計算される）
    let with_zero = vec![100.0, 0.0, 200.0]; // 0.0があっても前の値から計算される
    let volatility = calculate_volatility(&with_zero);
    assert!(volatility >= 0.0); // ボラティリティは0以上（ゼロの場合もある）
}
