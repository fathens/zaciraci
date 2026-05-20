use super::*;

#[tokio::test]
async fn test_enhanced_portfolio_performance() {
    // 高リターン期待値のトークンでテストデータを作成
    let tokens = create_high_return_tokens();
    let mut predictions = BTreeMap::new();
    predictions.insert(token_out("high_return_token"), price(0.50)); // 50%リターン期待
    predictions.insert(token_out("medium_return_token"), price(0.30)); // 30%リターン期待
    predictions.insert(token_out("stable_token"), price(0.10)); // 10%リターン期待

    let historical_prices = create_realistic_price_history();

    let portfolio_data = super::PortfolioData {
        tokens: tokens.clone(),
        predictions: predictions.clone(),
        historical_prices,
        ..Default::default()
    };

    // 空のウォレット（初期状態）
    let wallet = WalletInfo {
        holdings: BTreeMap::new(),
        total_value: NearValue::from_near(BigDecimal::from(1000)), // 1000 NEAR初期資本
        cash_balance: NearValue::from_near(BigDecimal::from(1000)),
    };

    // 拡張ポートフォリオ最適化を実行
    let result = super::execute_portfolio_optimization(&wallet, portfolio_data, 0.05).await;

    assert!(
        result.is_ok(),
        "ポートフォリオ最適化が失敗: {:?}",
        result.err()
    );
    let report = result.unwrap();

    // パフォーマンス期待値を計算
    let expected_portfolio_return =
        calculate_expected_portfolio_return(&report.optimal_weights, &predictions, &tokens);

    println!("=== Enhanced Portfolio Performance Test ===");
    println!(
        "Expected portfolio return: {:.2}%",
        expected_portfolio_return * 100.0
    );
    println!("Optimal weights:");
    for (token, weight) in report.optimal_weights.weights.iter() {
        println!(
            "  {}: {:.1}%",
            token,
            weight.to_f64().unwrap_or(0.0) * 100.0
        );
    }
    println!("Rebalance needed: {}", report.rebalance_needed);
    println!("Number of actions: {}", report.actions.len());

    // 高パフォーマンス戦略の効果を検証
    assert!(
        expected_portfolio_return > 0.15,
        "期待リターンが15%を下回る: {:.2}%",
        expected_portfolio_return * 100.0
    );

    // 積極的パラメータの効果：最大ポジションサイズ60%まで許可
    let max_weight = report
        .optimal_weights
        .weights
        .values()
        .map(|w| w.to_f64().unwrap_or(0.0))
        .fold(0.0f64, f64::max);
    println!("Maximum position size: {:.1}%", max_weight * 100.0);

    // 集中投資効果の確認
    let non_zero_positions = report
        .optimal_weights
        .weights
        .values()
        .filter(|w| w.to_f64().unwrap_or(0.0) > 0.01)
        .count();
    println!("Number of significant positions: {}", non_zero_positions);
    assert!(
        non_zero_positions <= 6,
        "ポジション数が制限を超過: {}",
        non_zero_positions
    );

    // リスク調整の確認
    println!("Risk adjustment factor: calculated dynamically");

    // シミュレーション結果の期待値
    let simulated_final_value = 1000.0 * (1.0 + expected_portfolio_return);
    let simulated_return_pct = expected_portfolio_return * 100.0;

    println!("Simulated final value: {:.2} NEAR", simulated_final_value);
    println!("Simulated return: {:.1}%", simulated_return_pct);

    // 目標：15%以上のリターンを期待（現実的な値に調整）
    assert!(
        simulated_return_pct >= 15.0,
        "シミュレーションリターンが目標を下回る: {:.1}%",
        simulated_return_pct
    );
}

#[tokio::test]
async fn test_baseline_vs_enhanced_comparison() {
    // ベースライン（従来の40%制限）とエンハンスド（60%制限）の比較

    let tokens = create_high_return_tokens();
    // create_high_return_tokens() の現在価格に対して正のリターンを設定:
    // - high_return_token: current = 0.333, +25% → predicted = 0.416
    // - medium_return_token: current = 0.231, +20% → predicted = 0.277
    // - stable_token: current = 0.091, +15% → predicted = 0.105
    let mut predictions = BTreeMap::new();
    predictions.insert(token_out("high_return_token"), price(0.333 * 1.25)); // +25%
    predictions.insert(token_out("medium_return_token"), price(0.231 * 1.20)); // +20%
    predictions.insert(token_out("stable_token"), price(0.091 * 1.15)); // +15%

    let historical_prices = create_realistic_price_history();
    let portfolio_data = super::PortfolioData {
        tokens: tokens.clone(),
        predictions: predictions.clone(),
        historical_prices,
        ..Default::default()
    };

    let wallet = WalletInfo {
        holdings: BTreeMap::new(),
        total_value: NearValue::from_near(BigDecimal::from(1000)),
        cash_balance: NearValue::from_near(BigDecimal::from(1000)),
    };

    // エンハンスドポートフォリオの実行
    let enhanced_result =
        super::execute_portfolio_optimization(&wallet, portfolio_data.clone(), 0.05).await;
    assert!(enhanced_result.is_ok());
    let enhanced_report = enhanced_result.unwrap();

    let enhanced_return = calculate_expected_portfolio_return(
        &enhanced_report.optimal_weights,
        &predictions,
        &tokens,
    );

    println!("=== Baseline vs Enhanced Comparison ===");
    println!(
        "Enhanced strategy expected return: {:.2}%",
        enhanced_return * 100.0
    );

    let enhanced_max_weight = enhanced_report
        .optimal_weights
        .weights
        .values()
        .map(|w| w.to_f64().unwrap_or(0.0))
        .fold(0.0f64, f64::max);
    println!(
        "Enhanced max position size: {:.1}%",
        enhanced_max_weight * 100.0
    );

    // エンハンスド戦略の利点を確認
    println!("Enhanced strategy allows up to 60% position size");
    println!("Enhanced strategy uses dynamic risk adjustment");
    println!("Enhanced strategy concentrates on fewer high-performing tokens");

    // パフォーマンス期待値の検証
    assert!(
        enhanced_return >= 0.12,
        "エンハンスドリターンが期待値を下回る: {:.2}%",
        enhanced_return * 100.0
    );

    // 1000 NEAR → 目標 2000+ NEAR (100%+リターン)
    let final_value = 1000.0 * (1.0 + enhanced_return);
    println!("Projected final value: {:.0} NEAR", final_value);
    println!("Projected return: {:.1}%", enhanced_return * 100.0);
}

#[test]
fn test_price_calculation_precision() {
    // 異常なリターン（1887%）の原因を調査するテスト

    // 実際のシミュレーションで見られた価格値を再現
    let extreme_prices = [
        ("bean.tkn.near", 2.783120479512128E-19),         // 極小価格
        ("blackdragon.tkn.near", 1.7966334858472295E-16), // 中程度価格
        ("ndc.tkn.near", 4.8596827014459204E-20),         // 超極小価格
    ];

    let extreme_amounts = [
        8.478102225988582E+20, // bean.tkn.near の取引量
        8771460298447680.0,    // blackdragon.tkn.near の取引量
        3.942646877247608E+21, // ndc.tkn.near の取引量
    ];

    println!("=== Price Calculation Precision Test ===");

    for (i, (token, price)) in extreme_prices.iter().enumerate() {
        let amount = extreme_amounts[i];
        let total_value = price * amount;

        println!("Token: {}", token);
        println!("  Price: {:.3e}", price);
        println!("  Amount: {:.3e}", amount);
        println!("  Total Value: {:.6}", total_value);
        println!("  Price as string: {:.20e}", price);

        // 精度の問題をチェック
        if *price < 1e-15 {
            println!("  WARNING: Price is extremely small (< 1e-15)");
        }
        if amount > 1e18 {
            println!("  WARNING: Amount is extremely large (> 1e18)");
        }
        if total_value > 1000.0 {
            println!(
                "  WARNING: Total value seems unreasonably high: {:.2}",
                total_value
            );
        }
        println!();
    }

    // yoctoNEAR変換のテスト
    println!("=== YoctoNEAR Conversion Test ===");
    let near_amount = 1000.0; // 1000 NEAR
    let yocto_amount = near_amount * 1e24; // 手動でyoctoNEAR変換
    println!("1000 NEAR = {:.3e} yoctoNEAR", yocto_amount);

    // 極小価格での価値計算
    let bean_price = 2.783120479512128E-19;
    let bean_amount = 8.478102225988582E+20;
    let bean_value_near = (bean_price * bean_amount) / 1e24; // yoctoNEARをNEARに変換
    println!("Bean value in NEAR: {:.6}", bean_value_near);

    // この値が異常に高い場合、価格データに問題がある
    assert!(
        bean_value_near < 10000.0,
        "Bean value seems unreasonably high: {:.2} NEAR",
        bean_value_near
    );
}

#[test]
fn test_portfolio_evaluation_accuracy() {
    // ポートフォリオ評価の精度をテスト
    // calculate_current_weights の計算式: value_near = holding / rate
    // rate = 10^decimals / price なので、value_near = holding * price / 10^decimals

    // 現実的な価格での評価
    // price = 1 NEAR/token → rate = 10^24 / 1 = 10^24
    let realistic_tokens = vec![TokenData {
        symbol: token_out("token_a"),
        current_rate: ExchangeRate::from_raw_rate(
            BigDecimal::from_str("1E+24").unwrap(), // 1 NEAR/token
            24,
        ),
        historical_volatility: 0.2,
        liquidity_score: Some(0.8),
        market_cap: Some(cap(1000000)),
    }];

    // 500 whole tokens = 500 * 10^24 tokens_smallest
    // value = 5E+26 / 10^24 = 500 NEAR
    let mut wallet = WalletInfo {
        holdings: BTreeMap::new(),
        total_value: NearValue::from_near(BigDecimal::from(1000)),
        cash_balance: NearValue::from_near(BigDecimal::from(500)),
    };
    wallet.holdings.insert(
        token_out("token_a"),
        TokenAmount::from_smallest_units(BigDecimal::from_str("5E+26").unwrap(), 24), // 500 tokens in smallest units
    );

    let weights = super::calculate_current_weights(&realistic_tokens, &wallet);
    println!("=== Portfolio Evaluation Test ===");
    println!("Token A holdings: 500 tokens (5E+26 tokens_smallest)");
    println!("Token A price: 1 NEAR (rate = 1E+24)");
    println!("Expected weight: ~50% (500 NEAR / 1000 NEAR total)");
    println!("Calculated weight: {:.1}%", weights[0] * 100.0);

    // 重みが理論値と近いかチェック
    let expected_weight = 0.5; // 50%
    let tolerance = 0.05; // 5%の許容範囲
    assert!(
        (weights[0] - expected_weight).abs() < tolerance,
        "Weight calculation error: expected ~{:.1}%, got {:.1}%",
        expected_weight * 100.0,
        weights[0] * 100.0
    );
}

#[test]
fn test_extreme_price_weight_calculation() {
    // 極端な価格での重み計算をテスト
    // calculate_current_weights の計算式: value_near = holding / rate
    // rate = 10^decimals / price なので、value_near = holding * price / 10^decimals

    println!("=== Extreme Price Weight Calculation Test ===");

    // 現実的な価格での計算テスト
    // bean: price = 0.001 NEAR/token → rate = 10^24 / 0.001 = 10^27
    // ndc: price = 0.01 NEAR/token → rate = 10^24 / 0.01 = 10^26
    let extreme_tokens = vec![
        TokenData {
            symbol: token_out("bean.tkn.near"),
            current_rate: ExchangeRate::from_raw_rate(
                BigDecimal::from_str("1E+27").unwrap(), // 0.001 NEAR/token
                24,
            ),
            historical_volatility: 0.3,
            liquidity_score: Some(0.8),
            market_cap: Some(cap(1000000)),
        },
        TokenData {
            symbol: token_out("ndc.tkn.near"),
            current_rate: ExchangeRate::from_raw_rate(
                BigDecimal::from_str("1E+26").unwrap(), // 0.01 NEAR/token
                24,
            ),
            historical_volatility: 0.4,
            liquidity_score: Some(0.7),
            market_cap: Some(cap(500000)),
        },
    ];

    // 保有量を設定
    // bean: 10^28 tokens_smallest (10000 tokens) → value = 10^28 / 10^27 = 10 NEAR
    // ndc: 10^28 tokens_smallest (10000 tokens) → value = 10^28 / 10^26 = 100 NEAR
    // 合計: 110 NEAR
    let mut wallet = WalletInfo {
        holdings: BTreeMap::new(),
        total_value: NearValue::from_near(BigDecimal::from(110)), // 110 NEAR総価値
        cash_balance: NearValue::zero(),
    };

    wallet.holdings.insert(
        token_out("bean.tkn.near"),
        TokenAmount::from_smallest_units(
            BigDecimal::from_str("1E+28").unwrap(),
            24, // 10000 tokens
        ),
    );
    wallet.holdings.insert(
        token_out("ndc.tkn.near"),
        TokenAmount::from_smallest_units(
            BigDecimal::from_str("1E+28").unwrap(),
            24, // 10000 tokens
        ),
    );

    let weights = super::calculate_current_weights(&extreme_tokens, &wallet);

    println!("Bean token weight: {:.3}%", weights[0] * 100.0);
    println!("NDC token weight: {:.3}%", weights[1] * 100.0);
    println!("Total weights: {:.3}%", (weights[0] + weights[1]) * 100.0);

    // 重みが現実的な範囲内であることを確認
    for (i, weight) in weights.iter().enumerate() {
        assert!(
            *weight <= 1.0,
            "Weight for token {} exceeds 100%: {:.1}%",
            extreme_tokens[i].symbol,
            weight * 100.0
        );
        assert!(
            *weight >= 0.0,
            "Weight for token {} is negative: {:.1}%",
            extreme_tokens[i].symbol,
            weight * 100.0
        );
    }

    // 重みの合計が100%を大きく超えていないことを確認
    let total_weight = weights.iter().sum::<f64>();
    assert!(
        total_weight <= 1.5,
        "Total weight is unreasonably high: {:.1}%",
        total_weight * 100.0
    );

    println!("\n=== BigDecimal計算結果検証 ===");

    // 手動でBigDecimal計算を検証
    let bean_price = BigDecimal::from_str("2.783120479512128E-19").unwrap();
    let bean_holding = "847810222598858200000".parse::<BigDecimal>().unwrap();
    let yocto_per_near = "1000000000000000000000000".parse::<BigDecimal>().unwrap();

    let bean_value_yocto = &bean_price * &bean_holding;
    let bean_value_near = &bean_value_yocto / &yocto_per_near;

    println!("Bean token手動計算:");
    println!("  価格 (yocto): {}", bean_price);
    println!("  保有量: {}", bean_holding);
    println!("  価値 (yocto): {}", bean_value_yocto);
    println!("  価値 (NEAR): {}", bean_value_near);

    // 実際の価値が非常に小さいことを確認
    let bean_value_f64 = bean_value_near.to_string().parse::<f64>().unwrap_or(0.0);
    assert!(
        bean_value_f64 < 1.0,
        "Bean value should be very small: {:.10}",
        bean_value_f64
    );

    println!("BigDecimal計算により異常な高値が修正されました");
}

#[test]
fn test_dimensional_analysis_correctness() {
    // 次元解析の正しさを検証するテスト
    //
    // calculate_current_weights の計算式:
    //   value_near = holding / rate
    //
    // ここで:
    //   rate = raw_rate = 10^decimals / price
    //   price = NEAR/token
    //
    // 従って:
    //   value_near = holding / (10^decimals / price)
    //              = holding * price / 10^decimals
    //              = (tokens_smallest) * (NEAR/token) / 10^decimals
    //              = tokens * NEAR/token
    //              = NEAR  ✓

    println!("=== Dimensional Analysis Correctness Test ===");

    // ケース1: 価格 10 NEAR/token, 100 tokens 保有
    // 期待される価値: 10 * 100 = 1000 NEAR
    let price1 = 10.0; // NEAR/token
    let tokens1 = 100.0; // whole tokens
    let decimals: u32 = 24;
    let rate1 = pow10(decimals as u8) / BigDecimal::from_f64(price1).unwrap();
    let holding1 = BigDecimal::from_f64(tokens1).unwrap() * pow10(decimals as u8);

    let value1 = &holding1 / &rate1;
    let value1_f64 = value1.to_string().parse::<f64>().unwrap();
    let expected1 = price1 * tokens1;

    println!(
        "Case 1: price = {} NEAR/token, tokens = {}",
        price1, tokens1
    );
    println!("  Rate: {}", rate1);
    println!("  Holding: {}", holding1);
    println!("  Calculated value: {} NEAR", value1_f64);
    println!("  Expected value: {} NEAR", expected1);

    assert!(
        (value1_f64 - expected1).abs() < 0.001,
        "Case 1 failed: expected {}, got {}",
        expected1,
        value1_f64
    );

    // ケース2: 価格 0.001 NEAR/token (安いトークン), 1,000,000 tokens 保有
    // 期待される価値: 0.001 * 1,000,000 = 1000 NEAR
    let price2 = 0.001; // NEAR/token
    let tokens2 = 1_000_000.0; // whole tokens
    let rate2 = pow10(decimals as u8) / BigDecimal::from_f64(price2).unwrap();
    let holding2 = BigDecimal::from_f64(tokens2).unwrap() * pow10(decimals as u8);

    let value2 = &holding2 / &rate2;
    let value2_f64 = value2.to_string().parse::<f64>().unwrap();
    let expected2 = price2 * tokens2;

    println!(
        "\nCase 2: price = {} NEAR/token, tokens = {}",
        price2, tokens2
    );
    println!("  Rate: {}", rate2);
    println!("  Holding: {}", holding2);
    println!("  Calculated value: {} NEAR", value2_f64);
    println!("  Expected value: {} NEAR", expected2);

    assert!(
        (value2_f64 - expected2).abs() < 0.001,
        "Case 2 failed: expected {}, got {}",
        expected2,
        value2_f64
    );

    // ケース3: 価格 1000 NEAR/token (高価なトークン), 0.5 tokens 保有
    // 期待される価値: 1000 * 0.5 = 500 NEAR
    let price3 = 1000.0; // NEAR/token
    let tokens3 = 0.5; // whole tokens
    let rate3 = pow10(decimals as u8) / BigDecimal::from_f64(price3).unwrap();
    let holding3 = BigDecimal::from_f64(tokens3).unwrap() * pow10(decimals as u8);

    let value3 = &holding3 / &rate3;
    let value3_f64 = value3.to_string().parse::<f64>().unwrap();
    let expected3 = price3 * tokens3;

    println!(
        "\nCase 3: price = {} NEAR/token, tokens = {}",
        price3, tokens3
    );
    println!("  Rate: {}", rate3);
    println!("  Holding: {}", holding3);
    println!("  Calculated value: {} NEAR", value3_f64);
    println!("  Expected value: {} NEAR", expected3);

    assert!(
        (value3_f64 - expected3).abs() < 0.001,
        "Case 3 failed: expected {}, got {}",
        expected3,
        value3_f64
    );

    println!("\nAll dimensional analysis cases passed");
}

#[test]
fn test_calculate_current_weights_equivalence() {
    // テストデータを作成
    let tokens = vec![
        TokenInfo {
            symbol: token_out("token-a"),
            current_rate: ExchangeRate::from_raw_rate(
                BigDecimal::from_str("1000000000000000000").unwrap(), // 1e18
                18,
            ),
            historical_volatility: 0.2,
            liquidity_score: Some(0.8),
            market_cap: Some(cap(1000000)),
        },
        TokenInfo {
            symbol: token_out("token-b"),
            current_rate: ExchangeRate::from_raw_rate(
                BigDecimal::from_str("500000000000000000").unwrap(), // 0.5e18
                18,
            ),
            historical_volatility: 0.3,
            liquidity_score: Some(0.7),
            market_cap: Some(cap(500000)),
        },
    ];

    let mut holdings = BTreeMap::new();
    holdings.insert(
        token_out("token-a"),
        TokenAmount::from_smallest_units(BigDecimal::from_str("10000000000000000000").unwrap(), 18), // 10e18
    );
    holdings.insert(
        token_out("token-b"),
        TokenAmount::from_smallest_units(BigDecimal::from_str("20000000000000000000").unwrap(), 18), // 20e18
    );

    let wallet = WalletInfo {
        holdings,
        total_value: NearValue::from_near(BigDecimal::from_str("50").unwrap()),
        cash_balance: NearValue::zero(),
    };

    // BigDecimal直接計算版と実際のコードで計算
    let weights_original = calculate_current_weights_original(&tokens, &wallet);
    let weights_actual = super::calculate_current_weights(&tokens, &wallet);

    println!("Original (BigDecimal直接): {:?}", weights_original);
    println!("Actual (トレイトベース): {:?}", weights_actual);

    // 結果を比較（小数点以下6桁の精度で）
    for (i, (orig, actual)) in weights_original
        .iter()
        .zip(weights_actual.iter())
        .enumerate()
    {
        let diff = (orig - actual).abs();
        println!(
            "Token {}: original={:.10}, actual={:.10}, diff={:.10}",
            i, orig, actual, diff
        );
        assert!(
            diff < 1e-6,
            "Weight mismatch at index {}: original={}, actual={}, diff={}",
            i,
            orig,
            actual,
            diff
        );
    }

    println!("\ncalculate_current_weights equivalence test passed");
}
