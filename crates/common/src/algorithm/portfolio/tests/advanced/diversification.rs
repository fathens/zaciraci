//! `apply_prediction_error_diagonal` の diversification 圧縮 property test。
//!
//! 本関数は対角のみを inflate して off-diagonal は据え置くため、結果として
//! implied correlation `cov[i,j] / sqrt(cov[i,i] × cov[j,j])` が圧縮され、
//! Markowitz の diversification benefit が削られる。これは [`portfolio.rs`
//! の `apply_prediction_error_diagonal` docstring] で意図的な近似として
//! 記述された既知の挙動。本モジュールは production default
//! (`Additive` + `k=0.1`、`pev=0.04` 相当) で実際に correlation が
//! どこまで collapse するかを numeric に pin する。
//!
//! Phase 2 で correlation-preserving rescaling
//! (`new_cov = D × old_cov × D` with `D = diag(sqrt(new_diag/old_diag))`)
//! が実装されると、本テストの assert は「壊れるべき」シグナルとして
//! 機能する（旧近似の挙動が維持されていれば fail、修正されれば fail）。
//! その時点でテストの assert を新しい挙動に書き換えること。

use super::*;

/// 2×2 共分散行列を `var=v`、`corr=ρ` から組み立てる。
///
/// daily price var ~10⁻⁴（年率 30% 相当の `HIGH_VOLATILITY_THRESHOLD` 付近）
/// で `corr = 0.5` の設定は、apply_prediction_error_diagonal の docstring が
/// 引用する典型的な低 volatility ケース。
fn correlated_2x2(var: f64, corr: f64) -> ndarray::Array2<f64> {
    let cov = var * corr;
    let mut m = ndarray::Array2::<f64>::zeros((2, 2));
    m[[0, 0]] = var;
    m[[1, 1]] = var;
    m[[0, 1]] = cov;
    m[[1, 0]] = cov;
    m
}

/// `cov[i,j] / sqrt(cov[i,i] × cov[j,j])` を返す。
fn implied_correlation(cov: &ndarray::Array2<f64>, i: usize, j: usize) -> f64 {
    cov[[i, j]] / (cov[[i, i]].sqrt() * cov[[j, j]].sqrt())
}

#[test]
fn test_apply_pred_err_diagonal_collapses_correlation_at_default_k() {
    // production default の Additive + k=0.1 と、mape=20% 相当の MSRE pev=0.04 で、
    // daily price var ~10⁻⁴ の対称 2-token portfolio に対して
    // implied correlation 0.5 → ~0.012 まで collapse することを pin。
    //
    // 数値検算:
    //   inflated_diag = 1e-4 + 0.1 × 0.04 = 4.1e-3
    //   off_diag      = 0.5 × 1e-4 = 5e-5（変化なし）
    //   new_corr      = 5e-5 / sqrt(4.1e-3 × 4.1e-3) ≈ 0.01219...
    //
    // この圧縮は意図的な近似で、Markowitz の diversification benefit が
    // ほぼ消えていることを示す。Phase 2 で correlation-preserving rescaling
    // が実装されたら本 assert は更新される（テストが「壊れる」のが正しい signal）。
    let cov = correlated_2x2(1e-4, 0.5);
    let pre_corr = implied_correlation(&cov, 0, 1);
    assert!(
        (pre_corr - 0.5).abs() < 1e-10,
        "pre-inflate correlation must be 0.5 by construction, got {pre_corr}"
    );

    let tokens = vec![token_out("low-a"), token_out("low-b")];
    let mut variances = BTreeMap::new();
    variances.insert(token_out("low-a"), 0.04);
    variances.insert(token_out("low-b"), 0.04);

    let result = apply_prediction_error_diagonal(
        cov,
        &tokens,
        &variances,
        0.1,
        PredErrDiagonalMode::Additive,
    );

    let post_corr = implied_correlation(&result, 0, 1);

    // diversification benefit がほぼ消えるレベルまで圧縮される
    assert!(
        post_corr.abs() < 0.02,
        "default k=0.1 must collapse correlation toward 0, got {post_corr}"
    );
    // 上記境界を超えて 1 桁圧縮（0.5 → < 0.05）も別軸で pin
    assert!(
        post_corr < pre_corr / 10.0,
        "post-inflate correlation must be <1/10 of pre-inflate; pre={pre_corr} post={post_corr}"
    );

    // off-diagonal が触られていない（数学契約のもう片方）
    assert!(
        (result[[0, 1]] - 5e-5).abs() < 1e-12,
        "off-diagonal must be untouched: expected 5e-5, got {}",
        result[[0, 1]]
    );
    assert!(
        (result[[1, 0]] - 5e-5).abs() < 1e-12,
        "off-diagonal must be untouched: expected 5e-5, got {}",
        result[[1, 0]]
    );
}

#[test]
fn test_apply_pred_err_diagonal_high_volatility_collapses_less() {
    // docstring の altcoin ケース (daily var ~10⁻³、年率 80-150% 相当) では
    // 同じ k=0.1 / pev=0.04 でも対角インフレ倍率が ~5× に緩和される。
    //
    // 数値検算:
    //   inflated_diag = 1e-3 + 0.1 × 0.04 = 5.0e-3
    //   off_diag      = 0.5 × 1e-3 = 5e-4
    //   new_corr      = 5e-4 / sqrt(5.0e-3 × 5.0e-3) = 0.10
    //
    // 低 volatility ケースより緩いが diversification は半分以下に削られる。
    let cov = correlated_2x2(1e-3, 0.5);
    let tokens = vec![token_out("alt-a"), token_out("alt-b")];
    let mut variances = BTreeMap::new();
    variances.insert(token_out("alt-a"), 0.04);
    variances.insert(token_out("alt-b"), 0.04);

    let result = apply_prediction_error_diagonal(
        cov,
        &tokens,
        &variances,
        0.1,
        PredErrDiagonalMode::Additive,
    );
    let post_corr = implied_correlation(&result, 0, 1);

    // 0.5 → 0.10 付近で約半分以下まで圧縮（低 volatility ケースよりは緩い）
    assert!(
        (post_corr - 0.10).abs() < 0.01,
        "high-vol case must compress to ~0.10, got {post_corr}"
    );
    assert!(
        post_corr < 0.5 / 2.0,
        "high-vol correlation must still be <1/2 of original 0.5, got {post_corr}"
    );
}

#[test]
fn test_apply_pred_err_diagonal_rescale_preserves_correlation() {
    // Phase 2 で導入された Rescale モードは Additive と同じ対角インフレを
    // 適用しつつ、`new_cov = D · old_cov · D` で off-diagonal を比例 scale して
    // implied correlation を保持する。production default の `Additive` で
    // 0.5 → ~0.012 まで圧縮された correlation が、`Rescale` では 0.5 のまま
    // 維持されることを pin する。
    //
    // 数値検算（Additive 部分は collapses_correlation_at_default_k と同条件）:
    //   inflated_diag = 1e-4 + 0.1 × 0.04 = 4.1e-3
    //   D[i] = sqrt(4.1e-3 / 1e-4) = sqrt(41) ≈ 6.403
    //   off_diag (rescaled) = 5e-5 × D[0] × D[1] = 5e-5 × 41 = 2.05e-3
    //   new_corr = 2.05e-3 / sqrt(4.1e-3 × 4.1e-3) = 2.05e-3 / 4.1e-3 = 0.5
    let cov = correlated_2x2(1e-4, 0.5);
    let pre_corr = implied_correlation(&cov, 0, 1);

    let tokens = vec![token_out("rs-a"), token_out("rs-b")];
    let mut variances = BTreeMap::new();
    variances.insert(token_out("rs-a"), 0.04);
    variances.insert(token_out("rs-b"), 0.04);

    let result = apply_prediction_error_diagonal(
        cov,
        &tokens,
        &variances,
        0.1,
        PredErrDiagonalMode::Rescale,
    );

    let post_corr = implied_correlation(&result, 0, 1);

    // 相関は数値誤差レンジ内で保持される
    assert!(
        (post_corr - pre_corr).abs() < 1e-9,
        "Rescale must preserve correlation: pre={pre_corr} post={post_corr}"
    );

    // 対角は Additive と同じ値に膨らむ（リスクは引き上げる）
    let expected_diag = 1e-4 + 0.1 * 0.04;
    assert!((result[[0, 0]] - expected_diag).abs() < 1e-12);
    assert!((result[[1, 1]] - expected_diag).abs() < 1e-12);

    // off-diagonal は比例で scale された値 (≈ 2.05e-3)
    let expected_off = 5e-5 * expected_diag / 1e-4;
    assert!((result[[0, 1]] - expected_off).abs() < 1e-9);
    assert!((result[[1, 0]] - expected_off).abs() < 1e-9);
    assert!((result[[0, 1]] - result[[1, 0]]).abs() < 1e-12, "symmetry");
}

#[test]
fn test_apply_pred_err_diagonal_rescale_skips_missing_pev_row() {
    // pev が片側のみで他方が欠損のとき、欠損側は Additive 同様に対角据え置き、
    // off-diagonal は欠損側の D[i] = 1 で scale。残った行の D は適用される。
    let cov = correlated_2x2(1e-4, 0.5);
    let tokens = vec![token_out("rs-c"), token_out("rs-d")];
    let mut variances = BTreeMap::new();
    variances.insert(token_out("rs-c"), 0.04);
    // rs-d は欠損

    let result = apply_prediction_error_diagonal(
        cov,
        &tokens,
        &variances,
        0.1,
        PredErrDiagonalMode::Rescale,
    );

    // 対角: rs-c は inflate、rs-d は据え置き
    assert!((result[[0, 0]] - (1e-4 + 0.1 * 0.04)).abs() < 1e-12);
    assert!((result[[1, 1]] - 1e-4).abs() < 1e-12);

    // off-diagonal: D[0] = sqrt(41), D[1] = 1 → scale = sqrt(41)
    let expected_off = 5e-5 * (41.0_f64).sqrt();
    assert!((result[[0, 1]] - expected_off).abs() < 1e-9);
    assert!((result[[1, 0]] - expected_off).abs() < 1e-9);
}

#[test]
fn test_apply_pred_err_diagonal_zero_k_preserves_correlation() {
    // k=0 は inflate を完全に無効化する。production の `default=false` ゲートが
    // 緩んでも k=0 で実質 disable できることを保証（運用での緊急 mitigation 経路）。
    let cov = correlated_2x2(1e-4, 0.5);
    let tokens = vec![token_out("zk-a"), token_out("zk-b")];
    let mut variances = BTreeMap::new();
    variances.insert(token_out("zk-a"), 0.04);
    variances.insert(token_out("zk-b"), 0.04);

    let result = apply_prediction_error_diagonal(
        cov,
        &tokens,
        &variances,
        0.0,
        PredErrDiagonalMode::Additive,
    );

    let post_corr = implied_correlation(&result, 0, 1);
    assert!(
        (post_corr - 0.5).abs() < 1e-10,
        "k=0 must leave correlation untouched, got {post_corr}"
    );
}
