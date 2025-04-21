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
    // NaiveDateTimeをUTC DateTimeに変換
    let to_datetime = |ndt: NaiveDateTime| -> DateTime<Utc> {
        DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc)
    };

    let min_time = to_datetime(values.iter().map(|v| v.time).min().unwrap());
    let max_time = to_datetime(values.iter().map(|v| v.time).max().unwrap());
    let min_value = values
        .iter()
        .map(|v| v.value)
        .fold(f64::INFINITY, |a, b| a.min(b));
    let max_value = values
        .iter()
        .map(|v| v.value)
        .fold(f64::NEG_INFINITY, |a, b| a.max(b));

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
    let mesh_with_formatters = mesh_with_x_formatter.y_label_formatter(&|y| format!("{:.2}", y));

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

// ========== パブリックAPI関数群 ==========

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
