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

// --- damp_and_diff ---

#[test]
fn test_damp_and_diff_full_replacement() {
    // damping=1.0 で candidate がそのまま返り、diff = |candidate - current|
    let current = vec![0.0, 0.5, 1.0];
    let candidate = vec![1.0, 0.5, 0.0];
    let (new_weights, max_diff) = damp_and_diff(&current, &candidate, 1.0).unwrap();
    assert_eq!(new_weights, candidate);
    assert!((max_diff - 1.0).abs() < 1e-12);
}

#[test]
fn test_damp_and_diff_no_movement() {
    // damping=0.0 で current が維持され、diff = 0
    let current = vec![0.1, 0.4, 0.5];
    let candidate = vec![1.0, 1.0, 1.0];
    let (new_weights, max_diff) = damp_and_diff(&current, &candidate, 0.0).unwrap();
    assert_eq!(new_weights, current);
    assert_eq!(max_diff, 0.0);
}

#[test]
fn test_damp_and_diff_half_step() {
    // damping=0.5 で線形補間: new = 0.5 * current + 0.5 * candidate
    let current = vec![0.0, 0.0];
    let candidate = vec![1.0, 1.0];
    let (new_weights, max_diff) = damp_and_diff(&current, &candidate, 0.5).unwrap();
    for v in &new_weights {
        assert!((v - 0.5).abs() < 1e-12);
    }
    assert!((max_diff - 0.5).abs() < 1e-12);
}

#[test]
fn test_damp_and_diff_max_diff_picks_largest() {
    // max_diff は要素ごとの絶対差の最大値
    let current = vec![0.0, 0.0, 0.0];
    let candidate = vec![0.1, 0.5, 0.2];
    let (_, max_diff) = damp_and_diff(&current, &candidate, 1.0).unwrap();
    assert!((max_diff - 0.5).abs() < 1e-12);
}

#[test]
fn test_damp_and_diff_clamps_damping_above_one() {
    // damping=2.0 はクランプして 1.0 として扱われる → candidate と一致
    let current = vec![0.0, 0.0];
    let candidate = vec![1.0, 1.0];
    let (new_weights, _) = damp_and_diff(&current, &candidate, 2.0).unwrap();
    assert_eq!(new_weights, candidate);
}

#[test]
fn test_damp_and_diff_clamps_damping_below_zero() {
    // damping=-1.0 はクランプして 0.0 として扱われる → current 維持
    let current = vec![0.3, 0.7];
    let candidate = vec![1.0, 0.0];
    let (new_weights, max_diff) = damp_and_diff(&current, &candidate, -1.0).unwrap();
    assert_eq!(new_weights, current);
    assert_eq!(max_diff, 0.0);
}

#[test]
fn test_damp_and_diff_empty_slices() {
    // 空入力でも panic せず空ベクトル / max_diff=0 を返す
    let (new_weights, max_diff) = damp_and_diff(&[], &[], 0.5).unwrap();
    assert!(new_weights.is_empty());
    assert_eq!(max_diff, 0.0);
}

#[test]
fn test_damp_and_diff_length_mismatch_returns_err() {
    // 長さ不一致は呼び出し側のプログラミングバグだが、release panic で
    // process abort → cron 再起動 → 同条件再発で永続 crash loop に陥るため、
    // bail! に倒し caller が ? で受けて Hold に合流できるようにする。
    let current = vec![0.0, 0.0, 0.0];
    let candidate = vec![1.0, 1.0];
    let err = damp_and_diff(&current, &candidate, 0.5).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("length mismatch"),
        "unexpected error message: {msg}"
    );
    assert!(msg.contains("current=3"), "expected current=3 in: {msg}");
    assert!(
        msg.contains("candidate=2"),
        "expected candidate=2 in: {msg}"
    );
}

#[test]
fn test_damp_and_diff_nan_damping_returns_err() {
    // damping が NaN の場合は fail-loud で Err
    let current = vec![0.0, 0.0];
    let candidate = vec![1.0, 1.0];
    let err = damp_and_diff(&current, &candidate, f64::NAN).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("damping"), "unexpected error message: {msg}");
}

#[test]
fn test_damp_and_diff_infinite_damping_returns_err() {
    // damping が +∞ の場合も Err（is_finite チェック）
    let current = vec![0.0, 0.0];
    let candidate = vec![1.0, 1.0];
    let err = damp_and_diff(&current, &candidate, f64::INFINITY).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("damping"), "unexpected error message: {msg}");
}

#[test]
fn test_damp_and_diff_nan_in_current_returns_err() {
    // current_weights に NaN が混入したら fail-loud で Err
    let current = vec![0.5, f64::NAN, 0.5];
    let candidate = vec![1.0, 0.0, 0.0];
    let err = damp_and_diff(&current, &candidate, 0.5).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("current_weights[1]"),
        "unexpected error message: {msg}"
    );
}

#[test]
fn test_damp_and_diff_nan_in_candidate_returns_err() {
    // candidate_weights に NaN が混入したら fail-loud で Err
    let current = vec![0.5, 0.5];
    let candidate = vec![1.0, f64::NAN];
    let err = damp_and_diff(&current, &candidate, 0.5).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("candidate_weights[1]"),
        "unexpected error message: {msg}"
    );
}

#[test]
fn test_damp_and_diff_infinite_in_current_returns_err() {
    // current_weights に -∞ が混入したら fail-loud で Err
    let current = vec![0.5, f64::NEG_INFINITY];
    let candidate = vec![1.0, 0.0];
    let err = damp_and_diff(&current, &candidate, 0.5).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("current_weights[1]"),
        "unexpected error message: {msg}"
    );
}
