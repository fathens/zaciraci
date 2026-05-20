use super::*;

// ==================== damp_and_diff: 反復更新 + 数値ガード ====================

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
