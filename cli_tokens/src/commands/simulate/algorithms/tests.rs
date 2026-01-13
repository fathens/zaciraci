use super::*;
use std::str::FromStr;

/// ポートフォリオ価値計算（型安全版）
///
/// 型安全な計算: TokenAmountF64 × TokenPriceF64 = YoctoValueF64 → NearValueF64
#[allow(dead_code)]
fn calculate_portfolio_value_typed(
    holdings: &HashMap<String, TokenAmountF64>,
    prices: &HashMap<String, TokenPriceF64>,
) -> NearValueF64 {
    let mut total_value = NearValueF64::zero();
    for (token, &amount) in holdings {
        if let Some(&price) = prices.get(token) {
            // TokenAmountF64 × TokenPriceF64 = YoctoValueF64
            let value_yocto = amount * price;
            // YoctoValueF64 → NearValueF64
            total_value = total_value + value_yocto.to_near();
        }
    }
    total_value
}

/// レガシー: BigDecimal精度テスト用（スケーリングされた価格形式）
///
/// 注: prices は yoctoNEAR/token 形式で保存されているため 1e24 で除算
fn calculate_portfolio_value_precise(
    holdings: &HashMap<String, f64>,
    prices: &HashMap<String, f64>,
) -> BigDecimal {
    let mut total_value_bd = BigDecimal::from(0);
    let scale_factor = BigDecimal::from_str("1000000000000000000000000").unwrap(); // 1e24
    for (token, amount) in holdings {
        if let Some(&price_scaled) = prices.get(token) {
            let price_scaled_bd =
                BigDecimal::from_str(&price_scaled.to_string()).unwrap_or_default();
            let price_normalized_bd = &price_scaled_bd / &scale_factor;
            let amount_bd = BigDecimal::from_str(&amount.to_string()).unwrap_or_default();
            let value_bd = &price_normalized_bd * &amount_bd;
            total_value_bd += value_bd;
        }
    }
    total_value_bd
}

/// レガシー: f64精度テスト用（スケーリングされた価格形式）
///
/// 注: prices は yoctoNEAR/token 形式で保存されているため 1e24 で除算
fn calculate_portfolio_value_f64(
    holdings: &HashMap<String, f64>,
    prices: &HashMap<String, f64>,
) -> f64 {
    let mut total_value = 0.0;
    for (token, amount) in holdings {
        if let Some(&price_scaled) = prices.get(token) {
            let price_normalized = price_scaled / 1e24;
            total_value += amount * price_normalized;
        }
    }
    total_value
}

#[test]
fn test_bean_token_precision_issue() {
    // Bean tokenの実際の値でテスト
    // 注: このテストは精度比較用。prices は yoctoNEAR/token 形式（スケーリング済み）
    let mut holdings = HashMap::new();
    holdings.insert("bean.token".to_string(), 8.478e20);

    let mut prices = HashMap::new();
    prices.insert("bean.token".to_string(), 2.783e-19);

    // f64計算
    let value_f64 = calculate_portfolio_value_f64(&holdings, &prices);

    // BigDecimal計算（高精度）
    let value_bd = calculate_portfolio_value_precise(&holdings, &prices);
    let value_bd_f64 = value_bd.to_string().parse::<f64>().unwrap_or(0.0);

    println!("🔍 Bean Token Precision Test:");
    println!("   f64 calculation: {}", value_f64);
    println!("   BigDecimal calculation: {}", value_bd);
    println!("   BigDecimal as f64: {}", value_bd_f64);

    // 結果の比較（両方とも正確な結果）
    assert!(value_f64 > 0.0, "f64 calculation: {}", value_f64);
    assert!(
        value_bd_f64 > 0.0,
        "BigDecimal calculation: {}",
        value_bd_f64
    );

    // 値が極小であることを確認
    assert!(
        value_f64 < 1e-20,
        "Value should be extremely small: {}",
        value_f64
    );
    assert!(
        value_bd_f64 < 1e-20,
        "BigDecimal value should be extremely small: {}",
        value_bd_f64
    );
}

#[test]
fn test_realistic_portfolio_precision() {
    // より現実的なポートフォリオでテスト
    // 注: このテストは精度比較用。prices は yoctoNEAR/token 形式（スケーリング済み）
    let mut holdings = HashMap::new();
    holdings.insert("usdc.tether-token.near".to_string(), 100.0);
    holdings.insert("bean.token".to_string(), 8.478e20);
    holdings.insert("ndc.tkn.near".to_string(), 5.2e15);

    let mut prices = HashMap::new();
    prices.insert("usdc.tether-token.near".to_string(), 1e24); // 1 NEAR (スケーリング済み)
    prices.insert("bean.token".to_string(), 2.783e-19); // 極小価格
    prices.insert("ndc.tkn.near".to_string(), 1.5e15); // 中程度の価格

    let value_f64 = calculate_portfolio_value_f64(&holdings, &prices);
    let value_bd = calculate_portfolio_value_precise(&holdings, &prices);
    let value_bd_f64 = value_bd.to_string().parse::<f64>().unwrap_or(0.0);

    println!("💼 Realistic Portfolio Test:");
    println!("   f64 total: {}", value_f64);
    println!("   BigDecimal total: {}", value_bd);
    println!("   BigDecimal as f64: {}", value_bd_f64);

    // 値が正であることを確認
    assert!(value_f64 > 0.0, "f64 value should be positive");
    assert!(value_bd_f64 > 0.0, "BigDecimal value should be positive");
}

#[test]
fn test_quantity_limit_application() {
    use bigdecimal::BigDecimal;
    use std::str::FromStr;

    // リバランス計算での数量制限テスト
    let portfolio_value = BigDecimal::from_str("16201.58").unwrap(); // 16201.58 NEAR
    let target_weight = BigDecimal::from_str("0.5").unwrap(); // 50%配分
    let price_yocto = BigDecimal::from_str("2.783e-19").unwrap(); // Bean token価格（yocto）
    let yocto_per_near = BigDecimal::from_str("1000000000000000000000000").unwrap(); // 10^24

    let target_value = &portfolio_value * &target_weight; // 8100.79 NEAR
    let price_near = &price_yocto / &yocto_per_near; // 2.783e-43 NEAR
    let target_amount_unlimited = &target_value / &price_near; // 異常に大きな数

    // 制限前の数量
    println!("🧪 Quantity Limit Test:");
    println!("   Portfolio Value: {} NEAR", portfolio_value);
    println!("   Target Weight: 50%");
    println!("   Price (yocto): {}", price_yocto);
    println!("   Price (NEAR): {}", price_near);
    println!("   Target Value: {} NEAR", target_value);
    println!("   Unlimited Amount: {}", target_amount_unlimited);

    // 制限適用
    let max_reasonable_amount = BigDecimal::from_str("1000000000000000000000").unwrap(); // 10^21
    let target_amount_limited = if target_amount_unlimited > max_reasonable_amount {
        max_reasonable_amount.clone()
    } else {
        target_amount_unlimited.clone()
    };

    println!("   Limited Amount: {}", target_amount_limited);

    // 制限が適用されることを確認
    assert!(
        target_amount_unlimited > max_reasonable_amount,
        "Unlimited amount should exceed limit"
    );
    assert_eq!(
        target_amount_limited, max_reasonable_amount,
        "Limited amount should equal max limit"
    );

    // 制限値は現実的な範囲内であることを確認
    let limited_f64 = target_amount_limited.to_string().parse::<f64>().unwrap();
    assert!(limited_f64 < 1e22, "Limited amount should be reasonable");
}

#[test]
fn test_rebalance_quantity_accumulation_prevention() {
    use bigdecimal::BigDecimal;
    use std::str::FromStr;

    // 1887%問題を再現するシナリオをテスト
    let mut current_holdings = HashMap::new();
    current_holdings.insert("bean.token".to_string(), 4.267e20); // 初期保有量

    let mut current_prices = HashMap::new();
    current_prices.insert("bean.token".to_string(), 2.783e-19); // Bean token価格（yocto）

    // 元のリバランス計算（制限なし）- 再現のみ
    let total_portfolio_value = 16201.58; // NEAR
    let target_weight = 0.5; // 50%配分
    let current_price_near = 2.783e-19 / 1e24; // NEAR単位価格（極小）
    let target_value_old = total_portfolio_value * target_weight;
    let target_amount_old = target_value_old / current_price_near; // 異常に大きな数

    // 新しい修正されたリバランス計算（制限あり）
    let total_portfolio_value_bd = BigDecimal::from_str("16201.58").unwrap();
    let price_yocto_bd = BigDecimal::from_str("2.783e-19").unwrap();
    let yocto_per_near = BigDecimal::from_str("1000000000000000000000000").unwrap();
    let price_near_bd = &price_yocto_bd / &yocto_per_near;

    let target_weight_bd = BigDecimal::from_str("0.5").unwrap();
    let target_value_bd = &total_portfolio_value_bd * &target_weight_bd;
    let target_amount_bd = &target_value_bd / &price_near_bd;

    // 制限適用
    let max_reasonable_amount = BigDecimal::from_str("1000000000000000000000").unwrap(); // 10^21
    let target_amount_limited = if target_amount_bd > max_reasonable_amount {
        max_reasonable_amount.clone()
    } else {
        target_amount_bd.clone()
    };

    let target_amount_new = target_amount_limited
        .to_string()
        .parse::<f64>()
        .unwrap_or(0.0);

    println!("🧪 Rebalance Calculation Test:");
    println!("   Total Portfolio Value: {} NEAR", total_portfolio_value);
    println!("   Target Weight: 50%");
    println!(
        "   Bean Token Price: {} yoctoNEAR",
        current_prices.get("bean.token").unwrap()
    );
    println!("   Bean Token Price (NEAR): {:.2e}", current_price_near);
    println!(
        "   Old Target Amount (unrestricted): {:.2e}",
        target_amount_old
    );
    println!(
        "   New Target Amount (restricted): {:.2e}",
        target_amount_new
    );

    // 修正効果の検証
    assert!(
        target_amount_old > 1e40,
        "Old calculation should produce extremely large amounts"
    );
    assert!(
        target_amount_new < 1e22,
        "New calculation should be within reasonable limits"
    );

    // 制限前後の数量比較
    let reduction_factor = target_amount_old / target_amount_new;
    println!("   Reduction Factor: {:.2e}", reduction_factor);
    assert!(reduction_factor > 1e20, "Should be significant reduction");

    // 現在保有量との比較
    let current_amount = current_holdings.get("bean.token").unwrap();
    let diff_old = target_amount_old - current_amount;
    let diff_new = target_amount_new - current_amount;

    println!("   Current Holding: {:.2e}", current_amount);
    println!("   Old Diff: {:.2e}", diff_old);
    println!("   New Diff: {:.2e}", diff_new);

    // 差分も制限内であることを確認
    assert!(
        diff_new.abs() < 1e22,
        "Difference should be within reasonable limits"
    );
}

#[test]
fn test_portfolio_value_calculation_consistency() {
    // Bean token + 通常tokenの混合ポートフォリオでの一貫性テスト
    let mut holdings = HashMap::new();
    holdings.insert("bean.token".to_string(), 8.478e20); // Bean token（極大量）
    holdings.insert("normal.token".to_string(), 1000.0); // 通常token

    let mut prices = HashMap::new();
    prices.insert("bean.token".to_string(), 2.783e-19); // Bean token（極小価格）
    prices.insert("normal.token".to_string(), 1e24); // 通常token価格（1 NEAR）

    // BigDecimalでの高精度計算
    let total_bd = calculate_portfolio_value_precise(&holdings, &prices);
    let total_bd_f64 = total_bd.to_string().parse::<f64>().unwrap_or(0.0);

    // f64での従来計算
    let total_f64 = calculate_portfolio_value_f64(&holdings, &prices);

    println!("🧪 Portfolio Value Consistency Test:");
    println!(
        "   Holdings: Bean={:.2e}, Normal={}",
        holdings.get("bean.token").unwrap(),
        holdings.get("normal.token").unwrap()
    );
    println!("   BigDecimal Total: {} NEAR", total_bd);
    println!("   BigDecimal as f64: {:.6} NEAR", total_bd_f64);
    println!("   f64 Total: {:.6} NEAR", total_f64);

    // Bean tokenの寄与は極小で、主に通常tokenが価値を決定
    assert!(
        (999.0..=1001.0).contains(&total_bd_f64),
        "Total should be close to 1000 NEAR"
    );
    assert!(
        (999.0..=1001.0).contains(&total_f64),
        "f64 calculation should also be close to 1000 NEAR"
    );

    // 精度の違いは微小
    let precision_diff = (total_bd_f64 - total_f64).abs();
    assert!(
        precision_diff < 1e-10,
        "Precision difference should be minimal for this case"
    );
}

#[test]
fn test_extreme_value_handling() {
    // 極端な値での処理テスト
    let extreme_scenarios = vec![
        ("Very small price", 1e-25, 1e20),  // 極小価格、大量
        ("Very large amount", 1e-19, 1e25), // 極大量
        ("Both extreme", 1e-30, 1e30),      // 両方極端
    ];

    for (scenario, price, amount) in extreme_scenarios {
        let mut holdings = HashMap::new();
        holdings.insert("test.token".to_string(), amount);

        let mut prices = HashMap::new();
        prices.insert("test.token".to_string(), price);

        let value_bd = calculate_portfolio_value_precise(&holdings, &prices);
        let value_f64 = value_bd.to_string().parse::<f64>().unwrap_or(0.0);

        println!("🧪 Extreme Value Test - {}:", scenario);
        println!("   Price: {:.2e} yoctoNEAR", price);
        println!("   Amount: {:.2e} tokens", amount);
        println!("   Value: {} NEAR", value_bd);
        println!("   Value (f64): {:.6e} NEAR", value_f64);

        // 値が有限で非負であることを確認
        assert!(value_f64.is_finite(), "Value should be finite");
        assert!(value_f64 >= 0.0, "Value should be non-negative");

        // 極端すぎる値は適切に処理される
        if price * amount < 1e-20 {
            assert!(
                value_f64 < 1e-15,
                "Very small values should remain very small"
            );
        }
    }
}
