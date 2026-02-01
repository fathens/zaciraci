use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::types::TokenPrice;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChronosPredictionResponse {
    /// 予測値（タイムスタンプ → 価格）
    pub forecast: BTreeMap<DateTime<Utc>, BigDecimal>,
    /// 下限信頼区間（10パーセンタイル）
    pub lower_bound: Option<BTreeMap<DateTime<Utc>, BigDecimal>>,
    /// 上限信頼区間（90パーセンタイル）
    pub upper_bound: Option<BTreeMap<DateTime<Utc>, BigDecimal>>,
    /// 使用されたモデル名
    pub model_name: String,
    /// 選択された予測戦略名
    pub strategy_name: String,
    /// 予測処理にかかった時間（秒）
    pub processing_time_secs: f64,
    /// 使用されたモデル数
    pub model_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionPoint {
    pub timestamp: DateTime<Utc>,
    pub value: TokenPrice,
    pub confidence_interval: Option<ConfidenceInterval>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceInterval {
    pub lower: TokenPrice,
    pub upper: TokenPrice,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPredictionResult {
    pub token: String,
    pub prediction_id: String,
    pub predicted_values: Vec<PredictionPoint>,
    pub accuracy_metrics: Option<AccuracyMetrics>,
    pub chart_svg: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccuracyMetrics {
    pub mae: BigDecimal,
    pub rmse: BigDecimal,
    pub mape: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_prediction_point_serialization_compatibility() {
        // 旧形式（BigDecimal）の JSON 文字列
        let old_json = r#"{"timestamp":"2024-01-15T10:30:00Z","value":"123.456789","confidence_interval":null}"#;

        // 新形式（TokenPrice）でデシリアライズ
        let point: PredictionPoint = serde_json::from_str(old_json).unwrap();

        // 値が正しいことを確認
        assert_eq!(point.value.to_f64().as_f64(), 123.456789);

        // 再シリアライズして同じ形式になることを確認
        let new_json = serde_json::to_string(&point).unwrap();
        assert_eq!(old_json, new_json);
    }

    #[test]
    fn test_prediction_point_with_confidence_interval_serialization() {
        // 信頼区間付きの旧形式 JSON
        let old_json = r#"{"timestamp":"2024-01-15T10:30:00Z","value":"100.0","confidence_interval":{"lower":"95.0","upper":"105.0"}}"#;

        // デシリアライズ
        let point: PredictionPoint = serde_json::from_str(old_json).unwrap();

        // 値が正しいことを確認
        assert_eq!(point.value.to_f64().as_f64(), 100.0);
        let ci = point.confidence_interval.as_ref().unwrap();
        assert_eq!(ci.lower.to_f64().as_f64(), 95.0);
        assert_eq!(ci.upper.to_f64().as_f64(), 105.0);

        // 再シリアライズして同じ形式になることを確認
        let new_json = serde_json::to_string(&point).unwrap();
        assert_eq!(old_json, new_json);
    }

    #[test]
    fn test_prediction_point_roundtrip() {
        let original = PredictionPoint {
            timestamp: DateTime::parse_from_rfc3339("2024-06-01T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            value: TokenPrice::from_near_per_token(BigDecimal::from_str("999.123").unwrap()),
            confidence_interval: Some(ConfidenceInterval {
                lower: TokenPrice::from_near_per_token(BigDecimal::from_str("990.0").unwrap()),
                upper: TokenPrice::from_near_per_token(BigDecimal::from_str("1010.0").unwrap()),
            }),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: PredictionPoint = serde_json::from_str(&json).unwrap();

        assert_eq!(original.timestamp, deserialized.timestamp);
        assert_eq!(original.value.to_f64(), deserialized.value.to_f64());
        assert_eq!(
            original
                .confidence_interval
                .as_ref()
                .unwrap()
                .lower
                .to_f64(),
            deserialized
                .confidence_interval
                .as_ref()
                .unwrap()
                .lower
                .to_f64()
        );
    }

    #[test]
    fn test_token_prediction_result_serialization() {
        // TokenPredictionResult の互換性テスト
        let old_json = r#"{"token":"test.near","prediction_id":"pred-123","predicted_values":[{"timestamp":"2024-01-15T10:00:00Z","value":"100.0","confidence_interval":null}],"accuracy_metrics":null,"chart_svg":null}"#;

        let result: TokenPredictionResult = serde_json::from_str(old_json).unwrap();

        assert_eq!(result.token, "test.near");
        assert_eq!(result.predicted_values.len(), 1);
        assert_eq!(result.predicted_values[0].value.to_f64().as_f64(), 100.0);

        // 再シリアライズ
        let new_json = serde_json::to_string(&result).unwrap();
        assert_eq!(old_json, new_json);
    }
}
