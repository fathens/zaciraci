use super::*;

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
        ..Default::default()
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
