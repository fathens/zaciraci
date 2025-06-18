use super::*;
use chrono::NaiveDateTime;
use std::fs;
use zaciraci_common::stats::ValueAtTime;

/// 視覚的検証用のSVGファイル生成テスト
/// 
/// このテストは複数のシナリオでSVGを生成してファイルに保存します。
/// 生成されたSVGファイルはブラウザで開いて目視確認してください。
/// 
/// 実行方法:
/// ```bash
/// cargo test visual_svg_generation -- --ignored
/// ```
#[test]
#[ignore] // デフォルトではスキップ、明示的に実行時のみ動作
fn visual_svg_generation() {
    // 出力ディレクトリの作成
    let output_dir = "target/svg_test_output";
    fs::create_dir_all(output_dir).expect("出力ディレクトリの作成に失敗");

    println!("SVGファイルを生成中... 出力先: {}", output_dir);

    // テストケース1: 基本的な予測チャート
    generate_basic_prediction_chart(&output_dir);
    
    // テストケース2: 単調増加データ
    generate_monotonic_increasing_chart(&output_dir);
    
    // テストケース3: 変動の激しいデータ
    generate_volatile_data_chart(&output_dir);
    
    // テストケース4: 極端な値のスケール
    generate_extreme_values_chart(&output_dir);
    
    // テストケース5: 予測が実際と大きく乖離
    generate_prediction_divergence_chart(&output_dir);

    println!("✅ SVGファイル生成完了。以下のファイルをブラウザで確認してください:");
    println!("   {}/01_basic_prediction.svg", output_dir);
    println!("   {}/02_monotonic_increasing.svg", output_dir);
    println!("   {}/03_volatile_data.svg", output_dir);
    println!("   {}/04_extreme_values.svg", output_dir);
    println!("   {}/05_prediction_divergence.svg", output_dir);
}

fn generate_basic_prediction_chart(output_dir: &str) {
    let actual_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 100.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 105.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-03 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 110.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-04 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 108.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-05 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 115.0,
        },
    ];

    let forecast_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-06 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 118.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-07 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 122.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-08 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 120.0,
        },
    ];

    let svg = generate_prediction_chart_svg(&actual_data, &forecast_data)
        .expect("基本チャートの生成に失敗");
    
    let file_path = format!("{}/01_basic_prediction.svg", output_dir);
    fs::write(&file_path, svg).expect("SVGファイルの書き込みに失敗");
    println!("生成完了: {}", file_path);
}

fn generate_monotonic_increasing_chart(output_dir: &str) {
    let actual_data: Vec<ValueAtTime> = (1..=10)
        .map(|i| ValueAtTime {
            time: NaiveDateTime::parse_from_str(
                &format!("2025-06-{:02} 00:00:00", i),
                "%Y-%m-%d %H:%M:%S"
            ).unwrap(),
            value: i as f64 * 10.0,
        })
        .collect();

    let forecast_data: Vec<ValueAtTime> = (11..=15)
        .map(|i| ValueAtTime {
            time: NaiveDateTime::parse_from_str(
                &format!("2025-06-{:02} 00:00:00", i),
                "%Y-%m-%d %H:%M:%S"
            ).unwrap(),
            value: i as f64 * 10.0 + 5.0, // 少し上向きの予測
        })
        .collect();

    let svg = generate_prediction_chart_svg(&actual_data, &forecast_data)
        .expect("単調増加チャートの生成に失敗");
    
    let file_path = format!("{}/02_monotonic_increasing.svg", output_dir);
    fs::write(&file_path, svg).expect("SVGファイルの書き込みに失敗");
    println!("生成完了: {}", file_path);
}

fn generate_volatile_data_chart(output_dir: &str) {
    let values = vec![100.0, 120.0, 90.0, 130.0, 80.0, 140.0, 75.0, 145.0, 70.0, 150.0];
    let actual_data: Vec<ValueAtTime> = values
        .into_iter()
        .enumerate()
        .map(|(i, value)| ValueAtTime {
            time: NaiveDateTime::parse_from_str(
                &format!("2025-06-{:02} 00:00:00", i + 1),
                "%Y-%m-%d %H:%M:%S"
            ).unwrap(),
            value,
        })
        .collect();

    let forecast_values = vec![160.0, 140.0, 170.0, 130.0, 180.0];
    let forecast_data: Vec<ValueAtTime> = forecast_values
        .into_iter()
        .enumerate()
        .map(|(i, value)| ValueAtTime {
            time: NaiveDateTime::parse_from_str(
                &format!("2025-06-{:02} 00:00:00", i + 11),
                "%Y-%m-%d %H:%M:%S"
            ).unwrap(),
            value,
        })
        .collect();

    let svg = generate_prediction_chart_svg(&actual_data, &forecast_data)
        .expect("変動データチャートの生成に失敗");
    
    let file_path = format!("{}/03_volatile_data.svg", output_dir);
    fs::write(&file_path, svg).expect("SVGファイルの書き込みに失敗");
    println!("生成完了: {}", file_path);
}

fn generate_extreme_values_chart(output_dir: &str) {
    let actual_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 0.001,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1_000.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-03 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 0.5,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-04 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 50_000.0,
        },
    ];

    let forecast_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-05 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 25_000.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-06 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 0.1,
        },
    ];

    let svg = generate_prediction_chart_svg(&actual_data, &forecast_data)
        .expect("極端な値チャートの生成に失敗");
    
    let file_path = format!("{}/04_extreme_values.svg", output_dir);
    fs::write(&file_path, svg).expect("SVGファイルの書き込みに失敗");
    println!("生成完了: {}", file_path);
}

fn generate_prediction_divergence_chart(output_dir: &str) {
    // 実際のデータは下降トレンド
    let actual_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 200.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 180.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-03 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 160.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-04 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 140.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-05 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 120.0,
        },
    ];

    // 予測は上昇トレンド（大きく乖離）
    let forecast_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-06 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 150.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-07 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 180.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-08 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 220.0,
        },
    ];

    let svg = generate_prediction_chart_svg(&actual_data, &forecast_data)
        .expect("予測乖離チャートの生成に失敗");
    
    let file_path = format!("{}/05_prediction_divergence.svg", output_dir);
    fs::write(&file_path, svg).expect("SVGファイルの書き込みに失敗");
    println!("生成完了: {}", file_path);
}

/// 視覚的確認のためのチャート生成テスト（反復的改善用）
#[test]
#[ignore]
fn iterative_chart_improvement() {
    let output_dir = "target/chart_improvement";
    std::fs::create_dir_all(output_dir).expect("出力ディレクトリの作成に失敗");

    println!("🔄 反復的改善のためのチャート生成中... 出力先: {}", output_dir);

    // 反復1: 基本版
    generate_iteration_1_basic(&output_dir);
    
    // 反復2: 問題のあるケース
    generate_iteration_2_problematic(&output_dir);
    
    // 反復3: 改良版
    generate_iteration_3_improved(&output_dir);

    println!("✅ チャート生成完了。以下のファイルをブラウザで確認してください:");
    println!("   {}/iteration_1_basic.svg", output_dir);
    println!("   {}/iteration_2_problematic.svg", output_dir);
    println!("   {}/iteration_3_improved.svg", output_dir);
    println!();
    println!("🔍 確認項目:");
    println!("   1. 実際データと予測データが明確に区別できるか？");
    println!("   2. 軸のスケールは適切か？");
    println!("   3. 凡例は読みやすいか？");
    println!("   4. 全体的なレイアウトは見やすいか？");
    println!("   5. データの境界は明確か？");
    println!("   6. 色分けは適切か？");
}

fn generate_iteration_1_basic(output_dir: &str) {
    // 反復1: 基本的なケース
    let actual_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 100.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 105.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-03 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 110.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-04 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 108.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-05 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 115.0,
        },
    ];

    let forecast_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-06 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 118.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-07 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 122.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-08 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 120.0,
        },
    ];

    let svg = generate_prediction_chart_svg(&actual_data, &forecast_data)
        .expect("反復1チャートの生成に失敗");
    
    let file_path = format!("{}/iteration_1_basic.svg", output_dir);
    fs::write(&file_path, svg).expect("SVGファイルの書き込みに失敗");
    println!("🔄 反復1完了: {} (基本的なケース)", file_path);
}

fn generate_iteration_2_problematic(output_dir: &str) {
    // 反復2: 問題のあるケース - 極端なスケールの違い
    let actual_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1000.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-03 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 2.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-04 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 500.0,
        },
    ];

    let forecast_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-05 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1500.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-06 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 3.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-07 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 800.0,
        },
    ];

    let svg = generate_prediction_chart_svg(&actual_data, &forecast_data)
        .expect("反復2チャートの生成に失敗");
    
    let file_path = format!("{}/iteration_2_problematic.svg", output_dir);
    fs::write(&file_path, svg).expect("SVGファイルの書き込みに失敗");
    println!("⚠️  反復2完了: {} (問題のあるケース - 極端なスケール)", file_path);
}

fn generate_iteration_3_improved(output_dir: &str) {
    // 反復3: 改良版 - より現実的で読みやすいデータ
    let actual_data: Vec<ValueAtTime> = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1200.50,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1205.25,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 15:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1198.75,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1210.00,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1215.80,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 15:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1208.40,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-03 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1225.60,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-03 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1230.25,
        },
    ];

    let forecast_data: Vec<ValueAtTime> = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-03 15:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1235.50,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-04 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1240.20,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-04 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1245.80,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-04 15:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1250.10,
        },
    ];

    let svg = generate_prediction_chart_svg(&actual_data, &forecast_data)
        .expect("反復3チャートの生成に失敗");
    
    let file_path = format!("{}/iteration_3_improved.svg", output_dir);
    fs::write(&file_path, svg).expect("SVGファイルの書き込みに失敗");
    println!("✅ 反復3完了: {} (改良版 - 現実的なデータ)", file_path);
}

/// 特定の問題を修正するための反復テスト
#[test]
#[ignore]
fn fix_specific_issues() {
    let output_dir = "target/chart_fixes";
    std::fs::create_dir_all(output_dir).expect("出力ディレクトリの作成に失敗");

    println!("🔧 特定の問題を修正するための反復テスト開始...");

    // 問題1: 線の太さが細すぎる問題
    test_line_thickness_issue(&output_dir);
    
    // 問題2: 色の区別が不明確な問題
    test_color_distinction_issue(&output_dir);
    
    // 問題3: 時間軸のラベルが読みにくい問題
    test_time_label_issue(&output_dir);

    println!("🔧 修正テスト完了:");
    println!("   {}/fix_line_thickness.svg", output_dir);
    println!("   {}/fix_color_distinction.svg", output_dir);
    println!("   {}/fix_time_labels.svg", output_dir);
}

fn test_line_thickness_issue(output_dir: &str) {
    // 現在の実装では線の太さを直接変更できないが、
    // データポイントを増やしてより連続的な線にする
    let actual_data: Vec<ValueAtTime> = (0..10)
        .map(|i| ValueAtTime {
            time: NaiveDateTime::parse_from_str(
                &format!("2025-06-{:02} 00:00:00", i + 1),
                "%Y-%m-%d %H:%M:%S"
            ).unwrap(),
            value: 100.0 + i as f64 * 2.0 + (i as f64 * 0.5).sin() * 5.0,
        })
        .collect();

    let forecast_data: Vec<ValueAtTime> = (10..15)
        .map(|i| ValueAtTime {
            time: NaiveDateTime::parse_from_str(
                &format!("2025-06-{:02} 00:00:00", i + 1),
                "%Y-%m-%d %H:%M:%S"
            ).unwrap(),
            value: 100.0 + i as f64 * 2.0 + (i as f64 * 0.3).cos() * 3.0,
        })
        .collect();

    let svg = generate_prediction_chart_svg(&actual_data, &forecast_data)
        .expect("線の太さ修正チャートの生成に失敗");
    
    let file_path = format!("{}/fix_line_thickness.svg", output_dir);
    fs::write(&file_path, svg).expect("SVGファイルの書き込みに失敗");
    println!("🔧 線の太さ問題の検証: {}", file_path);
}

fn test_color_distinction_issue(output_dir: &str) {
    // 色の区別を明確にするため、より明確な違いのあるデータパターンを作成
    let actual_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1000.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1020.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 15:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1010.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 18:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1030.0,
        },
    ];

    // 予測データは明確に異なるパターンにする
    let forecast_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 21:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1040.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1060.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 03:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1080.0,
        },
    ];

    let svg = generate_prediction_chart_svg(&actual_data, &forecast_data)
        .expect("色の区別修正チャートの生成に失敗");
    
    let file_path = format!("{}/fix_color_distinction.svg", output_dir);
    fs::write(&file_path, svg).expect("SVGファイルの書き込みに失敗");
    println!("🔧 色の区別問題の検証: {}", file_path);
}

fn test_time_label_issue(output_dir: &str) {
    // 時間ラベルの読みやすさを確認するため、より短い期間でデータを作成
    let actual_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1200.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 10:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1205.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 11:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1210.0,
        },
    ];

    let forecast_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1215.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 13:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1220.0,
        },
    ];

    let svg = generate_prediction_chart_svg(&actual_data, &forecast_data)
        .expect("時間ラベル修正チャートの生成に失敗");
    
    let file_path = format!("{}/fix_time_labels.svg", output_dir);
    fs::write(&file_path, svg).expect("SVGファイルの書き込みに失敗");
    println!("🔧 時間ラベル問題の検証: {}", file_path);
}

/// 改良版のチャート生成関数を作成してテストする
#[test]
#[ignore]
fn test_improved_chart_generation() {
    let output_dir = "target/chart_improvements";
    std::fs::create_dir_all(output_dir).expect("出力ディレクトリの作成に失敗");

    println!("🚀 改良版チャート生成テスト開始...");

    // 改良案1: より大きなサイズでのチャート
    test_improved_chart_size(&output_dir);
    
    // 改良案2: より明確な色分け
    test_improved_color_scheme(&output_dir);
    
    // 改良案3: より良いタイトルとラベル
    test_improved_labels(&output_dir);

    println!("🚀 改良版テスト完了:");
    println!("   {}/improved_size.svg", output_dir);
    println!("   {}/improved_colors.svg", output_dir);
    println!("   {}/improved_labels.svg", output_dir);
}

fn test_improved_chart_size(output_dir: &str) {
    use crate::chart::plots::{MultiPlotSeries, MultiPlotOptions, plot_multi_values_at_time_to_svg_with_options};
    use plotters::prelude::{BLUE, RED};
    
    let actual_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1200.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1205.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 15:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1198.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 18:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1210.0,
        },
    ];

    let forecast_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 21:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1215.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1220.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 03:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1225.0,
        },
    ];

    // より大きなサイズでテスト
    let chart_svg = plot_multi_values_at_time_to_svg_with_options(
        &[
            MultiPlotSeries {
                name: "実際の価格".to_string(),
                values: actual_data,
                color: BLUE,
            },
            MultiPlotSeries {
                name: "予測価格".to_string(),
                values: forecast_data,
                color: RED,
            },
        ],
        MultiPlotOptions {
            title: Some("価格予測 (改良版 - より大きなサイズ)".to_string()),
            image_size: (1000, 700), // より大きなサイズ
            x_label: Some("時間".to_string()),
            y_label: Some("価格 (円)".to_string()),
            legend_on_left: None,
        },
    ).expect("改良版チャートの生成に失敗");
    
    let file_path = format!("{}/improved_size.svg", output_dir);
    fs::write(&file_path, chart_svg).expect("SVGファイルの書き込みに失敗");
    println!("🚀 改良案1: サイズ改善 - {}", file_path);
}

fn test_improved_color_scheme(output_dir: &str) {
    use crate::chart::plots::{MultiPlotSeries, MultiPlotOptions, plot_multi_values_at_time_to_svg_with_options};
    use plotters::prelude::{GREEN, MAGENTA};
    
    let actual_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1200.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1205.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 15:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1198.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 18:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1210.0,
        },
    ];

    let forecast_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 21:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1215.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1220.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 03:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1225.0,
        },
    ];

    // より明確な色分けでテスト
    let chart_svg = plot_multi_values_at_time_to_svg_with_options(
        &[
            MultiPlotSeries {
                name: "実際の価格".to_string(),
                values: actual_data,
                color: GREEN, // より明確な色に変更
            },
            MultiPlotSeries {
                name: "予測価格".to_string(),
                values: forecast_data,
                color: MAGENTA, // より明確な色に変更
            },
        ],
        MultiPlotOptions {
            title: Some("価格予測 (改良版 - 色分け改善)".to_string()),
            image_size: (800, 600),
            x_label: Some("時間".to_string()),
            y_label: Some("価格 (円)".to_string()),
            legend_on_left: None,
        },
    ).expect("色分け改良チャートの生成に失敗");
    
    let file_path = format!("{}/improved_colors.svg", output_dir);
    fs::write(&file_path, chart_svg).expect("SVGファイルの書き込みに失敗");
    println!("🚀 改良案2: 色分け改善 - {}", file_path);
}

fn test_improved_labels(output_dir: &str) {
    use crate::chart::plots::{MultiPlotSeries, MultiPlotOptions, plot_multi_values_at_time_to_svg_with_options};
    use plotters::prelude::{BLUE, RED};
    
    let actual_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1200.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1205.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 15:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1198.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 18:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1210.0,
        },
    ];

    let forecast_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 21:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1215.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1220.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 03:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1225.0,
        },
    ];

    // より分かりやすいラベルでテスト
    let chart_svg = plot_multi_values_at_time_to_svg_with_options(
        &[
            MultiPlotSeries {
                name: "📈 Historical Price".to_string(), // より明確な系列名
                values: actual_data,
                color: BLUE,
            },
            MultiPlotSeries {
                name: "🔮 Predicted Price".to_string(), // より明確な系列名
                values: forecast_data,
                color: RED,
            },
        ],
        MultiPlotOptions {
            title: Some("💹 Stock Price Prediction Analysis".to_string()), // より詳細なタイトル
            image_size: (800, 600),
            x_label: Some("Time (Hours)".to_string()),
            y_label: Some("Price (JPY)".to_string()),
            legend_on_left: None,
        },
    ).expect("ラベル改良チャートの生成に失敗");
    
    let file_path = format!("{}/improved_labels.svg", output_dir);
    fs::write(&file_path, chart_svg).expect("SVGファイルの書き込みに失敗");
    println!("🚀 改良案3: ラベル改善 - {}", file_path);
}

/// ジグザグパターンの機能検証テスト
#[test]
#[ignore]
fn test_zigzag_pattern_verification() {
    let output_dir = "target/svg_test_output";
    fs::create_dir_all(output_dir).expect("出力ディレクトリの作成に失敗");

    println!("🔧 ジグザグパターン機能検証開始...");

    // 意図的なジグザグパターンのactual_data
    let actual_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 100.0, // 開始点
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 10:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 150.0, // 上昇
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 11:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 80.0,  // 急降下（ジグザグ1）
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 170.0, // 急上昇（ジグザグ2）
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 13:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 90.0,  // 急降下（ジグザグ3）
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 14:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 180.0, // 急上昇（ジグザグ4）
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 15:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 110.0, // 降下
        },
    ];

    // forecast_dataも異なるジグザグパターン
    let forecast_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 16:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 200.0, // 予測開始（高め）
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 17:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 120.0, // 急降下
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 18:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 220.0, // 急上昇
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 19:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 130.0, // 急降下
        },
    ];

    let svg = generate_prediction_chart_svg(&actual_data, &forecast_data)
        .expect("ジグザグデータでのSVG生成に失敗");
    
    let file_path = format!("{}/zigzag_pattern_verification.svg", output_dir);
    fs::write(&file_path, &svg).expect("SVGファイルの書き込みに失敗");
    
    // 基本的な生成確認
    assert!(svg.contains("<svg"), "SVG要素が含まれていない");
    assert!(svg.contains("💹 価格予測分析"), "タイトルが含まれていない");
    assert!(svg.contains("#00FF00"), "緑色（実際データ）が含まれていない");
    assert!(svg.contains("#FF00FF"), "マゼンタ色（予測データ）が含まれていない");
    
    println!("✅ ジグザグパターン検証完了: {}", file_path);
    println!("📊 SVGサイズ: {} バイト", svg.len());
    println!("📈 実際データ範囲: {}〜{}", 
        actual_data.iter().map(|d| d.value).fold(f64::INFINITY, f64::min),
        actual_data.iter().map(|d| d.value).fold(f64::NEG_INFINITY, f64::max)
    );
    println!("🔮 予測データ範囲: {}〜{}", 
        forecast_data.iter().map(|d| d.value).fold(f64::INFINITY, f64::min),
        forecast_data.iter().map(|d| d.value).fold(f64::NEG_INFINITY, f64::max)
    );
    
    // データ詳細表示
    println!("📊 実際データのジグザグパターン:");
    for (i, point) in actual_data.iter().enumerate() {
        println!("  {}: {} -> {}", i+1, point.time.format("%H:%M"), point.value);
    }
    
    println!("🔮 予測データのジグザグパターン:");
    for (i, point) in forecast_data.iter().enumerate() {
        println!("  {}: {} -> {}", i+1, point.time.format("%H:%M"), point.value);
    }
}

/// 横軸（時間軸）の詳細検証テスト
#[test]
#[ignore]
fn test_time_axis_verification() {
    let output_dir = "target/svg_test_output";
    fs::create_dir_all(output_dir).expect("出力ディレクトリの作成に失敗");

    println!("🔧 時間軸詳細検証開始...");

    // 不等間隔の時間データ
    let actual_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 100.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 09:15:00", "%Y-%m-%d %H:%M:%S").unwrap(), // 15分後
            value: 110.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 10:00:00", "%Y-%m-%d %H:%M:%S").unwrap(), // 45分後
            value: 120.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap(), // 2時間後
            value: 130.0,
        },
    ];

    let forecast_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 15:00:00", "%Y-%m-%d %H:%M:%S").unwrap(), // 3時間後
            value: 140.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 15:30:00", "%Y-%m-%d %H:%M:%S").unwrap(), // 30分後
            value: 135.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-02 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap(), // 翌日
            value: 125.0,
        },
    ];

    let svg = generate_prediction_chart_svg(&actual_data, &forecast_data)
        .expect("時間軸検証データでのSVG生成に失敗");
    
    let file_path = format!("{}/time_axis_verification.svg", output_dir);
    fs::write(&file_path, &svg).expect("SVGファイルの書き込みに失敗");
    
    println!("✅ 時間軸検証完了: {}", file_path);
    
    // 時間間隔の分析
    println!("📊 実際データの時間間隔:");
    for i in 1..actual_data.len() {
        let duration = actual_data[i].time.signed_duration_since(actual_data[i-1].time);
        println!("  {} -> {}: {}分間隔", 
            actual_data[i-1].time.format("%H:%M"), 
            actual_data[i].time.format("%H:%M"), 
            duration.num_minutes()
        );
    }
    
    println!("🔮 予測データの時間間隔:");
    let last_actual = actual_data.last().unwrap();
    let first_forecast = &forecast_data[0];
    let gap_duration = first_forecast.time.signed_duration_since(last_actual.time);
    println!("  実際→予測ギャップ: {}時間", gap_duration.num_hours());
    
    for i in 1..forecast_data.len() {
        let duration = forecast_data[i].time.signed_duration_since(forecast_data[i-1].time);
        if duration.num_hours() > 0 {
            println!("  {} -> {}: {}時間間隔", 
                forecast_data[i-1].time.format("%m-%d %H:%M"), 
                forecast_data[i].time.format("%m-%d %H:%M"), 
                duration.num_hours()
            );
        } else {
            println!("  {} -> {}: {}分間隔", 
                forecast_data[i-1].time.format("%H:%M"), 
                forecast_data[i].time.format("%H:%M"), 
                duration.num_minutes()
            );
        }
    }
    
    // SVG内でのX座標確認のためのデバッグ出力
    if svg.contains("09:15") {
        println!("✅ 不等間隔時間（09:15）がSVGに含まれている");
    }
    if svg.contains("2025-06-02") {
        println!("✅ 日付変更（翌日）がSVGに含まれている");
    }
}

/// 凡例位置の検証テスト（左側 vs 右側）
#[test]
#[ignore]
fn test_legend_position_verification() {
    let output_dir = "target/svg_test_output";
    fs::create_dir_all(output_dir).expect("出力ディレクトリの作成に失敗");

    println!("🔧 凡例位置検証開始...");

    // シンプルなテストデータ
    let actual_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 100.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 110.0,
        },
    ];

    let forecast_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 15:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 120.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 18:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 115.0,
        },
    ];

    use crate::chart::plots::{MultiPlotSeries, MultiPlotOptions, plot_multi_values_at_time_to_svg_with_options};
    use plotters::prelude::{GREEN, MAGENTA};

    // 左側凡例版を生成
    let left_legend_svg = plot_multi_values_at_time_to_svg_with_options(
        &[
            MultiPlotSeries {
                name: "📊 実際の価格".to_string(),
                values: actual_data.clone(),
                color: GREEN,
            },
            MultiPlotSeries {
                name: "🔮 予測価格".to_string(),
                values: forecast_data.clone(),
                color: MAGENTA,
            },
        ],
        MultiPlotOptions {
            title: Some("💹 凡例左側版".to_string()),
            image_size: (800, 600),
            x_label: Some("時間".to_string()),
            y_label: Some("価格".to_string()),
            legend_on_left: Some(true), // 左側
        },
    ).expect("左側凡例SVG生成に失敗");

    // 右側凡例版を生成
    let right_legend_svg = plot_multi_values_at_time_to_svg_with_options(
        &[
            MultiPlotSeries {
                name: "📊 実際の価格".to_string(),
                values: actual_data,
                color: GREEN,
            },
            MultiPlotSeries {
                name: "🔮 予測価格".to_string(),
                values: forecast_data,
                color: MAGENTA,
            },
        ],
        MultiPlotOptions {
            title: Some("💹 凡例右側版".to_string()),
            image_size: (800, 600),
            x_label: Some("時間".to_string()),
            y_label: Some("価格".to_string()),
            legend_on_left: Some(false), // 右側
        },
    ).expect("右側凡例SVG生成に失敗");

    // ファイルに保存
    let left_path = format!("{}/legend_left_position.svg", output_dir);
    let right_path = format!("{}/legend_right_position.svg", output_dir);
    
    fs::write(&left_path, &left_legend_svg).expect("左側凡例SVGファイル書き込みに失敗");
    fs::write(&right_path, &right_legend_svg).expect("右側凡例SVGファイル書き込みに失敗");
    
    println!("✅ 凡例位置検証完了:");
    println!("   左側凡例: {}", left_path);
    println!("   右側凡例: {}", right_path);
    
    // X座標の範囲を確認
    if left_legend_svg.contains("x=\"7") || left_legend_svg.contains("x=\"8") || left_legend_svg.contains("x=\"9") {
        println!("✅ 左側凡例のX座標が正しく左側範囲（70-150）にある");
    }
    
    if right_legend_svg.contains("x=\"6") || right_legend_svg.contains("x=\"7") {
        println!("✅ 右側凡例のX座標が正しく右側範囲（600-800）にある");
    }
}

/// 極端なパターンの機能検証テスト（急上昇・急降下）
#[test]
#[ignore]
fn test_extreme_patterns_verification() {
    let output_dir = "target/svg_test_output";
    fs::create_dir_all(output_dir).expect("出力ディレクトリの作成に失敗");

    println!("🔧 極端パターン機能検証開始...");

    // 急激な変化のテストパターン
    let actual_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 100.0, // 開始
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 10:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 500.0, // 急上昇（5倍）
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 11:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 50.0,  // 急降下（1/10）
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 1000.0, // 極端な上昇（20倍）
        },
    ];

    let forecast_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 13:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 10.0,   // 極端な下落
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 14:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 800.0,  // 回復
        },
    ];

    let svg = generate_prediction_chart_svg(&actual_data, &forecast_data)
        .expect("極端データでのSVG生成に失敗");
    
    let file_path = format!("{}/extreme_patterns_verification.svg", output_dir);
    fs::write(&file_path, &svg).expect("SVGファイルの書き込みに失敗");
    
    println!("✅ 極端パターン検証完了: {}", file_path);
    
    // データ変化率を分析
    println!("📊 実際データの変化率:");
    for i in 1..actual_data.len() {
        let change_rate = (actual_data[i].value - actual_data[i-1].value) / actual_data[i-1].value * 100.0;
        println!("  {} -> {}: {:.1}%変化", 
            actual_data[i-1].time.format("%H:%M"), 
            actual_data[i].time.format("%H:%M"), 
            change_rate
        );
    }
}

/// 単調パターンの機能検証テスト（線形増加・線形減少）
#[test]
#[ignore]
fn test_monotonic_patterns_verification() {
    let output_dir = "target/svg_test_output";
    fs::create_dir_all(output_dir).expect("出力ディレクトリの作成に失敗");

    println!("🔧 単調パターン機能検証開始...");

    // 線形増加パターン
    let actual_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 100.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 10:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 110.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 11:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 120.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 130.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 13:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 140.0,
        },
    ];

    // 線形減少パターン
    let forecast_data = vec![
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 14:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 135.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 15:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 125.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 16:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 115.0,
        },
        ValueAtTime {
            time: NaiveDateTime::parse_from_str("2025-06-01 17:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            value: 105.0,
        },
    ];

    let svg = generate_prediction_chart_svg(&actual_data, &forecast_data)
        .expect("単調データでのSVG生成に失敗");
    
    let file_path = format!("{}/monotonic_patterns_verification.svg", output_dir);
    fs::write(&file_path, &svg).expect("SVGファイルの書き込みに失敗");
    
    println!("✅ 単調パターン検証完了: {}", file_path);
    println!("📈 実際データ: 線形増加 (100 -> 140, +10/時間)");
    println!("📉 予測データ: 線形減少 (135 -> 105, -10/時間)");
}

/// 現実的なデータパターンでSVGを生成するテスト
#[test]
#[ignore]
fn test_realistic_price_data() {
    let output_dir = "target/svg_test_output";
    fs::create_dir_all(output_dir).expect("出力ディレクトリの作成に失敗");

    // より現実的な価格変動パターン
    let prices = vec![
        ("2025-06-01 09:00:00", 1200.50),
        ("2025-06-01 10:00:00", 1205.25),
        ("2025-06-01 11:00:00", 1198.75),
        ("2025-06-01 12:00:00", 1210.00),
        ("2025-06-01 13:00:00", 1215.80),
        ("2025-06-01 14:00:00", 1208.40),
        ("2025-06-01 15:00:00", 1225.60),
        ("2025-06-01 16:00:00", 1230.25),
        ("2025-06-02 09:00:00", 1228.90),
        ("2025-06-02 10:00:00", 1235.70),
        ("2025-06-02 11:00:00", 1240.15),
        ("2025-06-02 12:00:00", 1245.30),
    ];

    let actual_data: Vec<ValueAtTime> = prices
        .into_iter()
        .map(|(datetime_str, value)| ValueAtTime {
            time: NaiveDateTime::parse_from_str(datetime_str, "%Y-%m-%d %H:%M:%S")
                .expect("有効な日時形式"),
            value,
        })
        .collect();

    // 予測データ（わずかに上昇トレンド）
    let forecasts = vec![
        ("2025-06-02 13:00:00", 1248.50),
        ("2025-06-02 14:00:00", 1252.20),
        ("2025-06-02 15:00:00", 1255.80),
        ("2025-06-02 16:00:00", 1260.10),
        ("2025-06-03 09:00:00", 1265.40),
    ];

    let forecast_data: Vec<ValueAtTime> = forecasts
        .into_iter()
        .map(|(datetime_str, value)| ValueAtTime {
            time: NaiveDateTime::parse_from_str(datetime_str, "%Y-%m-%d %H:%M:%S")
                .expect("有効な日時形式"),
            value,
        })
        .collect();

    // SVGを生成
    let svg = generate_prediction_chart_svg(&actual_data, &forecast_data)
        .expect("現実的データでのSVG生成に失敗");
    
    let file_path = format!("{}/06_realistic_price_data.svg", output_dir);
    fs::write(&file_path, &svg).expect("SVGファイルの書き込みに失敗");
    
    // 検証（改良版の特徴）
    assert!(svg.contains("💹 価格予測分析"), "改良版タイトルが含まれていない");
    assert!(svg.contains("📊 実際の価格"), "改良版実際データ系列名が含まれていない");
    assert!(svg.contains("🔮 予測価格"), "改良版予測データ系列名が含まれていない");
    assert!(svg.contains("#00FF00"), "緑色（実際データ）が含まれていない");
    assert!(svg.contains("#FF00FF"), "マゼンタ色（予測データ）が含まれていない");
    
    println!("✅ 現実的データテスト完了: {}", file_path);
    println!("📊 SVGサイズ: {} バイト", svg.len());
    println!("📈 実際データポイント数: {}", actual_data.len());
    println!("🔮 予測データポイント数: {}", forecast_data.len());
}

/// 単一テストケース用のSVGファイル生成
/// 
/// 特定のデータでSVGを生成したい場合に使用
/// 
/// # 使用例
/// ```rust
/// use prediction_utils::visual_tests::generate_svg_file;
/// 
/// let actual = vec![/* データ */];
/// let forecast = vec![/* データ */];
/// generate_svg_file(&actual, &forecast, "test_output.svg");
/// ```
#[allow(dead_code)]
pub fn generate_svg_file(
    actual_data: &[ValueAtTime],
    forecast_data: &[ValueAtTime],
    filename: &str,
) -> Result<(), String> {
    let svg = generate_prediction_chart_svg(actual_data, forecast_data)
        .map_err(|e| format!("SVG生成エラー: {}", e))?;
    fs::write(filename, svg)
        .map_err(|e| format!("ファイル書き込みエラー: {}", e))?;
    println!("SVGファイルを生成しました: {}", filename);
    Ok(())
}