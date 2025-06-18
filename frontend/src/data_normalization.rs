use std::collections::VecDeque;
use zaciraci_common::stats::ValueAtTime;

/// データ正規化とスムージング処理を行うモジュール
pub struct DataNormalizer {
    /// 移動平均のウィンドウサイズ
    pub moving_average_window: usize,
    /// 異常値検出のためのZ-scoreの閾値
    pub outlier_threshold: f64,
    /// 最大許容変化率（前の値からの変化の割合）
    pub max_change_ratio: f64,
}

impl Default for DataNormalizer {
    fn default() -> Self {
        Self {
            moving_average_window: 5,
            outlier_threshold: 2.5,
            max_change_ratio: 0.5, // 50%までの変化を許容
        }
    }
}

impl DataNormalizer {
    pub fn new(window: usize, threshold: f64, max_change: f64) -> Self {
        Self {
            moving_average_window: window,
            outlier_threshold: threshold,
            max_change_ratio: max_change,
        }
    }

    /// データを正規化し、滑らかな値の連続にする
    pub fn normalize_data(&self, data: &[ValueAtTime]) -> Result<Vec<ValueAtTime>, String> {
        if data.is_empty() {
            return Ok(Vec::new());
        }

        if data.len() < 3 {
            return Ok(data.to_vec());
        }

        // ステップ1: 異常値の検出と置換
        let mut cleaned_data = self.remove_outliers(data)?;

        // ステップ2: 急激な変化の平滑化
        cleaned_data = self.smooth_sharp_changes(&cleaned_data)?;

        // ステップ3: 移動平均による平滑化
        cleaned_data = self.apply_moving_average(&cleaned_data)?;

        Ok(cleaned_data)
    }

    /// Z-scoreベースで異常値を検出し、線形補間で置換
    fn remove_outliers(&self, data: &[ValueAtTime]) -> Result<Vec<ValueAtTime>, String> {
        let values: Vec<f64> = data.iter().map(|v| v.value).collect();

        // 平均と標準偏差を計算
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
        let std_dev = variance.sqrt();

        if std_dev == 0.0 {
            return Ok(data.to_vec());
        }

        let mut result = data.to_vec();

        // 異常値を検出し、補間で置換
        for i in 0..result.len() {
            let z_score = (values[i] - mean).abs() / std_dev;

            if z_score > self.outlier_threshold {
                // 線形補間で値を補正
                let interpolated_value = self.interpolate_value(&result, i)?;
                result[i].value = interpolated_value;
            }
        }

        Ok(result)
    }

    /// 急激な変化を検出し、段階的に平滑化
    fn smooth_sharp_changes(&self, data: &[ValueAtTime]) -> Result<Vec<ValueAtTime>, String> {
        if data.len() < 2 {
            return Ok(data.to_vec());
        }

        let mut result = data.to_vec();

        for i in 1..result.len() {
            let prev_value = result[i - 1].value;
            let current_value = result[i].value;

            // 変化率を計算
            let change_ratio = (current_value - prev_value).abs() / prev_value;

            if change_ratio > self.max_change_ratio {
                // 急激な変化を段階的に平滑化
                let smoothed_value = if current_value > prev_value {
                    prev_value * (1.0 + self.max_change_ratio)
                } else {
                    prev_value * (1.0 - self.max_change_ratio)
                };

                result[i].value = smoothed_value;
            }
        }

        Ok(result)
    }

    /// 移動平均による平滑化
    fn apply_moving_average(&self, data: &[ValueAtTime]) -> Result<Vec<ValueAtTime>, String> {
        if data.len() < self.moving_average_window {
            return Ok(data.to_vec());
        }

        let mut result = data.to_vec();
        let mut window: VecDeque<f64> = VecDeque::new();

        // 初期ウィンドウを構築
        for item in data.iter().take(self.moving_average_window.min(data.len())) {
            window.push_back(item.value);
        }

        // 移動平均を適用
        for i in (self.moving_average_window / 2)..(result.len() - self.moving_average_window / 2) {
            let avg = window.iter().sum::<f64>() / window.len() as f64;
            result[i].value = avg;

            // ウィンドウを移動
            if i + self.moving_average_window / 2 + 1 < data.len() {
                window.pop_front();
                window.push_back(data[i + self.moving_average_window / 2 + 1].value);
            }
        }

        Ok(result)
    }

    /// 指定されたインデックスの値を線形補間で計算
    fn interpolate_value(&self, data: &[ValueAtTime], index: usize) -> Result<f64, String> {
        if data.is_empty() {
            return Err("データが空です".to_string());
        }

        if index == 0 {
            // 最初の要素の場合、次の有効な値を使用
            return Ok(data.get(1).map(|v| v.value).unwrap_or(data[0].value));
        }

        if index >= data.len() - 1 {
            // 最後の要素の場合、前の有効な値を使用
            return Ok(data
                .get(data.len() - 2)
                .map(|v| v.value)
                .unwrap_or(data[data.len() - 1].value));
        }

        // 前後の値の平均を取る
        let prev_value = data[index - 1].value;
        let next_value = data[index + 1].value;
        Ok((prev_value + next_value) / 2.0)
    }

    /// データの品質指標を計算
    pub fn calculate_data_quality_metrics(
        &self,
        original: &[ValueAtTime],
        normalized: &[ValueAtTime],
    ) -> DataQualityMetrics {
        let original_variance =
            self.calculate_variance(&original.iter().map(|v| v.value).collect::<Vec<_>>());
        let normalized_variance =
            self.calculate_variance(&normalized.iter().map(|v| v.value).collect::<Vec<_>>());

        let outlier_count = self.count_outliers(original);
        let smoothness_improvement = if original_variance > 0.0 {
            (original_variance - normalized_variance) / original_variance
        } else {
            0.0
        };

        DataQualityMetrics {
            original_variance,
            normalized_variance,
            outlier_count,
            smoothness_improvement,
            data_points: original.len(),
        }
    }

    fn calculate_variance(&self, values: &[f64]) -> f64 {
        if values.is_empty() {
            return 0.0;
        }

        let mean = values.iter().sum::<f64>() / values.len() as f64;
        values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64
    }

    fn count_outliers(&self, data: &[ValueAtTime]) -> usize {
        let values: Vec<f64> = data.iter().map(|v| v.value).collect();
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let variance = self.calculate_variance(&values);
        let std_dev = variance.sqrt();

        if std_dev == 0.0 {
            return 0;
        }

        values
            .iter()
            .filter(|&&v| (v - mean).abs() / std_dev > self.outlier_threshold)
            .count()
    }
}

/// データ品質の指標
#[derive(Debug, Clone)]
pub struct DataQualityMetrics {
    pub original_variance: f64,
    pub normalized_variance: f64,
    pub outlier_count: usize,
    pub smoothness_improvement: f64,
    pub data_points: usize,
}

impl DataQualityMetrics {
    pub fn print_summary(&self) {
        log::debug!("=== データ品質改善サマリー ===");
        log::debug!("データポイント数: {}", self.data_points);
        log::debug!("検出された異常値: {}", self.outlier_count);
        log::debug!("元の分散: {:.6}", self.original_variance);
        log::debug!("正規化後の分散: {:.6}", self.normalized_variance);
        log::debug!(
            "滑らかさの改善: {:.2}%",
            self.smoothness_improvement * 100.0
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;

    fn create_test_data_with_outliers() -> Vec<ValueAtTime> {
        vec![
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-01 00:00:00", "%Y-%m-%d %H:%M:%S")
                    .unwrap(),
                value: 1.0,
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-02 00:00:00", "%Y-%m-%d %H:%M:%S")
                    .unwrap(),
                value: 1.1,
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-03 00:00:00", "%Y-%m-%d %H:%M:%S")
                    .unwrap(),
                value: 5.0, // 異常値
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-04 00:00:00", "%Y-%m-%d %H:%M:%S")
                    .unwrap(),
                value: 1.15,
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-05 00:00:00", "%Y-%m-%d %H:%M:%S")
                    .unwrap(),
                value: 1.2,
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-06 00:00:00", "%Y-%m-%d %H:%M:%S")
                    .unwrap(),
                value: 0.1, // 異常値
            },
            ValueAtTime {
                time: NaiveDateTime::parse_from_str("2025-06-07 00:00:00", "%Y-%m-%d %H:%M:%S")
                    .unwrap(),
                value: 1.25,
            },
        ]
    }

    #[test]
    fn test_data_normalization() {
        // より厳しい閾値を使用してテスト
        let normalizer = DataNormalizer::new(3, 1.5, 0.3);
        let test_data = create_test_data_with_outliers();

        let normalized_data = normalizer.normalize_data(&test_data).unwrap();

        // 正規化後のデータが元のデータと同じ長さであることを確認
        assert_eq!(normalized_data.len(), test_data.len());

        // 品質指標を計算
        let metrics = normalizer.calculate_data_quality_metrics(&test_data, &normalized_data);
        metrics.print_summary();

        // 分散が減少していることを確認（より滑らかになっている）
        assert!(metrics.normalized_variance < metrics.original_variance);

        // 異常値が検出されていることを確認
        assert!(metrics.outlier_count > 0);
    }

    #[test]
    fn test_outlier_detection() {
        // より厳しい閾値を使用してテスト
        let normalizer = DataNormalizer::new(3, 1.5, 0.3);
        let test_data = create_test_data_with_outliers();

        let outlier_count = normalizer.count_outliers(&test_data);

        // より厳しい閾値でテスト
        assert!(outlier_count >= 1);
        log::debug!("検出された異常値数: {}", outlier_count);
    }
}
