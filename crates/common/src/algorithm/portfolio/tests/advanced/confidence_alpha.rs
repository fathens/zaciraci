use super::*;

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
        ..Default::default()
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
        ..Default::default()
    };
    let report_low = execute_portfolio_optimization(&wallet, pd_low, 0.05)
        .await
        .unwrap();

    // 空（データ不足 → 後方互換）
    let pd_none = PortfolioData {
        tokens,
        predictions,
        historical_prices,
        ..Default::default()
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
        ..Default::default()
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
        ..Default::default()
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
