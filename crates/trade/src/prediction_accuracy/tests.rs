use super::*;

// デフォルト値を使用したテスト用ヘルパー
fn confidence_with_defaults(mape: f64) -> f64 {
    mape_to_confidence(mape, DEFAULT_MAPE_EXCELLENT, DEFAULT_MAPE_POOR)
}

// --- mape_to_confidence ---

#[test]
fn test_mape_to_confidence_excellent() {
    // MAPE ≤ EXCELLENT(3%) → confidence = 1.0
    assert_eq!(confidence_with_defaults(0.0), 1.0);
    assert_eq!(confidence_with_defaults(2.0), 1.0);
    assert_eq!(confidence_with_defaults(3.0), 1.0);
}

#[test]
fn test_mape_to_confidence_poor() {
    // MAPE ≥ POOR(15%) → confidence = 0.0
    assert_eq!(confidence_with_defaults(15.0), 0.0);
    assert_eq!(confidence_with_defaults(20.0), 0.0);
    assert_eq!(confidence_with_defaults(100.0), 0.0);
}

#[test]
fn test_mape_to_confidence_midpoint() {
    // MAPE = 9.0% → (15-9)/(15-3) = 6/12 = 0.5
    let c = confidence_with_defaults(9.0);
    assert!((c - 0.5).abs() < 1e-10, "expected 0.5, got {c}");
}

#[test]
fn test_mape_to_confidence_linear_interpolation() {
    // 6% → (15-6)/(15-3) = 9/12 = 0.75
    let c6 = confidence_with_defaults(6.0);
    assert!((c6 - 0.75).abs() < 1e-10);

    // 12% → (15-12)/(15-3) = 3/12 = 0.25
    let c12 = confidence_with_defaults(12.0);
    assert!((c12 - 0.25).abs() < 1e-10);

    // 単調減少
    assert!(c6 > c12);
}

#[test]
fn test_mape_to_confidence_monotonically_decreasing() {
    let mut prev = confidence_with_defaults(0.0);
    for i in 1..=30 {
        let mape = i as f64;
        let curr = confidence_with_defaults(mape);
        assert!(curr <= prev, "not monotonic at MAPE={mape}");
        prev = curr;
    }
}

#[test]
fn test_mape_to_confidence_custom_thresholds() {
    // カスタムしきい値: excellent=5.0, poor=20.0
    let excellent = 5.0;
    let poor = 20.0;

    // MAPE ≤ excellent → 1.0
    assert_eq!(mape_to_confidence(0.0, excellent, poor), 1.0);
    assert_eq!(mape_to_confidence(5.0, excellent, poor), 1.0);

    // MAPE ≥ poor → 0.0
    assert_eq!(mape_to_confidence(20.0, excellent, poor), 0.0);
    assert_eq!(mape_to_confidence(30.0, excellent, poor), 0.0);

    // 中間値: MAPE = 12.5 → (20-12.5)/(20-5) = 7.5/15 = 0.5
    let c = mape_to_confidence(12.5, excellent, poor);
    assert!((c - 0.5).abs() < 1e-10, "expected 0.5, got {c}");
}

// --- is_direction_correct ---

#[test]
fn test_is_direction_correct() {
    let prev = BigDecimal::from(100);

    // 両方上昇 → true
    assert!(is_direction_correct(
        &prev,
        &BigDecimal::from(110),
        &BigDecimal::from(105)
    ));

    // 予測上昇、実際下落 → false
    assert!(!is_direction_correct(
        &prev,
        &BigDecimal::from(110),
        &BigDecimal::from(95)
    ));

    // 両方下落 → true
    assert!(is_direction_correct(
        &prev,
        &BigDecimal::from(90),
        &BigDecimal::from(95)
    ));

    // 変化なし → true
    assert!(is_direction_correct(
        &prev,
        &BigDecimal::from(100),
        &BigDecimal::from(100)
    ));

    // 予測下落、実際上昇 → false
    assert!(!is_direction_correct(
        &prev,
        &BigDecimal::from(90),
        &BigDecimal::from(105)
    ));
}

// --- calculate_composite_confidence ---

#[test]
fn test_calculate_composite_confidence() {
    // MAPE のみ（方向データなし）- デフォルトしきい値 (3.0, 15.0) で計算
    let c1 = calculate_composite_confidence(9.0, None, 3.0, 15.0);
    // MAPE = 9.0 → (15-9)/(15-3) = 0.5
    assert!((c1 - 0.5).abs() < 0.01);

    // MAPE + 高い hit_rate (80%)
    let c2 = calculate_composite_confidence(9.0, Some(0.8), 3.0, 15.0);
    // mape_confidence = 0.5
    // direction_confidence = (0.8 - 0.5) * 2.0 = 0.6
    // composite = 0.6 * 0.5 + 0.4 * 0.6 = 0.3 + 0.24 = 0.54
    assert!((c2 - 0.54).abs() < 0.01);
    assert!(c2 > c1);

    // MAPE + 低い hit_rate (50% = ランダム)
    let c3 = calculate_composite_confidence(9.0, Some(0.5), 3.0, 15.0);
    // mape_confidence = 0.5
    // direction_confidence = (0.5 - 0.5) * 2.0 = 0.0
    // composite = 0.6 * 0.5 + 0.4 * 0.0 = 0.3
    assert!((c3 - 0.3).abs() < 0.01);
    assert!(c3 < c1);

    // 完璧なケース: MAPE 優秀 + 100% 方向正解
    let c4 = calculate_composite_confidence(2.0, Some(1.0), 3.0, 15.0);
    // mape_confidence = 1.0 (MAPE ≤ excellent)
    // direction_confidence = (1.0 - 0.5) * 2.0 = 1.0
    // composite = 0.6 * 1.0 + 0.4 * 1.0 = 1.0
    assert!((c4 - 1.0).abs() < 0.01);

    // 最悪ケース: MAPE 不良 + 50% 方向（ランダム）
    let c5 = calculate_composite_confidence(15.0, Some(0.5), 3.0, 15.0);
    // mape_confidence = 0.0 (MAPE ≥ poor)
    // direction_confidence = 0.0
    // composite = 0.0
    assert!((c5 - 0.0).abs() < 0.01);
}
