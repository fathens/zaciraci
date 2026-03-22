use super::*;

// デフォルト値を使用したテスト用ヘルパー
fn confidence_with_defaults(mape: f64) -> f64 {
    mape_to_confidence(mape, 3.0, 15.0)
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

#[test]
fn test_mape_to_confidence_equal_thresholds() {
    // poor == excellent のエッジケース: ゼロ除算を起こさないこと
    assert_eq!(mape_to_confidence(2.0, 3.0, 3.0), 1.0); // mape < excellent
    assert_eq!(mape_to_confidence(3.0, 3.0, 3.0), 1.0); // mape == excellent == poor
    assert_eq!(mape_to_confidence(5.0, 3.0, 3.0), 0.0); // mape > poor
}

// --- calculate_direction_accuracy_for_records ---

fn make_time(offset_hours: i64) -> NaiveDateTime {
    let base = chrono::DateTime::from_timestamp(1_700_000_000, 0)
        .unwrap()
        .naive_utc();
    base + chrono::Duration::hours(offset_hours)
}

fn make_record(
    target_time: NaiveDateTime,
    predicted: i64,
    actual: Option<i64>,
) -> DbPredictionRecord {
    DbPredictionRecord {
        id: 0,
        token: "token.near".to_string(),
        quote_token: "wrap.near".to_string(),
        predicted_price: BigDecimal::from(predicted),
        prediction_time: target_time - chrono::Duration::hours(24),
        target_time,
        actual_price: actual.map(BigDecimal::from),
        mape: None,
        absolute_error: None,
        evaluated_at: Some(target_time + chrono::Duration::hours(1)),
        created_at: target_time,
    }
}

#[test]
fn test_direction_accuracy_empty_input() {
    let (correct, total) = calculate_direction_accuracy_for_records(&[]);
    assert_eq!(correct, 0);
    assert_eq!(total, 0);
}

#[test]
fn test_direction_accuracy_single_record() {
    let records = vec![make_record(make_time(0), 100, Some(105))];
    let (correct, total) = calculate_direction_accuracy_for_records(&records);
    assert_eq!(correct, 0);
    assert_eq!(total, 0);
}

#[test]
fn test_direction_accuracy_with_none_actual() {
    let t1 = make_time(1);
    let t2 = make_time(0);
    // pair[0].actual_price = None → skip
    let records = vec![make_record(t1, 110, None), make_record(t2, 100, Some(100))];
    let (correct, total) = calculate_direction_accuracy_for_records(&records);
    assert_eq!(total, 0);
    assert_eq!(correct, 0);
}

#[test]
fn test_direction_accuracy_normal_cases() {
    // target_time DESC: t3 > t2 > t1
    let t1 = make_time(0);
    let t2 = make_time(1);
    let t3 = make_time(2);

    let records = vec![
        // pair[0]: predicted=120, actual=115 (上昇を正しく予測)
        make_record(t3, 120, Some(115)),
        // pair[1] (prev): actual=110
        // pair[0]: predicted=90, actual=110 (下落を予測したが上昇 → 不正解)
        make_record(t2, 90, Some(110)),
        // pair[1] (prev): actual=100
        make_record(t1, 100, Some(100)),
    ];

    let (correct, total) = calculate_direction_accuracy_for_records(&records);
    // pair (t3, t2): prev_actual=110, predicted=120(上昇), actual=115(上昇) → correct
    // pair (t2, t1): prev_actual=100, predicted=90(下落), actual=110(上昇) → incorrect
    assert_eq!(total, 2);
    assert_eq!(correct, 1);
}

// --- mape_to_confidence edge cases ---

#[test]
fn test_mape_to_confidence_nan_input() {
    let c = mape_to_confidence(f64::NAN, 3.0, 15.0);
    // NaN is non-finite and not sign-negative → 0.0 (worst confidence)
    assert_eq!(c, 0.0, "NaN MAPE should produce 0.0 confidence");
}

#[test]
fn test_mape_to_confidence_infinity_input() {
    let c = mape_to_confidence(f64::INFINITY, 3.0, 15.0);
    assert_eq!(c, 0.0, "INFINITY MAPE should produce 0.0 confidence");
}

#[test]
fn test_mape_to_confidence_negative_infinity_input() {
    let c = mape_to_confidence(f64::NEG_INFINITY, 3.0, 15.0);
    // MAPE is non-negative by definition; NEG_INFINITY is anomalous → worst confidence
    assert_eq!(c, 0.0, "NEG_INFINITY MAPE should produce 0.0 confidence");
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "poor")]
fn test_mape_to_confidence_poor_less_than_excellent() {
    // poor < excellent violates the contract
    mape_to_confidence(5.0, 15.0, 3.0);
}

// --- ラウンドトリップテスト ---

#[test]
fn test_token_account_display_fromstr_roundtrip() {
    let original = TokenAccount::from_str("token.near").unwrap();
    let roundtrip: TokenAccount = original.to_string().parse().unwrap();
    assert_eq!(original, roundtrip);
}

#[test]
fn test_predicted_price_bigdecimal_roundtrip() {
    // 極小値（NEARエコシステムのトークン価格で現実的な値）
    let price = BigDecimal::from_str("0.000000001234567890123456789").unwrap();
    let token_price = TokenPrice::from_near_per_token(price.clone());
    assert_eq!(*token_price.as_bigdecimal(), price);
}

#[test]
fn test_predicted_price_large_value_roundtrip() {
    let price = BigDecimal::from_str("123456789.987654321").unwrap();
    let token_price = TokenPrice::from_near_per_token(price.clone());
    assert_eq!(*token_price.as_bigdecimal(), price);
}
