use super::*;

// ==================== NaN/Inf 防御テスト ====================

#[test]
fn test_calculate_daily_returns_zero_price_no_nan() {
    // ゼロ価格を含む価格データ → NaN/Inf がリターンに含まれないこと
    let prices = vec![PriceHistory {
        token: token_out("token-a"),
        quote_token: token_in("wrap.near"),
        prices: vec![
            PricePoint {
                timestamp: Utc::now() - TimeDelta::days(3),
                price: price(1.0),
                volume: None,
            },
            PricePoint {
                timestamp: Utc::now() - TimeDelta::days(2),
                price: price(0.0), // ゼロ価格
                volume: None,
            },
            PricePoint {
                timestamp: Utc::now() - TimeDelta::days(1),
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

// --- apply_prediction_error_diagonal ---

fn diag_2x2(d00: f64, d11: f64) -> ndarray::Array2<f64> {
    let mut m = ndarray::Array2::<f64>::zeros((2, 2));
    m[[0, 0]] = d00;
    m[[1, 1]] = d11;
    m
}

#[test]
fn test_apply_prediction_error_diagonal_additive_increases_diagonal() {
    let cov = diag_2x2(0.01, 0.02);
    let tokens = vec![token_out("aa"), token_out("bb")];
    let mut variances = BTreeMap::new();
    variances.insert(token_out("aa"), 0.005);
    variances.insert(token_out("bb"), 0.0);

    let result = apply_prediction_error_diagonal(
        cov,
        &tokens,
        &variances,
        1.0,
        PredErrDiagonalMode::Additive,
    );
    // additive: 0.01 + 1.0 * 0.005 = 0.015
    assert!((result[[0, 0]] - 0.015).abs() < 1e-10);
    // 0.02 + 1.0 * 0.0 = 0.02
    assert!((result[[1, 1]] - 0.02).abs() < 1e-10);
}

#[test]
fn test_apply_prediction_error_diagonal_max_picks_larger() {
    let cov = diag_2x2(0.01, 0.02);
    let tokens = vec![token_out("aa"), token_out("bb")];
    let mut variances = BTreeMap::new();
    variances.insert(token_out("aa"), 0.05); // 大 → 採用
    variances.insert(token_out("bb"), 0.001); // 小 → 据え置き

    let result =
        apply_prediction_error_diagonal(cov, &tokens, &variances, 1.0, PredErrDiagonalMode::Max);
    // max(0.01, 1.0 * 0.05) = 0.05
    assert!((result[[0, 0]] - 0.05).abs() < 1e-10);
    // max(0.02, 1.0 * 0.001) = 0.02 (据え置き)
    assert!((result[[1, 1]] - 0.02).abs() < 1e-10);
}

#[test]
fn test_apply_prediction_error_diagonal_missing_entry_keeps_diagonal() {
    let cov = diag_2x2(0.01, 0.02);
    let tokens = vec![token_out("aa"), token_out("bb")];
    let mut variances = BTreeMap::new();
    variances.insert(token_out("aa"), 0.005);
    // bb は欠損

    let result = apply_prediction_error_diagonal(
        cov,
        &tokens,
        &variances,
        2.0,
        PredErrDiagonalMode::Additive,
    );
    assert!((result[[0, 0]] - (0.01 + 2.0 * 0.005)).abs() < 1e-10);
    assert!((result[[1, 1]] - 0.02).abs() < 1e-10); // 据え置き
}

#[test]
fn test_apply_prediction_error_diagonal_skips_non_finite() {
    let cov = diag_2x2(0.01, 0.02);
    let tokens = vec![token_out("aa"), token_out("bb")];
    let mut variances = BTreeMap::new();
    variances.insert(token_out("aa"), f64::NAN);
    variances.insert(token_out("bb"), f64::INFINITY);

    let result = apply_prediction_error_diagonal(
        cov,
        &tokens,
        &variances,
        1.0,
        PredErrDiagonalMode::Additive,
    );
    // どちらも skip → 据え置き
    assert!((result[[0, 0]] - 0.01).abs() < 1e-10);
    assert!((result[[1, 1]] - 0.02).abs() < 1e-10);
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
