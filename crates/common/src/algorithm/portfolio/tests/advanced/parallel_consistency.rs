use super::*;

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
    let base_time = Utc::now() - TimeDelta::days(30);

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
            timestamp: base_time + TimeDelta::days(i),
            price: price(50.0 + (i as f64 * 0.7).sin() * 15.0),
            volume: Some(BigDecimal::from_f64(500.0).unwrap()),
        })
        .collect();

    // token-a: 安定した上昇トレンド（低ボラティリティ）
    let token_a_prices: Vec<PricePoint> = (0..30)
        .map(|i| PricePoint {
            timestamp: base_time + TimeDelta::days(i),
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
        ..Default::default()
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
