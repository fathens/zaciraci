#[cfg(test)]
mod tests;

use crate::Result;
use crate::logging::*;
use crate::trade::stats::Point;
use anyhow::anyhow;
use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use linfa::prelude::*;
use linfa_linear::LinearRegression;
use ndarray::{Array1, Array2};
#[cfg(test)]
use num_traits::ToPrimitive;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TimeSeriesPredictionError {
    #[error("insufficient data points for prediction: need at least {required} but got {actual}")]
    InsufficientData { required: usize, actual: usize },

    #[error("conversion error: {0}")]
    ConversionError(String),

    #[error("prediction error: {0}")]
    PredictionError(String),

    #[error("invalid time range: {0}")]
    InvalidTimeRange(String),
}

/// 時系列データのポイントをf64に変換する
#[cfg(test)]
fn convert_to_f64(decimal: &BigDecimal) -> Result<f64> {
    decimal.to_f64().ok_or_else(|| {
        anyhow!(TimeSeriesPredictionError::ConversionError(format!(
            "Failed to convert BigDecimal to f64: {}",
            decimal
        )))
    })
}

/// f64から時系列データのポイントに変換する
fn convert_from_f64(value: f64) -> Result<BigDecimal> {
    BigDecimal::try_from(value).map_err(|e| {
        anyhow!(TimeSeriesPredictionError::ConversionError(format!(
            "Failed to convert f64 to BigDecimal: {}: {}",
            value, e
        )))
    })
}

/// ラグ特徴量を生成する
/// `lag_count`: 何期前までのデータを特徴量として使用するか
fn create_lag_features(points: &[Point], lag_count: usize) -> Result<(Vec<Vec<f64>>, Vec<f64>)> {
    let log = DEFAULT.new(o!(
        "function" => "arima::create_lag_features",
        "points_count" => points.len(),
        "lag_count" => lag_count,
    ));

    if points.len() <= lag_count {
        return Err(anyhow!(TimeSeriesPredictionError::InsufficientData {
            required: lag_count + 1,
            actual: points.len(),
        }));
    }

    info!(log, "Creating lag features");

    // 時間順にソート（古い順）
    let mut sorted_points = points.to_vec();
    sorted_points.sort_by_key(|p| p.timestamp);

    // rateをf64に変換
    let rates: Result<Vec<f64>> = sorted_points
        .iter()
        .map(|p| Ok(p.price.to_f64().as_f64()))
        .collect();

    let rates = rates?;

    // 特徴量（X）とターゲット（y）の準備
    let mut features = Vec::new();
    let mut targets = Vec::new();

    // lag_count+1以降のデータポイントについて処理
    for i in lag_count..rates.len() {
        // 過去lag_count分のデータを特徴量として使用
        let mut feature = Vec::with_capacity(lag_count);
        for j in 0..lag_count {
            feature.push(rates[i - lag_count + j]);
        }
        features.push(feature);
        targets.push(rates[i]);
    }

    info!(log, "Created lag features";
        "features_count" => features.len(),
        "target_count" => targets.len()
    );

    Ok((features, targets))
}

/// Vecをndarray形式に変換する
fn vec_to_ndarray(
    features: Vec<Vec<f64>>,
    targets: Vec<f64>,
) -> Result<(Array2<f64>, Array1<f64>)> {
    let n_samples = features.len();
    if n_samples == 0 {
        return Err(anyhow!(TimeSeriesPredictionError::InsufficientData {
            required: 1,
            actual: 0,
        }));
    }

    let n_features = features[0].len();

    // 特徴量行列X
    let mut x_data = Vec::with_capacity(n_samples * n_features);
    for feature in &features {
        x_data.extend_from_slice(feature);
    }
    let x = Array2::from_shape_vec((n_samples, n_features), x_data).map_err(|e| {
        anyhow!(TimeSeriesPredictionError::ConversionError(format!(
            "Failed to convert features to ndarray: {}",
            e
        )))
    })?;

    // ターゲットベクトルy
    let y = Array1::from_vec(targets);

    Ok((x, y))
}

/// 線形回帰モデルを訓練する
fn train_linear_model(
    features: Vec<Vec<f64>>,
    targets: Vec<f64>,
) -> Result<linfa_linear::FittedLinearRegression<f64>> {
    let log = DEFAULT.new(o!("function" => "arima::train_linear_model"));
    info!(log, "Training linear regression model");

    let (x, y) = vec_to_ndarray(features, targets)?;

    // データセットの作成
    let dataset = Dataset::new(x, y);

    // モデルの訓練
    let model = LinearRegression::default().fit(&dataset).map_err(|e| {
        anyhow!(TimeSeriesPredictionError::PredictionError(format!(
            "Failed to train linear model: {}",
            e
        )))
    })?;

    info!(log, "Model training completed");

    Ok(model)
}

/// 将来の特徴量を生成する
fn generate_future_features(
    points: &[Point],
    lag_count: usize,
    steps_ahead: usize,
) -> Result<Vec<f64>> {
    let log = DEFAULT.new(o!(
        "function" => "arima::generate_future_features",
        "lag_count" => lag_count,
        "steps_ahead" => steps_ahead,
    ));

    if points.len() < lag_count {
        return Err(anyhow!(TimeSeriesPredictionError::InsufficientData {
            required: lag_count,
            actual: points.len(),
        }));
    }

    // 時間順にソート（古い順）
    let mut sorted_points = points.to_vec();
    sorted_points.sort_by_key(|p| p.timestamp);

    // 最新のlag_count個のデータポイントを取得
    let latest_points = &sorted_points[sorted_points.len() - lag_count..];

    // priceをf64に変換
    let rates: Result<Vec<f64>> = latest_points
        .iter()
        .map(|p| Ok(p.price.to_f64().as_f64()))
        .collect();

    let features = rates?;

    info!(log, "Generated future features"; "features" => ?features);

    Ok(features)
}

/// 将来の時点における予測値を計算する
pub fn predict_future_rate(points: &[Point], target_time: NaiveDateTime) -> Result<BigDecimal> {
    let log = DEFAULT.new(o!(
        "function" => "arima::predict_future_rate",
        "points_count" => points.len(),
        "target_time" => format!("{:?}", target_time),
    ));

    info!(log, "Starting prediction calculation");

    // 必要なラグ数の設定（ハイパーパラメータ）
    const LAG_COUNT: usize = 5;

    // 最小必要データ数のチェック
    if points.len() < LAG_COUNT + 1 {
        return Err(anyhow!(TimeSeriesPredictionError::InsufficientData {
            required: LAG_COUNT + 1,
            actual: points.len(),
        }));
    }

    // 時間順にポイントをソート
    let mut sorted_points = points.to_vec();
    sorted_points.sort_by_key(|p| p.timestamp);

    // 最新のタイムスタンプを取得
    let latest_time = sorted_points.last().unwrap().timestamp;

    // 予測すべき時間ステップ数を計算
    if target_time <= latest_time {
        return Err(anyhow!(TimeSeriesPredictionError::InvalidTimeRange(
            format!(
                "Target time must be in the future: latest={:?}, target={:?}",
                latest_time, target_time
            )
        )));
    }

    // おおよその時間間隔を計算（最後の数ポイントの平均間隔）
    let time_intervals: Vec<i64> = sorted_points
        .iter()
        .skip(1)
        .zip(sorted_points.iter())
        .map(|(curr, prev)| (curr.timestamp - prev.timestamp).num_seconds())
        .collect();

    let avg_interval = if time_intervals.is_empty() {
        // デフォルト値（1時間）
        3600
    } else {
        time_intervals.iter().sum::<i64>() / time_intervals.len() as i64
    };

    let seconds_to_predict = (target_time - latest_time).num_seconds();
    let steps_ahead = (seconds_to_predict as f64 / avg_interval as f64).ceil() as usize;

    info!(log, "Prediction parameters";
        "avg_interval_seconds" => avg_interval,
        "steps_ahead" => steps_ahead
    );

    // 訓練データの準備
    let (features, targets) = create_lag_features(&sorted_points, LAG_COUNT)?;

    // モデルの訓練
    let model = train_linear_model(features, targets)?;

    // 予測の実行
    let mut current_features = generate_future_features(&sorted_points, LAG_COUNT, steps_ahead)?;
    let mut predicted_value = 0.0;

    // 段階的に予測を実行
    for i in 0..steps_ahead {
        // 現在の特徴量からの予測
        let x_pred =
            Array2::from_shape_vec((1, LAG_COUNT), current_features.clone()).map_err(|e| {
                anyhow!(TimeSeriesPredictionError::ConversionError(format!(
                    "Failed to convert prediction features to ndarray: {}",
                    e
                )))
            })?;

        let dataset_pred = Dataset::from(x_pred);
        let predictions = model.predict(dataset_pred);

        // DatasetBaseからtargetsを取得し、予測値を抽出
        let prediction_values = predictions.targets().to_owned().into_raw_vec();
        predicted_value = if prediction_values.is_empty() {
            // 予測が失敗した場合は最後の値を使用
            current_features[current_features.len() - 1]
        } else if prediction_values.len() == 1 {
            // 単一の予測値の場合
            prediction_values[0]
        } else {
            // 複数の予測値がある場合は加重平均を使用（新しい値により重みを付ける）
            let mut weighted_sum = 0.0;
            let mut weight_sum = 0.0;
            for (idx, &value) in prediction_values.iter().enumerate() {
                let weight = (idx + 1) as f64; // 後の値により高い重みを付ける
                weighted_sum += value * weight;
                weight_sum += weight;
            }
            if weight_sum > 0.0 {
                weighted_sum / weight_sum
            } else {
                prediction_values[0]
            }
        };

        // 次のステップの特徴量を更新
        if i < steps_ahead - 1 {
            current_features.remove(0);
            current_features.push(predicted_value);
        }

        info!(log, "Step-ahead prediction";
            "step" => i + 1,
            "predicted_value" => predicted_value
        );
    }

    // 予測値をBigDecimalに変換
    let result = convert_from_f64(predicted_value)?;

    info!(log, "Prediction completed"; "result" => %result);

    Ok(result)
}
