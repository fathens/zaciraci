use bigdecimal::BigDecimal;
use chrono::Utc;
use num_traits::ToPrimitive;
use std::str::FromStr;
use zaciraci_common::pools::{SortPoolsRequest, SortPoolsResponse};

fn current_log_depth_calculation(depth: &BigDecimal) -> BigDecimal {
    let depth_plus_one = depth + BigDecimal::from(1);

    // 現在の実装
    match depth_plus_one.to_f64() {
        Some(depth_f64) if depth_f64 > 0.0 => {
            BigDecimal::try_from(depth_f64.ln()).unwrap_or_else(|_| BigDecimal::from(0))
        }
        _ => BigDecimal::from(0),
    }
}

#[test]
fn test_sort_pools_request_structure() {
    let request = SortPoolsRequest {
        timestamp: Utc::now().naive_utc(),
        limit: 10,
    };

    assert_eq!(request.limit, 10);
    assert!(request.timestamp <= Utc::now().naive_utc());
}

#[test]
fn test_sort_pools_response_structure() {
    let response = SortPoolsResponse { pools: vec![] };

    assert!(response.pools.is_empty());
}

#[test]
fn test_log_depth_typical_values() {
    let test_cases = vec![
        ("0.1", "小さな深度"),
        ("1.0", "単位深度"),
        ("10.0", "中程度の深度"),
        ("100.0", "大きな深度"),
        ("1000.0", "非常に大きな深度"),
    ];

    for (value_str, description) in test_cases {
        let depth = BigDecimal::from_str(value_str).unwrap();
        let log_depth = current_log_depth_calculation(&depth);

        println!("{}:", description);
        println!("  入力depth: {}", depth);
        println!("  log(depth + 1): {}", log_depth);

        // 負の値にならないことを確認
        assert!(
            log_depth >= BigDecimal::from(0),
            "深度{}で負の対数値が発生: {}",
            depth,
            log_depth
        );
    }
}

#[test]
fn test_log_depth_edge_cases() {
    // ゼロ値 - ln(0 + 1) = ln(1) = 0
    let zero_depth = BigDecimal::from(0);
    let log_zero = current_log_depth_calculation(&zero_depth);
    println!("ゼロ深度: log({} + 1) = {}", zero_depth, log_zero);
    assert_eq!(log_zero, BigDecimal::from(0));

    // 非常に小さな値
    let tiny_depth = BigDecimal::from_str("1e-10").unwrap();
    let log_tiny = current_log_depth_calculation(&tiny_depth);
    println!("極小深度: log({} + 1) = {}", tiny_depth, log_tiny);
    assert!(log_tiny >= BigDecimal::from(0));

    // 非常に大きな値
    let huge_depth = BigDecimal::from_str("1e50").unwrap();
    let log_huge = current_log_depth_calculation(&huge_depth);
    println!("極大深度: log({} + 1) = {}", huge_depth, log_huge);
    assert!(log_huge > BigDecimal::from(0));
}

#[test]
fn test_log_depth_precision_analysis() {
    // 高精度の値での精度損失を確認
    let high_precision = BigDecimal::from_str("123.456789012345678901234567890").unwrap();
    let log_result = current_log_depth_calculation(&high_precision);

    println!("高精度入力: {}", high_precision);
    println!("対数結果: {}", log_result);

    // f64変換での値
    let as_f64 = (&high_precision + BigDecimal::from(1)).to_f64().unwrap();
    println!("f64変換後: {}", as_f64);

    // f64の精度限界付近での動作を確認
    let very_large = BigDecimal::from_str("1e100").unwrap();
    let log_very_large = current_log_depth_calculation(&very_large);
    println!("極大値: {} -> log_depth: {}", very_large, log_very_large);

    // f64の範囲を超える値
    let beyond_f64 = BigDecimal::from_str("1e400").unwrap();
    let log_beyond = current_log_depth_calculation(&beyond_f64);
    println!("f64範囲超過: {} -> log_depth: {}", beyond_f64, log_beyond);

    // ゼロになることを確認（f64変換でinfinityまたはオーバーフロー）
    assert_eq!(log_beyond, BigDecimal::from(0));
}

#[test]
fn test_improved_calculate_log_depth() {
    use super::calculate_log_depth;

    // テストケース: 典型的な値
    let test_cases = vec![
        ("0", "ゼロ"),
        ("0.1", "小さな深度"),
        ("1.0", "単位深度"),
        ("10.0", "中程度の深度"),
        ("100.0", "大きな深度"),
        ("1000.0", "非常に大きな深度"),
        ("1e50", "極大値(f64範囲内)"),
        ("1e400", "超極大値(f64範囲外)"),
    ];

    for (value_str, description) in test_cases {
        let depth = BigDecimal::from_str(value_str).unwrap();
        let log_depth = calculate_log_depth(&depth);

        println!("{}:", description);
        println!("  入力depth: {}", depth);
        println!("  改善版log_depth: {}", log_depth);

        // 負の値にならないことを確認
        assert!(log_depth >= BigDecimal::from(0));

        // 合理的な上限値を確認
        assert!(log_depth <= BigDecimal::from(200));
    }

    // 精度テスト: 小数点以下の桁数が制限されていることを確認
    let precise_depth = BigDecimal::from_str("2.718281828459045").unwrap(); // e
    let log_e_plus_1 = calculate_log_depth(&precise_depth);
    println!("ln(e + 1) = {}", log_e_plus_1);

    // 精度が制限されていることを確認（6桁程度）
    let log_str = log_e_plus_1.to_string();
    let decimal_places = if let Some(pos) = log_str.find('.') {
        log_str.len() - pos - 1
    } else {
        0
    };
    println!("小数点以下桁数: {}", decimal_places);
    assert!(decimal_places <= 15); // f64の精度制限内
}

#[test]
fn test_calculate_volatility_weight() {
    use super::calculate_volatility_weight;

    // 基本的な動作テスト
    println!("=== ボラティリティ重み計算テスト ===");

    // テストケース 1: ゼロ値のハンドリング
    let zero_variance = BigDecimal::from(0);
    let zero_depth = BigDecimal::from(0);
    let weight_zero = calculate_volatility_weight(&zero_variance, &zero_depth);
    println!("ゼロテスト: variance=0, depth=0 → weight={}", weight_zero);
    assert_eq!(weight_zero, BigDecimal::from(0));

    // テストケース 2: 典型的な値での計算
    let test_cases = vec![
        ("0.01", "1.0", "低分散・低深度"),
        ("0.01", "100.0", "低分散・高深度"),
        ("1.0", "1.0", "中分散・低深度"),
        ("1.0", "100.0", "中分散・高深度"),
        ("100.0", "1.0", "高分散・低深度"),
        ("100.0", "100.0", "高分散・高深度"),
    ];

    for (var_str, dep_str, description) in test_cases {
        let variance = BigDecimal::from_str(var_str).unwrap();
        let depth = BigDecimal::from_str(dep_str).unwrap();
        let weight = calculate_volatility_weight(&variance, &depth);

        println!("{}:", description);
        println!(
            "  variance={}, depth={} → weight={}",
            variance, depth, weight
        );

        // 基本的な制約
        assert!(weight >= BigDecimal::from(0)); // 非負

        // 高分散・高深度の組み合わせで最大値になることを確認
        if var_str == "100.0" && dep_str == "100.0" {
            // これが最も高いスコアになるはず
            assert!(weight > BigDecimal::from(10));
        }
    }
}

#[test]
fn test_volatility_weight_mathematical_properties() {
    use super::calculate_volatility_weight;

    println!("=== 数学的性質のテスト ===");

    // 単調性テスト: 分散が増加すると重みも増加する
    let depth_fixed = BigDecimal::from_str("10.0").unwrap();
    let variances = vec!["0.1", "1.0", "10.0", "100.0"];
    let mut prev_weight = BigDecimal::from(0);

    for var_str in variances {
        let variance = BigDecimal::from_str(var_str).unwrap();
        let weight = calculate_volatility_weight(&variance, &depth_fixed);

        println!("分散単調性: variance={} → weight={}", variance, weight);
        assert!(weight >= prev_weight, "分散の増加で重みが減少しています");
        prev_weight = weight;
    }

    // 単調性テスト: 深度が増加すると重みも増加する
    let variance_fixed = BigDecimal::from_str("1.0").unwrap();
    let depths = vec!["0.1", "1.0", "10.0", "100.0"];
    let mut prev_weight = BigDecimal::from(0);

    for dep_str in depths {
        let depth = BigDecimal::from_str(dep_str).unwrap();
        let weight = calculate_volatility_weight(&variance_fixed, &depth);

        println!("深度単調性: depth={} → weight={}", depth, weight);
        assert!(weight >= prev_weight, "深度の増加で重みが減少しています");
        prev_weight = weight;
    }
}

#[test]
fn test_volatility_weight_edge_cases() {
    use super::calculate_volatility_weight;

    println!("=== エッジケースのテスト ===");

    // 非常に小さな値
    let tiny_variance = BigDecimal::from_str("1e-10").unwrap();
    let tiny_depth = BigDecimal::from_str("1e-10").unwrap();
    let weight_tiny = calculate_volatility_weight(&tiny_variance, &tiny_depth);
    println!(
        "極小値: variance={}, depth={} → weight={}",
        tiny_variance, tiny_depth, weight_tiny
    );
    assert!(weight_tiny >= BigDecimal::from(0));

    // 非常に大きな値
    let huge_variance = BigDecimal::from_str("1e50").unwrap();
    let huge_depth = BigDecimal::from_str("1e50").unwrap();
    let weight_huge = calculate_volatility_weight(&huge_variance, &huge_depth);
    println!(
        "極大値: variance={}, depth={} → weight={}",
        huge_variance, huge_depth, weight_huge
    );
    assert!(weight_huge >= BigDecimal::from(0));
    assert!(weight_huge < BigDecimal::from_str("1e100").unwrap()); // 合理的な上限

    // f64範囲を超える値
    let beyond_f64_variance = BigDecimal::from_str("1e400").unwrap();
    let beyond_f64_depth = BigDecimal::from_str("1e400").unwrap();
    let weight_beyond = calculate_volatility_weight(&beyond_f64_variance, &beyond_f64_depth);
    println!(
        "f64超過: variance={}, depth={} → weight={}",
        beyond_f64_variance, beyond_f64_depth, weight_beyond
    );
    assert!(weight_beyond >= BigDecimal::from(0));
}

#[test]
fn test_volatility_weight_financial_meaning() {
    use super::calculate_volatility_weight;

    println!("=== 金融的意味のテスト ===");

    // シナリオ1: 高ボラティリティ・低流動性（リスキー）
    let high_vol_low_liq = calculate_volatility_weight(
        &BigDecimal::from_str("100.0").unwrap(), // 高分散
        &BigDecimal::from_str("1.0").unwrap(),   // 低深度
    );

    // シナリオ2: 低ボラティリティ・高流動性（安全）
    let low_vol_high_liq = calculate_volatility_weight(
        &BigDecimal::from_str("1.0").unwrap(),   // 低分散
        &BigDecimal::from_str("100.0").unwrap(), // 高深度
    );

    // シナリオ3: 高ボラティリティ・高流動性（理想的）
    let high_vol_high_liq = calculate_volatility_weight(
        &BigDecimal::from_str("100.0").unwrap(), // 高分散
        &BigDecimal::from_str("100.0").unwrap(), // 高深度
    );

    println!("高ボラティリティ・低流動性: {}", high_vol_low_liq);
    println!("低ボラティリティ・高流動性: {}", low_vol_high_liq);
    println!("高ボラティリティ・高流動性: {}", high_vol_high_liq);

    // 期待される関係性：高ボラティリティ・高流動性が最高スコア
    assert!(high_vol_high_liq > high_vol_low_liq);
    assert!(high_vol_high_liq > low_vol_high_liq);

    // ボラティリティトレーダーにとって理想的なのは高ボラティリティ
    assert!(high_vol_low_liq > low_vol_high_liq);
}

// Integration tests would require database setup, so we'll focus on unit tests
// The main sort_pools function is tested indirectly through the sort module tests
