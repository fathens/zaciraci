use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use zaciraci_common::stats::ValueAtTime;

use crate::chart::plots::{
    MultiPlotOptions, MultiPlotSeries, plot_multi_values_at_time_to_svg_with_options,
};
use crate::chronos_api::predict::{ChronosApiClient, ZeroShotPredictionRequest};
use crate::data_normalization::DataNormalizer;
use crate::errors::PredictionError;
use crate::prediction_config::get_config;
use plotters::prelude::{BLUE, RED};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PredictionResult {
    pub predicted_price: f64,
    pub accuracy: f64,
    pub chart_svg: Option<String>,
    pub metrics: HashMap<String, f64>,
    pub forecast_data: Vec<ValueAtTime>,
}

/// ゼロショット予測を実行し、予測精度を検証する関数
/// 
/// この関数は未来予測ではなく、予測モデルの精度検証を目的としています：
/// - データを90:10に分割（90%を学習、10%をテスト）
/// - 学習データでモデルを訓練し、テストデータ期間の予測を実行
/// - 予測結果と実際のテストデータを比較して精度を評価
/// - チャートで予測と実際の差異を視覚的に確認
pub async fn execute_zero_shot_prediction(
    values_data: &[ValueAtTime],
    model_name: String,
    chronos_client: Arc<ChronosApiClient>,
) -> Result<PredictionResult, PredictionError> {
    // データの検証・正規化処理
    let normalized_data = validate_and_normalize_data(values_data)?;
    
    // データを90:10に分割
    let (training_data, test_data) = split_data_for_prediction(&normalized_data)?;
    
    // ZeroShotPredictionRequestの作成
    let prediction_request = create_prediction_request(training_data, test_data, model_name)?;

    // 予測実行
    match chronos_client.predict_zero_shot(&prediction_request).await {
        Ok(prediction_response) => {
            // 予測レスポンスの基本検証
            validate_prediction_response(
                &prediction_response.forecast_values,
                &prediction_response.forecast_timestamp,
            )?;

            // 予測結果とテストデータの比較
            let actual_values: Vec<_> = test_data.iter().map(|v| v.value).collect();
            let forecast_values = prediction_response.forecast_values;

            // 予測精度の計算
            let metrics = calculate_metrics(&actual_values, &forecast_values);

            // 予測データを変換・調整
            let forecast_data = transform_forecast_data(
                &forecast_values,
                &prediction_response.forecast_timestamp,
                &test_data, // test_dataを参照渡しに変更
            )?;

            // チャートSVGを生成
            let chart_svg = generate_prediction_chart_svg(&normalized_data, &forecast_data)?;

            // PredictionResultを作成
            Ok(create_prediction_result(forecast_data, chart_svg, metrics))
        }
        Err(e) => Err(PredictionError::PredictionFailed(format!(
            "予測実行エラー: {}",
            e
        ))),
    }
}

/// 予測レスポンスの基本検証を行う関数
/// 
/// この関数は予測APIからのレスポンスに含まれるデータの基本的な検証を行います：
/// - forecast_valuesが空でないこと
/// - forecast_timestampが空でないこと
/// - 両者の長さが一致していること
/// 
/// # Arguments
/// * `forecast_values` - 予測値の配列
/// * `forecast_timestamp` - 予測タイムスタンプの配列
/// 
/// # Returns
/// `Result<(), PredictionError>` - 検証が成功した場合はOk(())、失敗した場合はエラー
pub fn validate_prediction_response(
    forecast_values: &[f64],
    forecast_timestamp: &[DateTime<Utc>],
) -> Result<(), PredictionError> {
    if forecast_values.is_empty() {
        return Err(PredictionError::InvalidData(
            "Forecast values are empty".to_string(),
        ));
    }

    if forecast_timestamp.is_empty() {
        return Err(PredictionError::InvalidData(
            "Forecast timestamps are empty".to_string(),
        ));
    }

    if forecast_values.len() != forecast_timestamp.len() {
        return Err(PredictionError::InvalidData(format!(
            "Forecast values length ({}) does not match timestamps length ({})",
            forecast_values.len(),
            forecast_timestamp.len()
        )));
    }

    Ok(())
}

/// 予測データをValueAtTime形式に変換し、必要に応じてレベル調整を行う関数
/// 
/// この関数は予測APIから返されたデータを処理して、以下の処理を行います：
/// - 予測値とタイムスタンプをValueAtTime形式に変換
/// - テストデータの最後の値に基づいてレベル調整（オフセット適用）
/// - 予測データの形状を保持しつつ、実際のデータとの連続性を確保
/// 
/// # Arguments
/// * `forecast_values` - 予測値の配列
/// * `forecast_timestamp` - 予測タイムスタンプの配列  
/// * `test_data` - テストデータ（レベル調整の基準値として使用）
/// 
/// # Returns
/// `Result<Vec<ValueAtTime>, PredictionError>` - 変換・調整済みの予測データまたはエラー
pub fn transform_forecast_data(
    forecast_values: &[f64],
    forecast_timestamp: &[DateTime<Utc>],
    test_data: &[ValueAtTime],
) -> Result<Vec<ValueAtTime>, PredictionError> {
    // 前提条件のチェック
    if forecast_values.is_empty() {
        return Err(PredictionError::InvalidData(
            "予測値が空です".to_string(),
        ));
    }

    if forecast_timestamp.is_empty() {
        return Err(PredictionError::InvalidData(
            "予測タイムスタンプが空です".to_string(),
        ));
    }

    if forecast_values.len() != forecast_timestamp.len() {
        return Err(PredictionError::InvalidData(format!(
            "予測値の数({})と予測タイムスタンプの数({})が一致しません",
            forecast_values.len(),
            forecast_timestamp.len()
        )));
    }

    if test_data.is_empty() {
        return Err(PredictionError::InvalidData(
            "テストデータが空です".to_string(),
        ));
    }

    let mut forecast_points: Vec<ValueAtTime> = Vec::new();
    
    // 予測データを変換
    for (i, timestamp) in forecast_timestamp.iter().enumerate() {
        forecast_points.push(ValueAtTime {
            time: timestamp.naive_utc(),
            value: forecast_values[i],
        });
    }

    Ok(forecast_points)
}

/// チャートSVGを生成する関数
/// 
/// この関数は実際のデータと予測データを使ってチャートを生成します：
/// - 実際のデータと予測データを異なる色で表示
/// - 時間軸の重なりをデバッグ出力で確認
/// - カスタマイズ可能なチャートオプション
/// 
/// # Arguments
/// * `actual_data` - 実際のデータ（正規化済み）
/// * `forecast_data` - 予測データ（変換済み）
/// 
/// # Returns
/// `Result<String, PredictionError>` - 生成されたSVGチャートまたはエラー
pub fn generate_prediction_chart_svg(
    actual_data: &[ValueAtTime],
    forecast_data: &[ValueAtTime],
) -> Result<String, PredictionError> {
    let config = get_config();
    
    // デバッグ出力：時間軸の重なりをチェック
    log::debug!("=== チャートデータの時間軸分析 ===");
    
    if let (Some(actual_first), Some(actual_last)) = (actual_data.first(), actual_data.last()) {
        log::debug!("実際データ時間範囲: {} ～ {}", actual_first.time, actual_last.time);
        log::debug!("実際データ点数: {}", actual_data.len());
    }
    
    if let (Some(forecast_first), Some(forecast_last)) = (forecast_data.first(), forecast_data.last()) {
        log::debug!("予測データ時間範囲: {} ～ {}", forecast_first.time, forecast_last.time);
        log::debug!("予測データ点数: {}", forecast_data.len());
        
        // 重複チェック：実際データの最後と予測データの最初が重なっているか
        if let Some(actual_last) = actual_data.last() {
            if actual_last.time == forecast_first.time {
                log::debug!("✅ 時間軸の接続OK: {} で接続", actual_last.time);
            } else {
                log::debug!("⚠️ 時間軸のギャップ: 実際データ終了 {} vs 予測データ開始 {}", 
                        actual_last.time, forecast_first.time);
            }
        }
    }
    
    let chart_svg = plot_multi_values_at_time_to_svg_with_options(
        &[
            MultiPlotSeries {
                name: "実際の価格".to_string(),
                values: actual_data.to_vec(),
                color: BLUE,
            },
            MultiPlotSeries {
                name: "予測価格".to_string(),
                values: forecast_data.to_vec(),
                color: RED,
            },
        ],
        MultiPlotOptions {
            title: Some("価格予測".to_string()),
            image_size: config.chart_size(),
            x_label: Some("時間".to_string()),
            y_label: Some("価格".to_string()),
        },
    )
    .map_err(|e| PredictionError::ChartGenerationFailed(e.to_string()))?;

    Ok(chart_svg)
}

/// PredictionResultを作成する関数
/// 
/// この関数は処理済みのデータから最終的なPredictionResultを構築します：
/// - 予測価格の計算（最後の予測値）
/// - 精度の計算（MAPEから導出）
/// - 各種メトリクスの設定
/// 
/// # Arguments
/// * `forecast_data` - 変換済みの予測データ
/// * `chart_svg` - 生成済みのチャートSVG
/// * `metrics` - 計算済みの評価メトリクス
/// 
/// # Returns
/// `PredictionResult` - 完成した予測結果
pub fn create_prediction_result(
    forecast_data: Vec<ValueAtTime>,
    chart_svg: String,
    metrics: HashMap<String, f64>,
) -> PredictionResult {
    // 予測価格（最後の予測値）
    let predicted_price = forecast_data.last().map(|p| p.value).unwrap_or(0.0);

    PredictionResult {
        predicted_price,
        accuracy: 100.0 - metrics.get("MAPE").unwrap_or(&0.0), // MAPEから精度を計算
        chart_svg: Some(chart_svg),
        metrics,
        forecast_data,
    }
}

/// 予測精度の評価指標を計算する関数
pub fn calculate_metrics(actual: &[f64], predicted: &[f64]) -> HashMap<String, f64> {
    let n = actual.len().min(predicted.len());
    if n == 0 {
        return HashMap::new();
    }

    // 二乗誤差和
    let mut squared_errors_sum = 0.0;
    // 絶対誤差和
    let mut absolute_errors_sum = 0.0;
    // 絶対パーセント誤差和
    let mut absolute_percent_errors_sum = 0.0;

    for i in 0..n {
        let error = actual[i] - predicted[i];
        squared_errors_sum += error * error;
        absolute_errors_sum += error.abs();

        // 分母がゼロに近い場合はパーセント誤差を計算しない
        if actual[i].abs() > 1e-10 {
            absolute_percent_errors_sum += (error.abs() / actual[i].abs()) * 100.0;
        }
    }

    let mut metrics = HashMap::new();
    metrics.insert("RMSE".to_string(), (squared_errors_sum / n as f64).sqrt());
    metrics.insert("MAE".to_string(), absolute_errors_sum / n as f64);
    metrics.insert("MAPE".to_string(), absolute_percent_errors_sum / n as f64);

    metrics
}

/// データの基本検証と正規化処理を行う関数
/// 
/// この関数は以下の処理を行います：
/// - データの基本検証（空データ、最小サイズチェック）
/// - 数値の妥当性検証（有限値、正の値）
/// - 時系列データの順序チェック
/// - データ正規化処理（設定に応じて）
/// 
/// # Arguments
/// * `values_data` - 検証・正規化する時系列データ
/// 
/// # Returns
/// `Result<Vec<ValueAtTime>, PredictionError>` - 検証・正規化されたデータまたはエラー
pub fn validate_and_normalize_data(
    values_data: &[ValueAtTime],
) -> Result<Vec<ValueAtTime>, PredictionError> {
    // データの基本検証
    if values_data.is_empty() {
        return Err(PredictionError::DataNotFound);
    }

    if values_data.len() < 4 {
        return Err(PredictionError::InsufficientData);
    }

    // 数値の妥当性検証と時系列チェック
    let mut previous_time: Option<chrono::NaiveDateTime> = None;
    for (i, point) in values_data.iter().enumerate() {
        // 数値検証
        if !point.value.is_finite() {
            return Err(PredictionError::InvalidData(format!(
                "Invalid value at index {}: {} (not finite)", i, point.value
            )));
        }
        if point.value <= 0.0 {
            return Err(PredictionError::InvalidData(format!(
                "Invalid value at index {}: {} (not positive)", i, point.value
            )));
        }
        
        // 時系列の順序チェック
        if let Some(prev_time) = previous_time {
            if point.time <= prev_time {
                return Err(PredictionError::InvalidData(format!(
                    "Time series order error at index {}: {} <= {} (not strictly increasing)", 
                    i, point.time, prev_time
                )));
            }
        }
        
        // 重複時刻チェック（既に前のチェックで検出されるが明示的に）
        if let Some(prev_time) = previous_time {
            if point.time == prev_time {
                return Err(PredictionError::InvalidData(format!(
                    "Duplicate timestamp at index {}: {}", i, point.time
                )));
            }
        }
        
        previous_time = Some(point.time);
    }

    // データ正規化処理を追加
    let config = get_config();
    let normalized_data = if config.enable_normalization {
        let normalizer = DataNormalizer::new(
            config.normalization_window,
            config.outlier_threshold,
            config.max_change_ratio,
        );
        
        let normalized_values = normalizer.normalize_data(values_data)
            .map_err(|e| PredictionError::InvalidData(format!("データ正規化エラー: {}", e)))?;
        
        // 品質指標を出力（デバッグ用）
        let quality_metrics = normalizer.calculate_data_quality_metrics(values_data, &normalized_values);
        log::info!("データ正規化完了:");
        quality_metrics.print_summary();
        
        normalized_values
    } else {
        values_data.to_vec()
    };
    
    Ok(normalized_data)
}

/// データを90:10に分割する関数（90%を学習、10%をテスト）
/// 
/// # Arguments
/// * `normalized_data` - 検証・正規化済みの時系列データ
/// 
/// # Returns
/// `Result<(&[ValueAtTime], &[ValueAtTime]), PredictionError>` - (学習データ, テストデータ)のタプルまたはエラー
pub fn split_data_for_prediction(
    normalized_data: &[ValueAtTime],
) -> Result<(&[ValueAtTime], &[ValueAtTime]), PredictionError> {
    // データを90:10に分割（90%を学習、10%をテスト）
    let split_point = (normalized_data.len() as f64 * 0.9) as usize;
    if split_point < 2 || (normalized_data.len() - split_point) < 1 {
        return Err(PredictionError::InsufficientData);
    }

    let training_data = &normalized_data[..split_point];
    let test_data = &normalized_data[split_point..];

    if training_data.is_empty() || test_data.is_empty() {
        return Err(PredictionError::InsufficientData);
    }

    Ok((training_data, test_data))
}

/// ZeroShotPredictionRequestを作成する関数
/// 
/// この関数は分割済みのデータからZeroShotPredictionRequestを作成します。
/// 
/// # Arguments
/// * `training_data` - 学習用データ（既に分割済み）
/// * `test_data` - テスト用データ（既に分割済み）
/// * `model_name` - 使用するモデル名
/// 
/// # Returns
/// `Result<ZeroShotPredictionRequest, PredictionError>` - 作成されたリクエストまたはエラー
pub fn create_prediction_request(
    training_data: &[ValueAtTime],
    test_data: &[ValueAtTime],
    model_name: String,
) -> Result<ZeroShotPredictionRequest, PredictionError> {

    // 予測用のタイムスタンプと値を抽出
    let timestamps: Vec<DateTime<Utc>> = training_data
        .iter()
        .map(|v| DateTime::<Utc>::from_naive_utc_and_offset(v.time, Utc))
        .collect();
    let values: Vec<_> = training_data.iter().map(|v| v.value).collect();

    // 予測対象の終了日時（テストデータの最後まで）
    let forecast_until = match test_data.last() {
        Some(last_point) => DateTime::<Utc>::from_naive_utc_and_offset(last_point.time, Utc),
        None => return Err(PredictionError::InsufficientData),
    };

    // ZeroShotPredictionRequestを作成
    let config = get_config();
    let prediction_request = if config.omit_model_name {
        // モデル名を省略（サーバーのデフォルトモデルを使用）
        log::debug!("モデル名を省略してリクエストを送信（サーバーデフォルトを使用）");
        ZeroShotPredictionRequest::new(timestamps, values, forecast_until)
    } else {
        // モデル名を明示的に指定
        log::debug!("モデル名を指定してリクエストを送信: {}", model_name);
        ZeroShotPredictionRequest::new(timestamps, values, forecast_until)
            .with_model_name(model_name)
    };
    
    Ok(prediction_request)
}

#[cfg(test)]
mod tests;
