use super::*;

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
