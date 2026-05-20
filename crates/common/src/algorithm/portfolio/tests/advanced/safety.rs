use super::*;

// ==================== 数値安全性: ゼロ重み / ゼロ peak / 末尾揃え ====================

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
