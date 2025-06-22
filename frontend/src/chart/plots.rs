#![allow(dead_code)]

use anyhow::Result;
use chrono::{DateTime, NaiveDateTime, Utc};
use plotters::coord::Shift;
use plotters::prelude::*;
use std::string::String;
use zaciraci_common::stats::ValueAtTime;

/// プロットオプションを定義する構造体
#[derive(Debug, Clone)]
pub struct PlotOptions {
    /// 画像サイズ
    pub image_size: (u32, u32),
    /// タイトル
    pub title: Option<String>,
    /// X軸ラベル
    pub x_label: Option<String>,
    /// Y軸ラベル
    pub y_label: Option<String>,
    /// 線の色
    pub line_color: RGBColor,
}

/// 複数データセット用のプロットシリーズ
#[derive(Debug, Clone)]
pub struct MultiPlotSeries {
    /// データ値
    pub values: Vec<ValueAtTime>,
    /// 系列名（凡例表示用）
    pub name: String,
    /// 線の色
    pub color: RGBColor,
}

/// 複数データセット用のプロットオプション
#[derive(Debug, Clone)]
pub struct MultiPlotOptions {
    /// 画像サイズ
    pub image_size: (u32, u32),
    /// タイトル
    pub title: Option<String>,
    /// X軸ラベル
    pub x_label: Option<String>,
    /// Y軸ラベル
    pub y_label: Option<String>,
    /// 凡例を左側に表示するかどうか
    pub legend_on_left: Option<bool>,
}

impl Default for MultiPlotOptions {
    fn default() -> Self {
        Self {
            image_size: (800, 600),
            title: None,
            x_label: None,
            y_label: None,
            legend_on_left: None,
        }
    }
}

impl Default for PlotOptions {
    fn default() -> Self {
        Self {
            image_size: (800, 600),
            title: None,
            x_label: None,
            y_label: None,
            line_color: BLUE,
        }
    }
}

/// バックエンドの種類を表す列挙型
enum BackendType {
    /// メモリ上にPNG形式で保持
    Memory,
    /// メモリ上にSVG形式のテキストで保持
    Svg,
}

/// プロットの結果を表す列挙型
#[derive(Debug)]
enum PlotResult {
    /// メモリ上のPNGの場合はVec<u8>
    Memory(Vec<u8>),
    /// SVGの場合はString
    Svg(String),
}

impl From<PlotResult> for Vec<u8> {
    fn from(result: PlotResult) -> Self {
        match result {
            PlotResult::Memory(data) => data,
            _ => panic!("Cannot convert SVG result to Vec<u8>"),
        }
    }
}

impl From<PlotResult> for String {
    fn from(result: PlotResult) -> Self {
        match result {
            PlotResult::Svg(data) => data,
            _ => panic!("Cannot convert Memory result to String"),
        }
    }
}

/// NaiveDateTimeをUTC DateTimeに変換する
pub(crate) fn to_datetime(ndt: NaiveDateTime) -> DateTime<Utc> {
    DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc)
}

/// データの時間範囲と値の範囲を計算する
///
/// # 戻り値
/// (最小時間, 最大時間, 最小値, 最大値) のタプル
///
/// # Panics
/// 空のデータセットが渡された場合にパニックします
pub(crate) fn calculate_data_ranges(
    values: &[ValueAtTime],
) -> (DateTime<Utc>, DateTime<Utc>, f64, f64) {
    if values.is_empty() {
        // 空データの場合のデフォルト値を返す
        let now =
            DateTime::<Utc>::from_naive_utc_and_offset(chrono::Utc::now().naive_utc(), chrono::Utc);
        return (now, now, 0.0, 1.0);
    }

    let min_time = values
        .iter()
        .map(|v| to_datetime(v.time))
        .min()
        .unwrap_or_else(|| {
            DateTime::<Utc>::from_naive_utc_and_offset(chrono::Utc::now().naive_utc(), chrono::Utc)
        });
    let max_time = values
        .iter()
        .map(|v| to_datetime(v.time))
        .max()
        .unwrap_or_else(|| {
            DateTime::<Utc>::from_naive_utc_and_offset(chrono::Utc::now().naive_utc(), chrono::Utc)
        });
    let min_value = values
        .iter()
        .map(|v| v.value)
        .fold(f64::INFINITY, |a, b| a.min(b));
    let max_value = values
        .iter()
        .map(|v| v.value)
        .fold(f64::NEG_INFINITY, |a, b| a.max(b));

    (min_time, max_time, min_value, max_value)
}

/// 内部共通関数: ValueAtTimeのプロットを行う
fn plot_values_at_time_internal(
    values: &[ValueAtTime],
    backend_type: BackendType,
    options: &PlotOptions,
) -> Result<PlotResult> {
    // 空のデータチェック
    if values.is_empty() {
        return Err(anyhow::anyhow!("空のデータセットではプロットできません"));
    }

    // バックエンドと描画領域を作成
    let result = match backend_type {
        BackendType::Memory => {
            let mut buffer = vec![];
            {
                let root =
                    BitMapBackend::with_buffer(&mut buffer, options.image_size).into_drawing_area();
                draw_plot(values, root, options)?;
            }
            PlotResult::Memory(buffer)
        }
        BackendType::Svg => {
            let mut buffer = String::new();
            {
                let root =
                    SVGBackend::with_string(&mut buffer, options.image_size).into_drawing_area();
                draw_plot(values, root, options)?;
            }
            PlotResult::Svg(buffer)
        }
    };

    Ok(result)
}

/// 共通の描画処理
fn draw_plot<DB: DrawingBackend>(
    values: &[ValueAtTime],
    root: DrawingArea<DB, Shift>,
    options: &PlotOptions,
) -> Result<()> {
    // 背景色設定
    root.fill(&WHITE)
        .map_err(|e| anyhow::anyhow!("背景の描画に失敗しました: {}", e))?;

    // データ範囲の計算
    let (min_time, max_time, min_value, max_value) = calculate_data_ranges(values);

    // 値の範囲にマージンを追加
    // 最小値と最大値が同じ場合や非常に近い場合のための対策
    let value_range = max_value - min_value;
    let value_margin = if value_range.abs() < 1e-10 {
        // 値がほぼ一定の場合は絶対値の5%をマージンとして使用
        max_value.abs() * 0.05 + 0.1 // 少なくとも0.1の余白を確保
    } else {
        value_range * 0.05 // 通常は範囲の5%
    };

    let y_range = (min_value - value_margin)..(max_value + value_margin);

    // ChartBuilderの作成
    let mut builder_base = ChartBuilder::on(&root);
    let builder_margin = builder_base.margin(10);
    let builder_x_label = builder_margin.x_label_area_size(40);
    let builder_xy_label = builder_x_label.y_label_area_size(60);

    // タイトルの設定（オプショナル）
    let builder_with_title = if let Some(title) = &options.title {
        builder_xy_label.caption(title, ("sans-serif", 30).into_font())
    } else {
        builder_xy_label.caption("Value Over Time", ("sans-serif", 30).into_font())
    };

    // チャートの作成
    let mut chart = builder_with_title
        .build_cartesian_2d(min_time..max_time, y_range)
        .map_err(|e| anyhow::anyhow!("チャートの構築に失敗しました: {}", e))?;

    // 軸の設定
    let mut mesh_base = chart.configure_mesh();
    let mesh_x_labels = mesh_base.x_labels(10);
    let mesh_xy_labels = mesh_x_labels.y_labels(10);

    // デフォルト値となる文字列をあらかじめ変数に格納する
    let default_x_label = "Time".to_string();
    let default_y_label = "Value".to_string();

    // 文字列の参照を取得
    let x_label = options.x_label.as_ref().unwrap_or(&default_x_label);
    let y_label = options.y_label.as_ref().unwrap_or(&default_y_label);

    let mesh_with_x_desc = mesh_xy_labels.x_desc(x_label);
    let mesh_with_xy_desc = mesh_with_x_desc.y_desc(y_label);

    let mesh_with_x_formatter =
        mesh_with_xy_desc.x_label_formatter(&|dt| dt.format("%Y-%m-%d %H:%M").to_string());
    let mesh_with_formatters = mesh_with_x_formatter.y_label_formatter(&format_value);

    mesh_with_formatters
        .draw()
        .map_err(|e| anyhow::anyhow!("軸の描画に失敗しました: {}", e))?;

    // データのプロット
    chart
        .draw_series(LineSeries::new(
            values.iter().map(|v| (to_datetime(v.time), v.value)),
            options.line_color,
        ))
        .map_err(|e| anyhow::anyhow!("データのプロットに失敗しました: {}", e))?
        .label("Values")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], options.line_color));

    // 凡例の描画
    chart
        .configure_series_labels()
        .background_style(WHITE.mix(0.8))
        .border_style(BLACK)
        .draw()
        .map_err(|e| anyhow::anyhow!("凡例の描画に失敗しました: {}", e))?;

    // ドローイングエリアの最終処理
    root.present()
        .map_err(|e| anyhow::anyhow!("画像の完成に失敗しました: {}", e))?;

    Ok(())
}

/// 複数データセットを同一チャートに描画する共通処理
fn draw_multi_plot<DB: DrawingBackend>(
    series: &[MultiPlotSeries],
    root: DrawingArea<DB, Shift>,
    options: &MultiPlotOptions,
) -> Result<()> {
    // 背景色設定
    root.fill(&WHITE)
        .map_err(|e| anyhow::anyhow!("背景の描画に失敗しました: {}", e))?;

    // 空のデータチェック
    if series.is_empty() {
        return Err(anyhow::anyhow!("空のデータセットではプロットできません"));
    }

    // 各系列のデータが空ではないことを確認
    for (i, s) in series.iter().enumerate() {
        if s.values.is_empty() {
            return Err(anyhow::anyhow!("系列 {} のデータが空です", i));
        }
    }

    // 全データの範囲を計算
    // 最初の系列から初期値を取得
    let first_series_range = calculate_data_ranges(&series[0].values);
    let mut min_time = first_series_range.0;
    let mut max_time = first_series_range.1;
    let mut min_value = first_series_range.2;
    let mut max_value = first_series_range.3;

    // 残りの系列で範囲を更新
    for s in &series[1..] {
        let (s_min_time, s_max_time, s_min_value, s_max_value) = calculate_data_ranges(&s.values);
        min_time = min_time.min(s_min_time);
        max_time = max_time.max(s_max_time);
        min_value = min_value.min(s_min_value);
        max_value = max_value.max(s_max_value);
    }

    // 値の範囲にマージンを追加
    let value_range = max_value - min_value;
    let value_margin = if value_range.abs() < 1e-10 {
        // 値がほぼ一定の場合は絶対値の5%をマージンとして使用
        max_value.abs() * 0.05 + 0.1 // 少なくとも0.1の余白を確保
    } else {
        value_range * 0.05 // 通常は範囲の5%
    };

    let y_range = (min_value - value_margin)..(max_value + value_margin);

    // ChartBuilderの作成
    let mut builder_base = ChartBuilder::on(&root);
    let builder_margin = builder_base.margin(10);
    let builder_x_label = builder_margin.x_label_area_size(40);
    let builder_xy_label = builder_x_label.y_label_area_size(60);

    // タイトルの設定（オプショナル）
    let builder_with_title = if let Some(title) = &options.title {
        builder_xy_label.caption(title, ("sans-serif", 30).into_font())
    } else {
        builder_xy_label.caption("Values Over Time", ("sans-serif", 30).into_font())
    };

    // チャートの作成
    let mut chart = builder_with_title
        .build_cartesian_2d(min_time..max_time, y_range)
        .map_err(|e| anyhow::anyhow!("チャートの構築に失敗しました: {}", e))?;

    // 軸の設定
    let mut mesh_base = chart.configure_mesh();
    let mesh_x_labels = mesh_base.x_labels(10);
    let mesh_xy_labels = mesh_x_labels.y_labels(10);

    // デフォルト値となる文字列をあらかじめ変数に格納する
    let default_x_label = "Time".to_string();
    let default_y_label = "Value".to_string();

    // 文字列の参照を取得
    let x_label = options.x_label.as_ref().unwrap_or(&default_x_label);
    let y_label = options.y_label.as_ref().unwrap_or(&default_y_label);

    let mesh_with_x_desc = mesh_xy_labels.x_desc(x_label);
    let mesh_with_xy_desc = mesh_with_x_desc.y_desc(y_label);

    let mesh_with_x_formatter =
        mesh_with_xy_desc.x_label_formatter(&|dt| dt.format("%Y-%m-%d %H:%M").to_string());
    let mesh_with_formatters = mesh_with_x_formatter.y_label_formatter(&format_value);

    mesh_with_formatters
        .draw()
        .map_err(|e| anyhow::anyhow!("メッシュの描画に失敗しました: {}", e))?;

    // 各系列のデータをプロット
    for series_data in series {
        // NaiveDateTimeをDateTimeに変換
        let datetime_values: Vec<(DateTime<Utc>, f64)> = series_data
            .values
            .iter()
            .map(|v| (to_datetime(v.time), v.value))
            .collect();

        // 系列をプロット
        chart
            .draw_series(LineSeries::new(datetime_values, series_data.color))
            .map_err(|e| anyhow::anyhow!("データのプロットに失敗しました: {}", e))?
            .label(&series_data.name)
            .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], series_data.color));
    }

    // 凡例の描画
    if options.legend_on_left.unwrap_or(false) {
        // 左側に凡例を配置
        chart
            .configure_series_labels()
            .position(plotters::chart::SeriesLabelPosition::UpperLeft)
            .background_style(WHITE.mix(0.8))
            .border_style(BLACK)
            .draw()
            .map_err(|e| anyhow::anyhow!("凡例の描画に失敗しました: {}", e))?;
    } else {
        // デフォルトは右側
        chart
            .configure_series_labels()
            .position(plotters::chart::SeriesLabelPosition::UpperRight)
            .background_style(WHITE.mix(0.8))
            .border_style(BLACK)
            .draw()
            .map_err(|e| anyhow::anyhow!("凡例の描画に失敗しました: {}", e))?;
    }

    // ドローイングエリアの最終処理
    root.present()
        .map_err(|e| anyhow::anyhow!("画像の完成に失敗しました: {}", e))?;

    Ok(())
}

/// 複数データセットを同一チャートに描画する内部共通関数
fn plot_multi_values_at_time_internal(
    series: &[MultiPlotSeries],
    backend_type: BackendType,
    options: &MultiPlotOptions,
) -> Result<PlotResult> {
    // 空のデータチェック
    if series.is_empty() {
        return Err(anyhow::anyhow!("空のデータセットではプロットできません"));
    }

    // バックエンドと描画領域を作成
    let result = match backend_type {
        BackendType::Memory => {
            let mut buffer = vec![];
            {
                let root =
                    BitMapBackend::with_buffer(&mut buffer, options.image_size).into_drawing_area();
                draw_multi_plot(series, root, options)?;
            }
            PlotResult::Memory(buffer)
        }
        BackendType::Svg => {
            let mut buffer = String::new();
            {
                let root =
                    SVGBackend::with_string(&mut buffer, options.image_size).into_drawing_area();
                draw_multi_plot(series, root, options)?;
            }
            PlotResult::Svg(buffer)
        }
    };

    Ok(result)
}

/*
 * パブリックAPI関数群
 */

/// ValueAtTimeのリストをメモリ上にプロットし、PNG画像データを返す
pub fn plot_values_at_time_to_memory(values: &[ValueAtTime], size: (u32, u32)) -> Result<Vec<u8>> {
    let options = PlotOptions {
        image_size: size,
        ..Default::default()
    };
    plot_values_at_time_to_memory_with_options(values, options)
}

/// オプション付きでValueAtTimeのリストをメモリ上にプロットし、PNG画像データを返す
pub fn plot_values_at_time_to_memory_with_options(
    values: &[ValueAtTime],
    options: PlotOptions,
) -> Result<Vec<u8>> {
    let result = plot_values_at_time_internal(values, BackendType::Memory, &options)?;
    Ok(result.into())
}

/// ValueAtTimeのリストをメモリ上にSVGとしてプロットし、SVGデータを返す
pub fn plot_values_at_time_to_svg(values: &[ValueAtTime], size: (u32, u32)) -> Result<String> {
    let options = PlotOptions {
        image_size: size,
        ..Default::default()
    };
    plot_values_at_time_to_svg_with_options(values, options)
}

/// オプション付きでValueAtTimeのリストをメモリ上にSVGとしてプロットし、SVGデータを返す
pub fn plot_values_at_time_to_svg_with_options(
    values: &[ValueAtTime],
    options: PlotOptions,
) -> Result<String> {
    let result = plot_values_at_time_internal(values, BackendType::Svg, &options)?;
    Ok(result.into())
}

/// 複数のデータセットを同一チャートにSVGとしてプロットし、SVGデータを返す
pub fn plot_multi_values_at_time_to_svg(
    series: &[MultiPlotSeries],
    size: (u32, u32),
) -> Result<String> {
    let options = MultiPlotOptions {
        image_size: size,
        ..Default::default()
    };
    plot_multi_values_at_time_to_svg_with_options(series, options)
}

/// オプション付きで複数のデータセットを同一チャートにSVGとしてプロットし、SVGデータを返す
pub fn plot_multi_values_at_time_to_svg_with_options(
    series: &[MultiPlotSeries],
    options: MultiPlotOptions,
) -> Result<String> {
    let result = plot_multi_values_at_time_internal(series, BackendType::Svg, &options)?;
    Ok(result.into())
}

/// 複数のデータセットを同一チャートにPNGとしてプロットし、PNG画像データを返す
pub fn plot_multi_values_at_time_to_memory(
    series: &[MultiPlotSeries],
    size: (u32, u32),
) -> Result<Vec<u8>> {
    let options = MultiPlotOptions {
        image_size: size,
        ..Default::default()
    };
    plot_multi_values_at_time_to_memory_with_options(series, options)
}

/// オプション付きで複数のデータセットを同一チャートにPNGとしてプロットし、PNG画像データを返す
pub fn plot_multi_values_at_time_to_memory_with_options(
    series: &[MultiPlotSeries],
    options: MultiPlotOptions,
) -> Result<Vec<u8>> {
    let result = plot_multi_values_at_time_internal(series, BackendType::Memory, &options)?;
    Ok(result.into())
}

/// 数値を適切な単位付きの文字列に変換する
///
/// # 例
/// ```
/// assert_eq!(format_value(1500.0), "1.50K");
/// assert_eq!(format_value(0.001), "1.00e-3");
/// ```
pub(crate) fn format_value(y: &f64) -> String {
    // 大きな数値やさまざまな桁数に対応するためのフォーマット
    if y.abs() >= 1_000_000_000_000.0 {
        // 1兆以上なら「T」を使用
        format!("{:.2}T", y / 1_000_000_000_000.0)
    } else if y.abs() >= 1_000_000_000.0 {
        // 10億以上なら「G」を使用
        format!("{:.2}G", y / 1_000_000_000.0)
    } else if y.abs() >= 1_000_000.0 {
        // 100万以上なら「M」を使用
        format!("{:.2}M", y / 1_000_000.0)
    } else if y.abs() >= 1_000.0 {
        // 1000以上なら「K」を使用
        format!("{:.2}K", y / 1_000.0)
    } else if y.abs() < 0.01 && y.abs() > 0.0 {
        // 非常に小さい数値の場合は科学的表記法
        format!("{:.2e}", y)
    } else {
        // 通常のケース
        format!("{:.2}", y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, NaiveTime};

    #[test]
    fn test_format_value_trillion() {
        // 1兆以上の値
        assert_eq!(format_value(&1_500_000_000_000.0), "1.50T");
        assert_eq!(format_value(&1_000_000_000_000.0), "1.00T");
        assert_eq!(format_value(&-2_345_000_000_000.0), "-2.35T");
    }

    #[test]
    fn test_format_value_billion() {
        // 10億以上の値
        assert_eq!(format_value(&1_500_000_000.0), "1.50G");
        assert_eq!(format_value(&1_000_000_000.0), "1.00G");
        assert_eq!(format_value(&-2_345_000_000.0), "-2.35G");
    }

    #[test]
    fn test_format_value_million() {
        // 100万以上の値
        assert_eq!(format_value(&1_500_000.0), "1.50M");
        assert_eq!(format_value(&1_000_000.0), "1.00M");
        assert_eq!(format_value(&-2_345_000.0), "-2.35M");
    }

    #[test]
    fn test_format_value_thousand() {
        // 1000以上の値
        assert_eq!(format_value(&1_500.0), "1.50K");
        assert_eq!(format_value(&1_000.0), "1.00K");
        assert_eq!(format_value(&-2_345.0), "-2.35K");
    }

    #[test]
    fn test_format_value_small() {
        // 非常に小さい値（科学的表記法）
        assert_eq!(format_value(&0.001), "1.00e-3");
        assert_eq!(format_value(&-0.00012), "-1.20e-4");
        assert_eq!(format_value(&0.0000123), "1.23e-5");
    }

    #[test]
    fn test_format_value_normal() {
        // 通常の値
        assert_eq!(format_value(&123.456), "123.46");
        assert_eq!(format_value(&-42.42), "-42.42");
        assert_eq!(format_value(&0.123), "0.12");
        assert_eq!(format_value(&0.0), "0.00");
    }

    #[test]
    fn test_calculate_data_ranges_normal() {
        // 通常のデータセット
        let values = vec![
            ValueAtTime {
                time: NaiveDate::from_ymd_opt(2023, 1, 1)
                    .unwrap()
                    .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                value: 100.0,
            },
            ValueAtTime {
                time: NaiveDate::from_ymd_opt(2023, 1, 2)
                    .unwrap()
                    .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                value: 200.0,
            },
            ValueAtTime {
                time: NaiveDate::from_ymd_opt(2023, 1, 3)
                    .unwrap()
                    .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                value: 150.0,
            },
        ];

        let (min_time, max_time, min_value, max_value) = calculate_data_ranges(&values);

        // 時間の範囲検証
        assert_eq!(min_time, to_datetime(values[0].time));
        assert_eq!(max_time, to_datetime(values[2].time));

        // 値の範囲検証
        assert_eq!(min_value, 100.0);
        assert_eq!(max_value, 200.0);
    }

    #[test]
    fn test_calculate_data_ranges_single() {
        // 単一要素のデータセット
        let values = vec![ValueAtTime {
            time: NaiveDate::from_ymd_opt(2023, 1, 1)
                .unwrap()
                .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
            value: 100.0,
        }];

        let (min_time, max_time, min_value, max_value) = calculate_data_ranges(&values);

        // 単一要素なので最小値と最大値は同じになる
        assert_eq!(min_time, to_datetime(values[0].time));
        assert_eq!(max_time, to_datetime(values[0].time));
        assert_eq!(min_value, 100.0);
        assert_eq!(max_value, 100.0);
    }

    #[test]
    fn test_calculate_data_ranges_same_values() {
        // すべての値が同じデータセット
        let values = vec![
            ValueAtTime {
                time: NaiveDate::from_ymd_opt(2023, 1, 1)
                    .unwrap()
                    .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                value: 100.0,
            },
            ValueAtTime {
                time: NaiveDate::from_ymd_opt(2023, 1, 2)
                    .unwrap()
                    .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                value: 100.0,
            },
            ValueAtTime {
                time: NaiveDate::from_ymd_opt(2023, 1, 3)
                    .unwrap()
                    .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                value: 100.0,
            },
        ];

        let (min_time, max_time, min_value, max_value) = calculate_data_ranges(&values);

        // 時間の範囲検証
        assert_eq!(min_time, to_datetime(values[0].time));
        assert_eq!(max_time, to_datetime(values[2].time));

        // すべての値が同じ
        assert_eq!(min_value, 100.0);
        assert_eq!(max_value, 100.0);
    }

    #[test]
    fn test_calculate_data_ranges_extreme_values() {
        // 極端な値を含むデータセット
        let values = vec![
            ValueAtTime {
                time: NaiveDate::from_ymd_opt(2023, 1, 1)
                    .unwrap()
                    .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                value: -1_000_000.0,
            },
            ValueAtTime {
                time: NaiveDate::from_ymd_opt(2023, 1, 2)
                    .unwrap()
                    .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                value: 0.0,
            },
            ValueAtTime {
                time: NaiveDate::from_ymd_opt(2023, 1, 3)
                    .unwrap()
                    .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                value: 1_000_000.0,
            },
        ];

        let (min_time, max_time, min_value, max_value) = calculate_data_ranges(&values);

        // 時間の範囲検証
        assert_eq!(min_time, to_datetime(values[0].time));
        assert_eq!(max_time, to_datetime(values[2].time));

        // 極端な値の検証
        assert_eq!(min_value, -1_000_000.0);
        assert_eq!(max_value, 1_000_000.0);
    }

    #[test]
    fn test_calculate_data_ranges_empty() {
        // 空のデータセットはデフォルト値を返すはず
        let empty_values: Vec<ValueAtTime> = vec![];
        let (min_time, max_time, min_value, max_value) = calculate_data_ranges(&empty_values);

        // デフォルト値の確認
        assert_eq!(min_time, max_time); // 同じ時刻
        assert_eq!(min_value, 0.0);
        assert_eq!(max_value, 1.0);
    }

    #[test]
    fn test_plot_multi_values_at_time_internal_empty_series() {
        // 空の系列配列
        let empty_series: Vec<MultiPlotSeries> = vec![];
        let options = MultiPlotOptions::default();

        let result = plot_multi_values_at_time_internal(&empty_series, BackendType::Svg, &options);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("空のデータセット"));
    }

    #[test]
    fn test_plot_multi_values_at_time_internal_empty_data_in_series() {
        // 系列内のデータが空の場合
        let series = vec![MultiPlotSeries {
            values: vec![], // 空のデータ
            name: "Empty Series".to_string(),
            color: plotters::prelude::BLUE,
        }];
        let options = MultiPlotOptions::default();

        let result = plot_multi_values_at_time_internal(&series, BackendType::Svg, &options);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("系列 0 のデータが空")
        );
    }

    #[test]
    fn test_plot_multi_values_at_time_internal_single_series() {
        // 単一系列の正常ケース
        let series = vec![MultiPlotSeries {
            values: vec![
                ValueAtTime {
                    time: NaiveDate::from_ymd_opt(2023, 1, 1)
                        .unwrap()
                        .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                    value: 100.0,
                },
                ValueAtTime {
                    time: NaiveDate::from_ymd_opt(2023, 1, 2)
                        .unwrap()
                        .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                    value: 200.0,
                },
            ],
            name: "Series 1".to_string(),
            color: plotters::prelude::BLUE,
        }];
        let options = MultiPlotOptions::default();

        let result = plot_multi_values_at_time_internal(&series, BackendType::Svg, &options);
        assert!(result.is_ok());

        // SVG出力の基本検証
        if let Ok(PlotResult::Svg(svg_content)) = result {
            assert!(svg_content.contains("<svg"));
            assert!(svg_content.contains("</svg>"));
            assert!(svg_content.contains("Series 1"));
        }
    }

    #[test]
    fn test_plot_multi_values_at_time_internal_multi_series() {
        // 複数系列の正常ケース
        let series = vec![
            MultiPlotSeries {
                values: vec![
                    ValueAtTime {
                        time: NaiveDate::from_ymd_opt(2023, 1, 1)
                            .unwrap()
                            .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                        value: 100.0,
                    },
                    ValueAtTime {
                        time: NaiveDate::from_ymd_opt(2023, 1, 2)
                            .unwrap()
                            .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                        value: 200.0,
                    },
                ],
                name: "Series 1".to_string(),
                color: plotters::prelude::BLUE,
            },
            MultiPlotSeries {
                values: vec![
                    ValueAtTime {
                        time: NaiveDate::from_ymd_opt(2023, 1, 1)
                            .unwrap()
                            .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                        value: 150.0,
                    },
                    ValueAtTime {
                        time: NaiveDate::from_ymd_opt(2023, 1, 2)
                            .unwrap()
                            .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                        value: 250.0,
                    },
                ],
                name: "Series 2".to_string(),
                color: plotters::prelude::RED,
            },
        ];
        let options = MultiPlotOptions::default();

        let result = plot_multi_values_at_time_internal(&series, BackendType::Svg, &options);
        assert!(result.is_ok());

        // SVG出力の複数系列検証
        if let Ok(PlotResult::Svg(svg_content)) = result {
            assert!(svg_content.contains("<svg"));
            assert!(svg_content.contains("</svg>"));
            assert!(svg_content.contains("Series 1"));
            assert!(svg_content.contains("Series 2"));
        }
    }

    #[test]
    fn test_plot_multi_values_at_time_internal_svg_backend() {
        // SVGバックエンドの動作確認
        let series = vec![MultiPlotSeries {
            values: vec![
                ValueAtTime {
                    time: NaiveDate::from_ymd_opt(2023, 1, 1)
                        .unwrap()
                        .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                    value: 100.0,
                },
                ValueAtTime {
                    time: NaiveDate::from_ymd_opt(2023, 1, 2)
                        .unwrap()
                        .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                    value: 200.0,
                },
            ],
            name: "SVG Test".to_string(),
            color: plotters::prelude::GREEN,
        }];
        let options = MultiPlotOptions::default();

        let result = plot_multi_values_at_time_internal(&series, BackendType::Svg, &options);
        assert!(result.is_ok());

        // SVG出力の基本検証
        if let Ok(PlotResult::Svg(svg_content)) = result {
            assert!(svg_content.contains("<svg"));
            assert!(svg_content.contains("</svg>"));
            assert!(svg_content.contains("SVG Test"));
        }
    }

    #[test]
    fn test_plot_multi_values_at_time_internal_edge_cases() {
        // エッジケース: 単一ポイント
        let series = vec![MultiPlotSeries {
            values: vec![ValueAtTime {
                time: NaiveDate::from_ymd_opt(2023, 1, 1)
                    .unwrap()
                    .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                value: 100.0,
            }],
            name: "Single Point".to_string(),
            color: plotters::prelude::BLUE,
        }];
        let options = MultiPlotOptions::default();

        let result = plot_multi_values_at_time_internal(&series, BackendType::Svg, &options);
        assert!(result.is_ok());
    }

    #[test]
    fn test_plot_multi_values_at_time_internal_same_values() {
        // エッジケース: 同じ値の系列
        let series = vec![MultiPlotSeries {
            values: vec![
                ValueAtTime {
                    time: NaiveDate::from_ymd_opt(2023, 1, 1)
                        .unwrap()
                        .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                    value: 100.0,
                },
                ValueAtTime {
                    time: NaiveDate::from_ymd_opt(2023, 1, 2)
                        .unwrap()
                        .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                    value: 100.0,
                },
                ValueAtTime {
                    time: NaiveDate::from_ymd_opt(2023, 1, 3)
                        .unwrap()
                        .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                    value: 100.0,
                },
            ],
            name: "Constant Values".to_string(),
            color: plotters::prelude::BLUE,
        }];
        let options = MultiPlotOptions::default();

        let result = plot_multi_values_at_time_internal(&series, BackendType::Svg, &options);
        assert!(result.is_ok());
    }

    #[test]
    fn test_plot_multi_values_at_time_internal_extreme_values() {
        // エッジケース: 極端な値
        let series = vec![MultiPlotSeries {
            values: vec![
                ValueAtTime {
                    time: NaiveDate::from_ymd_opt(2023, 1, 1)
                        .unwrap()
                        .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                    value: -1_000_000.0,
                },
                ValueAtTime {
                    time: NaiveDate::from_ymd_opt(2023, 1, 2)
                        .unwrap()
                        .and_time(NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                    value: 1_000_000.0,
                },
            ],
            name: "Extreme Values".to_string(),
            color: plotters::prelude::BLUE,
        }];
        let options = MultiPlotOptions::default();

        let result = plot_multi_values_at_time_internal(&series, BackendType::Svg, &options);
        assert!(result.is_ok());
    }
}
