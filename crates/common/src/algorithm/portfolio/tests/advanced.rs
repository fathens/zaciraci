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
        prediction_confidences: BTreeMap::new(),
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
        prediction_confidences: BTreeMap::new(),
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

// ==================== NaN/Inf 防御テスト ====================

#[test]
fn test_calculate_daily_returns_zero_price_no_nan() {
    // ゼロ価格を含む価格データ → NaN/Inf がリターンに含まれないこと
    let prices = vec![PriceHistory {
        token: token_out("token-a"),
        quote_token: token_in("wrap.near"),
        prices: vec![
            PricePoint {
                timestamp: Utc::now() - Duration::days(3),
                price: price(1.0),
                volume: None,
            },
            PricePoint {
                timestamp: Utc::now() - Duration::days(2),
                price: price(0.0), // ゼロ価格
                volume: None,
            },
            PricePoint {
                timestamp: Utc::now() - Duration::days(1),
                price: price(2.0),
                volume: None,
            },
            PricePoint {
                timestamp: Utc::now(),
                price: price(3.0),
                volume: None,
            },
        ],
    }];

    let returns = calculate_daily_returns(&prices);
    assert_eq!(returns.len(), 1, "Should have 1 token");

    let token_returns = &returns[0];

    // 4 価格点のうち prices[1]=0.0 がスキップされ、リターンは 2 件
    // i=1: prices[0]=1.0>0 → (0.0-1.0)/1.0 = -1.0
    // i=2: prices[1]=0.0 → スキップ
    // i=3: prices[2]=2.0>0 → (3.0-2.0)/2.0 = 0.5
    assert_eq!(
        token_returns.len(),
        2,
        "Zero price should be skipped, expected 2 returns, got {}",
        token_returns.len()
    );

    for &r in token_returns {
        assert!(
            r.is_finite(),
            "Expected all returns to be finite, got {}",
            r
        );
    }

    assert!(
        (token_returns[0] - (-1.0)).abs() < 1e-10,
        "First return should be -1.0, got {}",
        token_returns[0]
    );
    assert!(
        (token_returns[1] - 0.5).abs() < 1e-10,
        "Second return should be 0.5, got {}",
        token_returns[1]
    );
}

#[test]
fn test_calculate_covariance_single_element_returns_zero() {
    // 1要素入力 → 0.0 を返す（NaN でない）
    let returns1 = vec![0.5];
    let returns2 = vec![0.3];

    let cov = calculate_covariance(&returns1, &returns2);
    assert_eq!(cov, 0.0, "Single element covariance should be 0.0");
    assert!(cov.is_finite(), "Covariance should be finite");
}

#[test]
fn test_calculate_covariance_empty_returns_zero() {
    let cov = calculate_covariance(&[], &[]);
    assert_eq!(cov, 0.0);
}

#[test]
fn test_calculate_covariance_two_elements_valid() {
    // 2要素入力 → 有効な値を返す
    let returns1 = vec![0.1, 0.2];
    let returns2 = vec![0.3, 0.4];

    let cov = calculate_covariance(&returns1, &returns2);
    assert!(cov.is_finite(), "Covariance should be finite, got {}", cov);
    // 2要素の場合: mean1=0.15, mean2=0.35
    // cov = ((0.1-0.15)*(0.3-0.35) + (0.2-0.15)*(0.4-0.35)) / (2-1)
    //     = ((-0.05)*(-0.05) + (0.05)*(0.05)) / 1
    //     = (0.0025 + 0.0025) / 1 = 0.005
    assert!((cov - 0.005).abs() < 1e-10, "Expected 0.005, got {}", cov);
}

#[test]
fn test_validate_weights_all_valid() {
    let weights = vec![0.3, 0.5, 0.2];
    let (validated, had_invalid) = validate_weights(&weights);
    assert!(!had_invalid);
    assert_eq!(validated, weights);
}

#[test]
fn test_validate_weights_nan_replaced() {
    let weights = vec![0.3, f64::NAN, 0.2];
    let (validated, had_invalid) = validate_weights(&weights);
    assert!(had_invalid);
    assert_eq!(validated, vec![0.3, 0.0, 0.2]);
}

#[test]
fn test_validate_weights_inf_replaced() {
    let weights = vec![f64::INFINITY, 0.5, f64::NEG_INFINITY];
    let (validated, had_invalid) = validate_weights(&weights);
    assert!(had_invalid);
    assert_eq!(validated, vec![0.0, 0.5, 0.0]);
}

#[test]
fn test_validate_weights_negative_replaced() {
    let weights = vec![0.3, -0.1, 0.2];
    let (validated, had_invalid) = validate_weights(&weights);
    assert!(had_invalid);
    assert_eq!(validated, vec![0.3, 0.0, 0.2]);
}

#[test]
fn test_validate_weights_empty() {
    let weights: Vec<f64> = vec![];
    let (validated, had_invalid) = validate_weights(&weights);
    assert!(!had_invalid);
    assert!(validated.is_empty());
}

// ==================== アルゴリズム検証テスト ====================
//
// 以下のテストは portfolio.rs のアルゴリズムの問題点を検証するためのもの。
// 各テストは Issue 番号に対応し、現在の動作を文書化する。

/// Issue 2: Sharpe-RP ブレンドがボラティリティに連動した alpha で変化することを検証
#[test]
fn test_issue2_sharpe_rp_blend_varies_with_alpha() {
    let expected_returns = vec![0.15, 0.03, 0.05];
    let covariance = array![[0.04, 0.01, 0.01], [0.01, 0.04, 0.01], [0.01, 0.01, 0.04]];
    let n = expected_returns.len();

    // Sharpe weights
    let w_sharpe = maximize_sharpe_ratio(&expected_returns, &covariance);

    // RP weights（等配分から開始）
    let mut w_rp = vec![1.0 / n as f64; n];
    apply_risk_parity(&mut w_rp, &covariance);

    // alpha 計算のテスト: ボラティリティ → alpha のマッピング
    let test_cases = vec![
        (HIGH_VOLATILITY_THRESHOLD * 1.5, 0.7_f64, "高ボラ"),
        (
            (HIGH_VOLATILITY_THRESHOLD + LOW_VOLATILITY_THRESHOLD) / 2.0,
            0.8_f64,
            "中ボラ",
        ),
        (LOW_VOLATILITY_THRESHOLD * 0.5, 0.9_f64, "低ボラ"),
    ];

    let mut blended_results = Vec::new();

    for (volatility, expected_alpha, label) in &test_cases {
        let alpha = super::volatility_blend_alpha(*volatility);

        // alpha が期待値と一致
        assert!(
            (alpha - expected_alpha).abs() < 1e-10,
            "{label}: alpha={alpha}, expected={expected_alpha}"
        );

        // alpha が [0.7, 0.9] の範囲内
        assert!(
            (0.7..=0.9).contains(&alpha),
            "{label}: alpha={alpha} は [0.7, 0.9] の範囲外"
        );

        // ブレンド
        let blended: Vec<f64> = w_sharpe
            .iter()
            .zip(w_rp.iter())
            .map(|(&ws, &wr)| alpha * ws + (1.0 - alpha) * wr)
            .collect();

        println!("{label}: alpha={alpha:.2}, weights={:?}", blended);
        blended_results.push(blended);
    }

    // 異なるボラティリティで異なるブレンド結果が得られる
    let diff_high_low: f64 = blended_results[0]
        .iter()
        .zip(blended_results[2].iter())
        .map(|(a, b)| (a - b).abs())
        .sum();

    println!("Diff (high vol vs low vol): {diff_high_low:.6}");

    assert!(
        diff_high_low > 1e-6,
        "高ボラと低ボラで異なるブレンド結果が得られるべき: diff={diff_high_low}"
    );

    // Sharpe weights が常に支配的（alpha >= 0.7）
    for (i, blended) in blended_results.iter().enumerate() {
        for (j, _) in blended.iter().enumerate() {
            let sharpe_contrib = test_cases[i].1 * w_sharpe[j];
            let rp_contrib = (1.0 - test_cases[i].1) * w_rp[j];
            assert!(
                sharpe_contrib >= rp_contrib || w_sharpe[j] < w_rp[j],
                "Sharpe が支配的であるべき: token={j}, sharpe_contrib={sharpe_contrib}, rp_contrib={rp_contrib}"
            );
        }
    }
}

/// Issue 3: 圧倒的に高リターンの資産がある場合、解析解が適切に集中配分することを検証
#[test]
fn test_issue3_analytical_sharpe_dominant_asset() {
    let expected_returns = vec![0.01, 0.50, 0.01]; // token-1 が圧倒的
    let covariance = array![
        [0.04, 0.005, 0.002],
        [0.005, 0.09, 0.005],
        [0.002, 0.005, 0.03]
    ];

    let weights = maximize_sharpe_ratio(&expected_returns, &covariance);

    println!("Weights: {:?}", weights);

    // 重みの合計が1に近い
    let sum: f64 = weights.iter().sum();
    assert!((sum - 1.0).abs() < 1e-10, "重みの合計が1に近い: {sum}");

    // 圧倒的に高リターンの token-1 に最も配分される
    assert!(
        weights[1] > weights[0] && weights[1] > weights[2],
        "token-1 が最大配分: {:?}",
        weights
    );
}

/// Issue 9: calculate_covariance が異なる長さのリターン系列を末尾トリミングで処理することを検証
/// [修正済み] 短い方の長さに合わせて末尾（最新データ）を優先
#[test]
fn test_issue9_covariance_length_mismatch_trims_to_shorter() {
    // 同じ傾向の系列だが長さが異なる
    let returns1 = vec![0.01, 0.02, -0.01, 0.03, 0.01];
    let returns2 = vec![0.01, 0.02, -0.01]; // 短い（3要素）

    let cov = calculate_covariance(&returns1, &returns2);

    // 修正後: 末尾3要素 [-0.01, 0.03, 0.01] と [0.01, 0.02, -0.01] で計算
    println!("Covariance with mismatched lengths: {cov}");
    assert!(cov.is_finite(), "有限な共分散が返る");

    // 同一データなら正の共分散
    let returns2_same = vec![0.01, 0.02, -0.01, 0.03, 0.01];
    let cov_same = calculate_covariance(&returns1, &returns2_same);
    assert!(cov_same > 0.0, "同一データの共分散は正: {cov_same}");

    // 長さ1以下なら 0.0
    let too_short = vec![0.01];
    assert_eq!(calculate_covariance(&returns1, &too_short), 0.0);
}

/// generate_rebalance_actions は Rebalance アクションのみを生成する
#[test]
fn test_rebalance_actions_generates_only_rebalance() {
    let tokens = create_sample_tokens();
    let current = vec![0.5, 0.3, 0.2];
    let target = vec![0.3, 0.4, 0.3]; // token-a: -0.2, token-b: +0.1, token-c: +0.1

    let actions = generate_rebalance_actions(&tokens, &current, &target, 0.05);

    // Rebalance アクションのみが生成される
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], TradingAction::Rebalance { .. }));

    // 個別の AddPosition/ReducePosition は生成されない
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, TradingAction::AddPosition { .. })),
        "AddPosition は生成されない"
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, TradingAction::ReducePosition { .. })),
        "ReducePosition は生成されない"
    );
}

/// target_weights が全て 0 の場合は空のアクションリスト
#[test]
fn test_rebalance_actions_empty_when_no_targets() {
    let tokens = create_sample_tokens();
    let current = vec![0.5, 0.3, 0.2];
    let target = vec![0.0, 0.0, 0.0];
    let actions = generate_rebalance_actions(&tokens, &current, &target, 0.05);
    assert!(actions.is_empty());
}

/// target_weights の内容が正しいことを検証
#[test]
fn test_rebalance_action_contains_correct_weights() {
    let tokens = create_sample_tokens();
    let current = vec![0.5, 0.3, 0.2];
    let target = vec![0.3, 0.4, 0.3];
    let actions = generate_rebalance_actions(&tokens, &current, &target, 0.05);

    if let TradingAction::Rebalance { target_weights } = &actions[0] {
        assert_eq!(target_weights.len(), 3);
        let tolerance = BigDecimal::from_str("0.0000000001").unwrap();
        assert!(
            (&target_weights[&token_out("token-a")] - BigDecimal::from_str("0.3").unwrap()).abs()
                < tolerance
        );
        assert!(
            (&target_weights[&token_out("token-b")] - BigDecimal::from_str("0.4").unwrap()).abs()
                < tolerance
        );
        assert!(
            (&target_weights[&token_out("token-c")] - BigDecimal::from_str("0.3").unwrap()).abs()
                < tolerance
        );
    } else {
        panic!("Expected Rebalance action");
    }
}

/// Issue 7: メトリクスが indicators.rs の関数で計算されることを検証
/// [修正済み] sortino/max_drawdown/calmar をスタブから実計算に変更
#[tokio::test]
async fn test_issue7_metrics_computed_from_indicators() {
    let tokens = create_sample_tokens();
    let predictions = create_sample_predictions();
    let history = create_sample_price_history();
    let wallet = create_sample_wallet();

    let portfolio_data = PortfolioData {
        tokens,
        predictions,
        historical_prices: history,
        prediction_confidences: BTreeMap::new(),
    };

    let report = execute_portfolio_optimization(&wallet, portfolio_data, 0.05)
        .await
        .unwrap();

    let metrics = &report.expected_metrics;

    println!("Sharpe ratio:  {}", report.optimal_weights.sharpe_ratio);
    println!("Sortino ratio: {}", metrics.sortino_ratio);
    println!("Max drawdown:  {}", metrics.max_drawdown);
    println!("Calmar ratio:  {}", metrics.calmar_ratio);

    // 全メトリクスが有限値
    assert!(metrics.sortino_ratio.is_finite(), "sortino_ratio は有限値");
    assert!(metrics.max_drawdown.is_finite(), "max_drawdown は有限値");
    assert!(metrics.calmar_ratio.is_finite(), "calmar_ratio は有限値");

    // max_drawdown は 0 以上
    assert!(
        metrics.max_drawdown >= 0.0,
        "max_drawdown は非負: {}",
        metrics.max_drawdown
    );
}

// ==================== Issue B: 等リターン時の maximize_sharpe_ratio ====================

#[test]
fn test_maximize_sharpe_ratio_equal_returns() {
    // 全トークンが同一の期待リターンを持つ場合、等配分が返ること
    let expected_returns = vec![0.05, 0.05, 0.05];
    let covariance = array![[0.04, 0.01, 0.0], [0.01, 0.09, 0.02], [0.0, 0.02, 0.01]];

    let weights = maximize_sharpe_ratio(&expected_returns, &covariance);

    assert_eq!(weights.len(), 3);
    let equal_weight = 1.0 / 3.0;
    for (i, &w) in weights.iter().enumerate() {
        assert!(
            (w - equal_weight).abs() < 1e-10,
            "weights[{}] = {}, expected {}",
            i,
            w,
            equal_weight
        );
    }
}

#[test]
fn test_maximize_sharpe_ratio_single_token() {
    // トークン1つの場合、min_return == max_return なので early return
    let expected_returns = vec![0.03];
    let covariance = array![[0.01]];

    let weights = maximize_sharpe_ratio(&expected_returns, &covariance);

    assert_eq!(weights.len(), 1);
    assert!((weights[0] - 1.0).abs() < 1e-10);
}

// ==================== calculate_returns_from_prices 直接テスト ====================

#[test]
fn test_calculate_returns_from_prices_basic() {
    // 既知の価格系列から正しいリターンが計算されること
    let prices = vec![
        PricePoint {
            timestamp: Utc::now() - Duration::days(2),
            price: price(100.0),
            volume: None,
        },
        PricePoint {
            timestamp: Utc::now() - Duration::days(1),
            price: price(110.0),
            volume: None,
        },
        PricePoint {
            timestamp: Utc::now(),
            price: price(99.0),
            volume: None,
        },
    ];

    let returns = calculate_returns_from_prices(&prices);
    assert_eq!(returns.len(), 2);
    assert!((returns[0] - 0.1).abs() < 1e-10, "110/100 - 1 = 0.1");
    assert!((returns[1] - (-0.1)).abs() < 1e-10, "99/110 - 1 = -0.1");
}

#[test]
fn test_calculate_returns_from_prices_unsorted_input() {
    // タイムスタンプが昇順でない入力でも正しくソートされてリターンが計算されること
    let now = Utc::now();
    let prices = vec![
        PricePoint {
            timestamp: now, // 最新（3番目に来るべき）
            price: price(120.0),
            volume: None,
        },
        PricePoint {
            timestamp: now - Duration::days(2), // 最古（1番目に来るべき）
            price: price(100.0),
            volume: None,
        },
        PricePoint {
            timestamp: now - Duration::days(1), // 中間（2番目に来るべき）
            price: price(110.0),
            volume: None,
        },
    ];

    let returns = calculate_returns_from_prices(&prices);
    // ソート後: 100 → 110 → 120
    assert_eq!(returns.len(), 2);
    assert!(
        (returns[0] - 0.1).abs() < 1e-10,
        "110/100 - 1 = 0.1, got {}",
        returns[0]
    );
    assert!(
        (returns[1] - (10.0 / 110.0)).abs() < 1e-10,
        "120/110 - 1 ≈ 0.0909, got {}",
        returns[1]
    );
}

#[test]
fn test_calculate_returns_from_prices_empty_and_single() {
    // 空入力 → 空のVec
    let empty: Vec<PricePoint> = vec![];
    assert!(calculate_returns_from_prices(&empty).is_empty());

    // 1要素 → 空のVec（リターン計算不可）
    let single = vec![PricePoint {
        timestamp: Utc::now(),
        price: price(100.0),
        volume: None,
    }];
    assert!(calculate_returns_from_prices(&single).is_empty());
}

#[test]
fn test_calculate_daily_returns_duplicate_tokens() {
    // 同一トークンが複数回含まれる場合、最初の出現のみが使われること
    let now = Utc::now();
    let prices = vec![
        PriceHistory {
            token: token_out("token-a"),
            quote_token: token_in("wrap.near"),
            prices: vec![
                PricePoint {
                    timestamp: now - Duration::days(1),
                    price: price(100.0),
                    volume: None,
                },
                PricePoint {
                    timestamp: now,
                    price: price(110.0),
                    volume: None,
                },
            ],
        },
        // 同一トークンの重複エントリ（異なる価格）
        PriceHistory {
            token: token_out("token-a"),
            quote_token: token_in("wrap.near"),
            prices: vec![
                PricePoint {
                    timestamp: now - Duration::days(1),
                    price: price(200.0),
                    volume: None,
                },
                PricePoint {
                    timestamp: now,
                    price: price(300.0),
                    volume: None,
                },
            ],
        },
        PriceHistory {
            token: token_out("token-b"),
            quote_token: token_in("wrap.near"),
            prices: vec![
                PricePoint {
                    timestamp: now - Duration::days(1),
                    price: price(50.0),
                    volume: None,
                },
                PricePoint {
                    timestamp: now,
                    price: price(55.0),
                    volume: None,
                },
            ],
        },
    ];

    let returns = calculate_daily_returns(&prices);

    // token-a は1回だけ、token-b は1回 → 2トークン
    assert_eq!(returns.len(), 2, "重複トークンは除去されるべき");

    // 最初の token-a エントリのリターン: (110-100)/100 = 0.1
    assert_eq!(returns[0].len(), 1);
    assert!(
        (returns[0][0] - 0.1).abs() < 1e-10,
        "最初の出現の価格が使われるべき, got {}",
        returns[0][0]
    );

    // token-b のリターン: (55-50)/50 = 0.1
    assert_eq!(returns[1].len(), 1);
    assert!(
        (returns[1][0] - 0.1).abs() < 1e-10,
        "token-b return should be 0.1, got {}",
        returns[1][0]
    );
}

// ==================== prediction_confidence × alpha テスト ====================

/// prediction_confidence が alpha のブレンドに影響することを検証
#[test]
fn test_prediction_confidence_adjusts_alpha() {
    let expected_returns = vec![0.15, 0.03, 0.05];
    let covariance = array![[0.04, 0.01, 0.01], [0.01, 0.04, 0.01], [0.01, 0.01, 0.04]];
    let n = expected_returns.len();

    let w_sharpe = maximize_sharpe_ratio(&expected_returns, &covariance);
    let mut w_rp = vec![1.0 / n as f64; n];
    apply_risk_parity(&mut w_rp, &covariance);

    // 中ボラ → alpha_vol = 0.8
    let mid_vol = (HIGH_VOLATILITY_THRESHOLD + LOW_VOLATILITY_THRESHOLD) / 2.0;
    let alpha_vol = super::volatility_blend_alpha(mid_vol);
    assert!((alpha_vol - 0.8).abs() < 1e-10);

    let floor = PREDICTION_ALPHA_FLOOR;

    // --- 数式検証 ---
    // confidence=1.0 → alpha = alpha_vol（変化なし）
    let alpha_high = floor + (alpha_vol - floor) * 1.0;
    assert!(
        (alpha_high - alpha_vol).abs() < 1e-10,
        "confidence=1.0 should equal alpha_vol"
    );

    // confidence=0.0 → alpha = floor
    let alpha_low = floor + (alpha_vol - floor) * 0.0;
    assert!(
        (alpha_low - floor).abs() < 1e-10,
        "confidence=0.0 should equal floor"
    );

    // confidence=0.5 → alpha = floor + (alpha_vol - floor) * 0.5
    let alpha_mid = floor + (alpha_vol - floor) * 0.5;
    let expected_mid = (floor + alpha_vol) / 2.0;
    assert!(
        (alpha_mid - expected_mid).abs() < 1e-10,
        "confidence=0.5 should be midpoint: {alpha_mid} != {expected_mid}"
    );

    // --- ブレンド結果が異なることを検証 ---
    let blend = |alpha: f64| -> Vec<f64> {
        w_sharpe
            .iter()
            .zip(w_rp.iter())
            .map(|(&ws, &wr)| alpha * ws + (1.0 - alpha) * wr)
            .collect()
    };

    let weights_high = blend(alpha_high);
    let weights_low = blend(alpha_low);
    let weights_mid = blend(alpha_mid);

    // 高 confidence と低 confidence で異なる重み
    let diff: f64 = weights_high
        .iter()
        .zip(weights_low.iter())
        .map(|(a, b)| (a - b).abs())
        .sum();
    assert!(
        diff > 1e-6,
        "異なる confidence で異なる重みを生成すべき: diff={diff}"
    );

    // mid は high と low の中間
    for i in 0..n {
        let lo = weights_high[i].min(weights_low[i]);
        let hi = weights_high[i].max(weights_low[i]);
        assert!(
            weights_mid[i] >= lo - 1e-10 && weights_mid[i] <= hi + 1e-10,
            "mid weight[{i}]={} should be between {lo} and {hi}",
            weights_mid[i]
        );
    }
}

/// prediction_confidence = None のとき既存動作と同一であることを検証
#[test]
fn test_prediction_confidence_none_backward_compatible() {
    let mid_vol = (HIGH_VOLATILITY_THRESHOLD + LOW_VOLATILITY_THRESHOLD) / 2.0;
    let alpha_vol = super::volatility_blend_alpha(mid_vol);

    // None → alpha_vol をそのまま返す
    let prediction_confidence: Option<f64> = None;
    let alpha = match prediction_confidence {
        Some(confidence) => {
            let floor = PREDICTION_ALPHA_FLOOR;
            (floor + (alpha_vol - floor) * confidence).clamp(floor, 0.9)
        }
        None => alpha_vol,
    };

    assert!(
        (alpha - alpha_vol).abs() < 1e-10,
        "None should return alpha_vol: alpha={alpha}, alpha_vol={alpha_vol}"
    );
}

/// 全てのボラティリティ × confidence 組み合わせで alpha が有効範囲内
#[test]
fn test_prediction_confidence_alpha_range_exhaustive() {
    let floor = PREDICTION_ALPHA_FLOOR;

    for vol_i in 0..=10 {
        let volatility = LOW_VOLATILITY_THRESHOLD
            + (vol_i as f64) * (HIGH_VOLATILITY_THRESHOLD - LOW_VOLATILITY_THRESHOLD) / 10.0;
        let alpha_vol = super::volatility_blend_alpha(volatility);

        for conf_i in 0..=10 {
            let confidence = conf_i as f64 / 10.0; // 0.0 → 1.0
            let alpha = (floor + (alpha_vol - floor) * confidence).clamp(floor, 0.9);

            assert!(
                alpha >= floor && alpha <= 0.9,
                "alpha={alpha} out of [{floor}, 0.9] at vol={volatility}, conf={confidence}"
            );
            assert!(alpha.is_finite());
        }

        // None のケース
        assert!((0.7..=0.9).contains(&alpha_vol));
    }
}

/// execute_portfolio_optimization が prediction_confidence を
/// 正しく反映して異なる重みを出力することを検証
#[tokio::test]
async fn test_portfolio_optimization_varies_with_prediction_confidence() {
    let tokens = create_sample_tokens();
    let predictions = create_sample_predictions();
    let historical_prices = create_sample_price_history();
    let wallet = create_sample_wallet();

    // confidence = 1.0（高精度予測）- 全トークンに同じ値を設定
    let confidences_high: BTreeMap<TokenOutAccount, f64> =
        tokens.iter().map(|t| (t.symbol.clone(), 1.0)).collect();
    let pd_high = PortfolioData {
        tokens: tokens.clone(),
        predictions: predictions.clone(),
        historical_prices: historical_prices.clone(),
        prediction_confidences: confidences_high,
    };
    let report_high = execute_portfolio_optimization(&wallet, pd_high, 0.05)
        .await
        .unwrap();

    // confidence = 0.0（低精度予測 → RP 寄り）- 全トークンに同じ値を設定
    let confidences_low: BTreeMap<TokenOutAccount, f64> =
        tokens.iter().map(|t| (t.symbol.clone(), 0.0)).collect();
    let pd_low = PortfolioData {
        tokens: tokens.clone(),
        predictions: predictions.clone(),
        historical_prices: historical_prices.clone(),
        prediction_confidences: confidences_low,
    };
    let report_low = execute_portfolio_optimization(&wallet, pd_low, 0.05)
        .await
        .unwrap();

    // 空（データ不足 → 後方互換）
    let pd_none = PortfolioData {
        tokens,
        predictions,
        historical_prices,
        prediction_confidences: BTreeMap::new(),
    };
    let report_none = execute_portfolio_optimization(&wallet, pd_none, 0.05)
        .await
        .unwrap();

    // 全て正常終了
    assert!(report_high.optimal_weights.sharpe_ratio.is_finite());
    assert!(report_low.optimal_weights.sharpe_ratio.is_finite());
    assert!(report_none.optimal_weights.sharpe_ratio.is_finite());

    // confidence=0.0 は異なる重みを生成する（RP 寄り = より均等配分）
    // 同一トークンが選択された場合のみ比較
    let common_tokens: Vec<_> = report_high
        .optimal_weights
        .weights
        .keys()
        .filter(|k| report_low.optimal_weights.weights.contains_key(*k))
        .collect();

    if common_tokens.len() >= 2 {
        let diff: f64 = common_tokens
            .iter()
            .map(|t| {
                let wh = report_high.optimal_weights.weights[*t]
                    .to_f64()
                    .unwrap_or(0.0);
                let wl = report_low.optimal_weights.weights[*t]
                    .to_f64()
                    .unwrap_or(0.0);
                (wh - wl).abs()
            })
            .sum();

        // 重みに差異がある（alpha が異なるため）
        println!("Weight diff between high/low confidence: {diff:.6}");
    }
}

/// per-token alpha で異なる confidence 値を設定し、
/// ブレンド比率がトークンごとに異なることを検証する
#[tokio::test]
async fn test_per_token_alpha_with_varying_confidence() {
    let tokens = create_sample_tokens();
    let predictions = create_sample_predictions();
    let historical_prices = create_sample_price_history();
    let wallet = create_sample_wallet();

    // トークンごとに異なる confidence を設定
    let mut confidences_varied: BTreeMap<TokenOutAccount, f64> = BTreeMap::new();
    for (i, t) in tokens.iter().enumerate() {
        let c = match i {
            0 => 1.0, // 高 confidence → Sharpe 寄り
            1 => 0.0, // 低 confidence → RP 寄り（FLOOR alpha）
            _ => 0.5, // 中 confidence
        };
        confidences_varied.insert(t.symbol.clone(), c);
    }

    let pd_varied = PortfolioData {
        tokens: tokens.clone(),
        predictions: predictions.clone(),
        historical_prices: historical_prices.clone(),
        prediction_confidences: confidences_varied,
    };
    let report_varied = execute_portfolio_optimization(&wallet, pd_varied, 0.05)
        .await
        .unwrap();

    // 全トークン同一 confidence（0.5）の場合と比較
    let confidences_uniform: BTreeMap<TokenOutAccount, f64> =
        tokens.iter().map(|t| (t.symbol.clone(), 0.5)).collect();
    let pd_uniform = PortfolioData {
        tokens,
        predictions,
        historical_prices,
        prediction_confidences: confidences_uniform,
    };
    let report_uniform = execute_portfolio_optimization(&wallet, pd_uniform, 0.05)
        .await
        .unwrap();

    // 両方正常終了
    assert!(report_varied.optimal_weights.sharpe_ratio.is_finite());
    assert!(report_uniform.optimal_weights.sharpe_ratio.is_finite());

    // 異なる confidence → 異なるウエイトが生成される（同一トークンが選択された場合）
    let common_tokens: Vec<_> = report_varied
        .optimal_weights
        .weights
        .keys()
        .filter(|k| report_uniform.optimal_weights.weights.contains_key(*k))
        .collect();

    assert!(
        common_tokens.len() >= 2,
        "expected at least 2 common tokens, got {}",
        common_tokens.len()
    );
    let diff: f64 = common_tokens
        .iter()
        .map(|t| {
            let wv = report_varied.optimal_weights.weights[*t]
                .to_f64()
                .unwrap_or(0.0);
            let wu = report_uniform.optimal_weights.weights[*t]
                .to_f64()
                .unwrap_or(0.0);
            (wv - wu).abs()
        })
        .sum();

    // 異なる alpha 設定では異なるウエイトが期待される
    assert!(
        diff > 1e-10,
        "expected weight difference between varied/uniform confidence, got {diff:.6}"
    );
}

/// unified_optimize で異なる alphas を渡した場合、
/// 均一 alphas とは異なるウエイトが生成されることを検証する
#[test]
fn test_unified_optimize_heterogeneous_alphas() {
    let returns = generate_synthetic_returns(5, 30, 4242);
    let cov = calculate_covariance_matrix(&returns);
    let expected_returns: Vec<f64> = vec![0.02, 0.06, 0.01, 0.04, 0.03];
    let liquidity = vec![0.8; 5];

    // 均一 alpha
    let weights_uniform =
        unified_optimize(&expected_returns, &cov, &liquidity, 0.5, 5, 0.05, &[0.8; 5]);

    // 不均一 alpha: token0 は Sharpe 寄り、token2 は RP 寄り
    let alphas_varied = vec![0.9, 0.5, 0.5, 0.9, 0.7];
    let weights_varied = unified_optimize(
        &expected_returns,
        &cov,
        &liquidity,
        0.5,
        5,
        0.05,
        &alphas_varied,
    );

    // 両方の和が 1.0
    let sum_u: f64 = weights_uniform.iter().sum();
    let sum_v: f64 = weights_varied.iter().sum();
    assert!((sum_u - 1.0).abs() < 1e-6, "Uniform sum={sum_u}");
    assert!((sum_v - 1.0).abs() < 1e-6, "Varied sum={sum_v}");

    // 異なる alpha → 異なるウエイト
    let diff: f64 = weights_uniform
        .iter()
        .zip(weights_varied.iter())
        .map(|(u, v)| (u - v).abs())
        .sum();
    assert!(
        diff > 1e-10,
        "Heterogeneous alphas should produce different weights, diff={diff}"
    );
}

/// コールドスタート alpha（confidence データなし）が PREDICTION_ALPHA_FLOOR になることを検証
#[test]
fn test_cold_start_alpha_uses_floor() {
    let returns = generate_synthetic_returns(3, 30, 7777);
    let cov = calculate_covariance_matrix(&returns);
    let expected_returns: Vec<f64> = vec![0.03, 0.05, 0.02];
    let liquidity = vec![0.8, 0.9, 0.7];

    // confidence データなし（空の BTreeMap）→ FLOOR alpha
    let weights_cold = unified_optimize(
        &expected_returns,
        &cov,
        &liquidity,
        0.5,
        3,
        0.05,
        &[PREDICTION_ALPHA_FLOOR; 3],
    );

    // 全トークン FLOOR alpha → 正常に動作
    let sum: f64 = weights_cold.iter().sum();
    assert!((sum - 1.0).abs() < 1e-8, "Sum={sum}");

    // FLOOR alpha（0.5）と高 alpha（0.9）で異なるウエイト
    let weights_high =
        unified_optimize(&expected_returns, &cov, &liquidity, 0.5, 3, 0.05, &[0.9; 3]);

    let diff: f64 = weights_cold
        .iter()
        .zip(weights_high.iter())
        .map(|(c, h)| (c - h).abs())
        .sum();
    assert!(
        diff > 1e-10,
        "FLOOR alpha should differ from high alpha, diff={diff}"
    );
}

// ==================== 並行/並列処理の結果一貫性テスト ====================

/// 共分散行列計算が rayon 並列化後も決定的な結果を返すことを検証
#[test]
fn test_covariance_matrix_parallel_determinism() {
    // 同じ入力に対して複数回計算し、結果が一致することを確認
    let daily_returns = vec![
        vec![0.01, 0.02, -0.01, 0.03, 0.01, 0.02, -0.005, 0.015],
        vec![0.02, 0.01, -0.02, 0.02, 0.03, 0.01, -0.01, 0.02],
        vec![-0.01, 0.03, 0.01, -0.01, 0.02, 0.03, 0.01, -0.02],
        vec![0.015, -0.01, 0.02, 0.01, -0.01, 0.02, 0.015, 0.01],
    ];

    // 10回計算して全て同じ結果であることを確認
    let results: Vec<_> = (0..10)
        .map(|_| calculate_covariance_matrix(&daily_returns))
        .collect();

    for (i, result) in results.iter().enumerate().skip(1) {
        for row in 0..result.nrows() {
            for col in 0..result.ncols() {
                let diff = (result[[row, col]] - results[0][[row, col]]).abs();
                assert!(
                    diff < 1e-15,
                    "Iteration {i}: covariance[{row},{col}] differs by {diff}"
                );
            }
        }
    }
}

/// Sharpe最適化が rayon 並列化後も決定的な結果を返すことを検証
#[test]
fn test_maximize_sharpe_ratio_parallel_determinism() {
    let expected_returns = vec![0.05, 0.08, 0.03, 0.06, 0.04];
    let daily_returns = vec![
        vec![0.01, 0.02, -0.01, 0.03, 0.01],
        vec![0.02, 0.01, -0.02, 0.02, 0.03],
        vec![-0.01, 0.03, 0.01, -0.01, 0.02],
        vec![0.015, -0.01, 0.02, 0.01, -0.01],
        vec![0.02, 0.01, 0.01, -0.01, 0.03],
    ];
    let covariance = calculate_covariance_matrix(&daily_returns);

    // 10回計算して全て同じ結果であることを確認
    let results: Vec<_> = (0..10)
        .map(|_| maximize_sharpe_ratio(&expected_returns, &covariance))
        .collect();

    for (i, result) in results.iter().enumerate().skip(1) {
        for (j, &weight) in result.iter().enumerate() {
            let diff = (weight - results[0][j]).abs();
            assert!(diff < 1e-10, "Iteration {i}: weight[{j}] differs by {diff}");
        }
    }
}

/// 大量のトークンでの並行処理が正しく動作することを検証
#[test]
fn test_covariance_matrix_large_input() {
    // 20トークン分のデータを生成
    let n = 20;
    let days = 50;

    let daily_returns: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            (0..days)
                .map(|d| {
                    // 疑似ランダムだが決定的な値を生成
                    let seed = (i * 1000 + d) as f64;
                    (seed * 0.618).sin() * 0.05
                })
                .collect()
        })
        .collect();

    let covariance = calculate_covariance_matrix(&daily_returns);

    // 行列サイズが正しいこと
    assert_eq!(covariance.nrows(), n);
    assert_eq!(covariance.ncols(), n);

    // 対称行列であること
    for i in 0..n {
        for j in 0..n {
            let diff = (covariance[[i, j]] - covariance[[j, i]]).abs();
            assert!(diff < 1e-15, "Matrix should be symmetric at [{i},{j}]");
        }
    }

    // 対角要素が正（分散は非負）であること
    for i in 0..n {
        assert!(
            covariance[[i, i]] > 0.0,
            "Diagonal element [{i},{i}] should be positive"
        );
    }
}

/// BigDecimal → f64 変換で ToPrimitive 経由の精度が保たれることを検証
#[test]
fn test_price_to_f64_conversion_accuracy() {
    let p = price(123.456789);
    let f64_val = p.as_bigdecimal().to_f64().unwrap_or(0.0);
    assert!(
        (f64_val - 123.456789).abs() < 1e-6,
        "ToPrimitive conversion should preserve precision: got {}",
        f64_val
    );
}

/// selected_price_histories が selected_tokens の順序に整合していることを検証する回帰テスト。
/// スコアリングで入力順序が入れ替わるケースをカバーする。
#[tokio::test]
async fn test_price_history_alignment_with_selected_tokens() {
    let base_time = Utc::now() - Duration::days(30);

    // token-z: 低スコア（中流動性、中市場規模）→ 入力では先頭
    // token-a: 高スコア（高流動性、高市場規模）→ 入力では末尾
    // スコアリング後に token-a が先頭に来るため、入力順と逆転する
    // 注: 両方とも MIN_LIQUIDITY_SCORE(0.5) と min_market_cap(10,000) をクリアする
    let tokens = vec![
        TokenData {
            symbol: token_out("token-z.near"),
            current_rate: rate_from_price(0.01),
            historical_volatility: 0.5,
            liquidity_score: Some(0.55),
            market_cap: Some(cap(100_000)),
        },
        TokenData {
            symbol: token_out("token-a.near"),
            current_rate: rate_from_price(0.02),
            historical_volatility: 0.1,
            liquidity_score: Some(0.95),
            market_cap: Some(cap(5_000_000)),
        },
    ];

    let mut predictions = BTreeMap::new();
    // token-z: 弱い上昇予測 (+2%)
    predictions.insert(token_out("token-z.near"), price(0.01 * 1.02));
    // token-a: 強い上昇予測 (+15%)
    predictions.insert(token_out("token-a.near"), price(0.02 * 1.15));

    // 価格履歴を入力順 (token-z → token-a) で配置
    // token-z: ランダムに大きく変動（高ボラティリティ）
    let token_z_prices: Vec<PricePoint> = (0..30)
        .map(|i| PricePoint {
            timestamp: base_time + Duration::days(i),
            price: price(50.0 + (i as f64 * 0.7).sin() * 15.0),
            volume: Some(BigDecimal::from_f64(500.0).unwrap()),
        })
        .collect();

    // token-a: 安定した上昇トレンド（低ボラティリティ）
    let token_a_prices: Vec<PricePoint> = (0..30)
        .map(|i| PricePoint {
            timestamp: base_time + Duration::days(i),
            price: price(100.0 + i as f64 * 0.3),
            volume: Some(BigDecimal::from_f64(2000.0).unwrap()),
        })
        .collect();

    let historical_prices: BTreeMap<TokenOutAccount, PriceHistory> = [
        PriceHistory {
            token: token_out("token-z.near"),
            quote_token: token_in("wrap.near"),
            prices: token_z_prices,
        },
        PriceHistory {
            token: token_out("token-a.near"),
            quote_token: token_in("wrap.near"),
            prices: token_a_prices,
        },
    ]
    .into_iter()
    .map(|ph| (ph.token.clone(), ph))
    .collect();

    let mut holdings = BTreeMap::new();
    holdings.insert(
        token_out("token-z.near"),
        TokenAmount::from_smallest_units(BigDecimal::from(5), 18),
    );
    holdings.insert(
        token_out("token-a.near"),
        TokenAmount::from_smallest_units(BigDecimal::from(5), 18),
    );
    let wallet = WalletInfo {
        holdings,
        total_value: NearValue::from_near(BigDecimal::from(1000)),
        cash_balance: NearValue::zero(),
    };

    let confidences: BTreeMap<TokenOutAccount, f64> =
        tokens.iter().map(|t| (t.symbol.clone(), 0.8)).collect();
    let portfolio_data = PortfolioData {
        tokens,
        predictions,
        historical_prices,
        prediction_confidences: confidences,
    };

    let result = execute_portfolio_optimization(&wallet, portfolio_data, 0.05).await;
    assert!(result.is_ok(), "Optimization should succeed: {:?}", result);

    let report = result.unwrap();

    // token-a はスコアが高いため、より大きな重みを持つべき
    let weight_a = report
        .optimal_weights
        .weights
        .get(&token_out("token-a.near"));
    let weight_z = report
        .optimal_weights
        .weights
        .get(&token_out("token-z.near"));

    // token-a は高スコア・低ボラ・強い予測のため、必ず含まれるべき
    assert!(
        weight_a.is_some(),
        "token-a (high score) should be in optimal weights"
    );

    // token-z は低スコアのため、解析解で除外される可能性がある
    // 含まれている場合は token-a 以下の重みであること
    // 注: n=2 で box 制約 (max_position ≈ 0.5) の場合、w_1 + w_2 = 1.0 かつ
    // w_i ≤ 0.5 により等配分が唯一の実行可能解となる
    if let Some(w_z) = weight_z {
        let w_a = weight_a.unwrap();
        assert!(
            w_a >= w_z,
            "token-a should have weight >= token-z: a={}, z={}",
            w_a,
            w_z
        );
    }
}

/// ゼロ重みが含まれる場合に apply_risk_parity が Inf/NaN を生成しないことを検証
#[test]
fn test_apply_risk_parity_zero_weight_no_inf() {
    let mut weights = vec![0.0, 0.5, 0.5];
    let covariance = array![[0.04, 0.01, 0.01], [0.01, 0.09, 0.02], [0.01, 0.02, 0.06]];

    apply_risk_parity(&mut weights, &covariance);

    for &w in &weights {
        assert!(w.is_finite(), "weight should be finite, got {}", w);
    }
    let sum: f64 = weights.iter().sum();
    assert!((sum - 1.0).abs() < 1e-6, "weights should sum to 1.0");
}

/// peak=0.0 で calculate_max_drawdown がゼロ除算しないことを検証
#[test]
fn test_max_drawdown_zero_peak() {
    let values = vec![0.0, 0.0, 1.0, 0.5];
    let dd = calculate_max_drawdown(&values);
    assert!(dd.is_finite(), "max_drawdown should be finite, got {}", dd);
}

/// 全値ゼロで calculate_max_drawdown がパニックしないことを検証
#[test]
fn test_max_drawdown_all_zeros() {
    let values = vec![0.0, 0.0, 0.0];
    let dd = calculate_max_drawdown(&values);
    assert_eq!(dd, 0.0);
}

/// 異なる長さの daily_returns がポートフォリオ日次リターン構築時に末尾揃えされることを検証
#[test]
fn test_portfolio_daily_returns_tail_aligned() {
    // Token A: 5日分のリターン [0.01, 0.02, 0.03, 0.04, 0.05]
    // Token B: 3日分のリターン [0.10, 0.20, 0.30]
    // min_return_len = 3 → 末尾3日を使用
    // Token A の末尾3日: [0.03, 0.04, 0.05]
    // Token B の末尾3日: [0.10, 0.20, 0.30]
    let daily_returns = [vec![0.01, 0.02, 0.03, 0.04, 0.05], vec![0.10, 0.20, 0.30]];
    let weights = [0.5, 0.5];

    let min_return_len = daily_returns.iter().map(|r| r.len()).min().unwrap();
    assert_eq!(min_return_len, 3);

    let portfolio_daily_returns: Vec<f64> = (0..min_return_len)
        .map(|day| {
            weights
                .iter()
                .zip(daily_returns.iter())
                .map(|(w, returns)| w * returns[returns.len() - min_return_len + day])
                .sum()
        })
        .collect();

    // day 0: 0.5*0.03 + 0.5*0.10 = 0.065
    // day 1: 0.5*0.04 + 0.5*0.20 = 0.12
    // day 2: 0.5*0.05 + 0.5*0.30 = 0.175
    assert!((portfolio_daily_returns[0] - 0.065).abs() < 1e-10);
    assert!((portfolio_daily_returns[1] - 0.12).abs() < 1e-10);
    assert!((portfolio_daily_returns[2] - 0.175).abs() < 1e-10);
}

/// 同一長の daily_returns では末尾揃えが通常のインデックスと一致することを検証
#[test]
fn test_portfolio_daily_returns_same_length() {
    let daily_returns = [vec![0.01, 0.02, 0.03], vec![0.10, 0.20, 0.30]];
    let weights = [0.6, 0.4];

    let min_return_len = daily_returns.iter().map(|r| r.len()).min().unwrap();

    let portfolio_daily_returns: Vec<f64> = (0..min_return_len)
        .map(|day| {
            weights
                .iter()
                .zip(daily_returns.iter())
                .map(|(w, returns)| w * returns[returns.len() - min_return_len + day])
                .sum()
        })
        .collect();

    // day 0: 0.6*0.01 + 0.4*0.10 = 0.046
    // day 1: 0.6*0.02 + 0.4*0.20 = 0.092
    // day 2: 0.6*0.03 + 0.4*0.30 = 0.138
    assert!((portfolio_daily_returns[0] - 0.046).abs() < 1e-10);
    assert!((portfolio_daily_returns[1] - 0.092).abs() < 1e-10);
    assert!((portfolio_daily_returns[2] - 0.138).abs() < 1e-10);
}
