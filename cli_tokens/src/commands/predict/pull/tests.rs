use super::*;
use bigdecimal::BigDecimal;
use chrono::Utc;
use common::prediction::{ChronosPredictionResponse, ConfidenceInterval};
use common::types::TokenPrice;
use std::collections::HashMap;

#[test]
fn test_confidence_interval_extraction_with_standard_keys() {
    let mut confidence_intervals = HashMap::new();
    confidence_intervals.insert(
        "lower".to_string(),
        vec![
            BigDecimal::from(95),
            BigDecimal::from(105),
            BigDecimal::from(115),
        ],
    );
    confidence_intervals.insert(
        "upper".to_string(),
        vec![
            BigDecimal::from(105),
            BigDecimal::from(115),
            BigDecimal::from(125),
        ],
    );

    let prediction_result = ChronosPredictionResponse {
        forecast_timestamp: vec![
            Utc::now(),
            Utc::now() + chrono::Duration::hours(1),
            Utc::now() + chrono::Duration::hours(2),
        ],
        forecast_values: vec![
            BigDecimal::from(100),
            BigDecimal::from(110),
            BigDecimal::from(120),
        ],
        model_name: "test_model".to_string(),
        confidence_intervals: Some(confidence_intervals),
        metrics: None,
    };

    // Test the confidence interval extraction logic
    let forecast: Vec<PredictionPoint> = prediction_result
        .forecast_timestamp
        .into_iter()
        .zip(prediction_result.forecast_values)
        .enumerate()
        .map(|(i, (timestamp, value))| {
            let confidence_interval =
                prediction_result
                    .confidence_intervals
                    .as_ref()
                    .and_then(|intervals| {
                        let lower_key = intervals
                            .keys()
                            .find(|k| k.contains("lower") || k.contains("0.025"));
                        let upper_key = intervals
                            .keys()
                            .find(|k| k.contains("upper") || k.contains("0.975"));

                        if let (Some(lower_key), Some(upper_key)) = (lower_key, upper_key) {
                            let lower_values = intervals.get(lower_key)?;
                            let upper_values = intervals.get(upper_key)?;

                            if i < lower_values.len() && i < upper_values.len() {
                                Some(ConfidenceInterval {
                                    lower: TokenPrice::from_near_per_token(lower_values[i].clone()),
                                    upper: TokenPrice::from_near_per_token(upper_values[i].clone()),
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    });

            PredictionPoint {
                timestamp,
                value: TokenPrice::from_near_per_token(value),
                confidence_interval,
            }
        })
        .collect();

    // Verify confidence intervals were extracted correctly
    assert_eq!(forecast.len(), 3);

    assert!(forecast[0].confidence_interval.is_some());
    let ci0 = forecast[0].confidence_interval.as_ref().unwrap();
    assert_eq!(ci0.lower.to_f64(), 95.0);
    assert_eq!(ci0.upper.to_f64(), 105.0);

    assert!(forecast[1].confidence_interval.is_some());
    let ci1 = forecast[1].confidence_interval.as_ref().unwrap();
    assert_eq!(ci1.lower.to_f64(), 105.0);
    assert_eq!(ci1.upper.to_f64(), 115.0);

    assert!(forecast[2].confidence_interval.is_some());
    let ci2 = forecast[2].confidence_interval.as_ref().unwrap();
    assert_eq!(ci2.lower.to_f64(), 115.0);
    assert_eq!(ci2.upper.to_f64(), 125.0);
}

#[test]
fn test_confidence_interval_extraction_with_quantile_keys() {
    let mut confidence_intervals = HashMap::new();
    confidence_intervals.insert(
        "0.025".to_string(),
        vec![
            BigDecimal::from(90),
            BigDecimal::from(100),
            BigDecimal::from(110),
        ],
    );
    confidence_intervals.insert(
        "0.975".to_string(),
        vec![
            BigDecimal::from(110),
            BigDecimal::from(120),
            BigDecimal::from(130),
        ],
    );

    let prediction_result = ChronosPredictionResponse {
        forecast_timestamp: vec![Utc::now(), Utc::now() + chrono::Duration::hours(1)],
        forecast_values: vec![BigDecimal::from(100), BigDecimal::from(110)],
        model_name: "test_model".to_string(),
        confidence_intervals: Some(confidence_intervals),
        metrics: None,
    };

    // Test confidence interval extraction with quantile keys
    let forecast: Vec<PredictionPoint> = prediction_result
        .forecast_timestamp
        .into_iter()
        .zip(prediction_result.forecast_values)
        .enumerate()
        .map(|(i, (timestamp, value))| {
            let confidence_interval =
                prediction_result
                    .confidence_intervals
                    .as_ref()
                    .and_then(|intervals| {
                        let lower_key = intervals
                            .keys()
                            .find(|k| k.contains("lower") || k.contains("0.025"));
                        let upper_key = intervals
                            .keys()
                            .find(|k| k.contains("upper") || k.contains("0.975"));

                        if let (Some(lower_key), Some(upper_key)) = (lower_key, upper_key) {
                            let lower_values = intervals.get(lower_key)?;
                            let upper_values = intervals.get(upper_key)?;

                            if i < lower_values.len() && i < upper_values.len() {
                                Some(ConfidenceInterval {
                                    lower: TokenPrice::from_near_per_token(lower_values[i].clone()),
                                    upper: TokenPrice::from_near_per_token(upper_values[i].clone()),
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    });

            PredictionPoint {
                timestamp,
                value: TokenPrice::from_near_per_token(value),
                confidence_interval,
            }
        })
        .collect();

    // Verify confidence intervals were extracted correctly
    assert_eq!(forecast.len(), 2);

    let ci0 = forecast[0].confidence_interval.as_ref().unwrap();
    assert_eq!(ci0.lower.to_f64(), 90.0);
    assert_eq!(ci0.upper.to_f64(), 110.0);

    let ci1 = forecast[1].confidence_interval.as_ref().unwrap();
    assert_eq!(ci1.lower.to_f64(), 100.0);
    assert_eq!(ci1.upper.to_f64(), 120.0);
}

#[test]
fn test_confidence_interval_extraction_no_intervals() {
    let prediction_result = ChronosPredictionResponse {
        forecast_timestamp: vec![Utc::now()],
        forecast_values: vec![BigDecimal::from(100)],
        model_name: "test_model".to_string(),
        confidence_intervals: None,
        metrics: None,
    };

    // Test without confidence intervals
    let forecast: Vec<PredictionPoint> = prediction_result
        .forecast_timestamp
        .into_iter()
        .zip(prediction_result.forecast_values)
        .enumerate()
        .map(|(i, (timestamp, value))| {
            let confidence_interval =
                prediction_result
                    .confidence_intervals
                    .as_ref()
                    .and_then(|intervals| {
                        let lower_key = intervals
                            .keys()
                            .find(|k| k.contains("lower") || k.contains("0.025"));
                        let upper_key = intervals
                            .keys()
                            .find(|k| k.contains("upper") || k.contains("0.975"));

                        if let (Some(lower_key), Some(upper_key)) = (lower_key, upper_key) {
                            let lower_values = intervals.get(lower_key)?;
                            let upper_values = intervals.get(upper_key)?;

                            if i < lower_values.len() && i < upper_values.len() {
                                Some(ConfidenceInterval {
                                    lower: TokenPrice::from_near_per_token(lower_values[i].clone()),
                                    upper: TokenPrice::from_near_per_token(upper_values[i].clone()),
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    });

            PredictionPoint {
                timestamp,
                value: TokenPrice::from_near_per_token(value),
                confidence_interval,
            }
        })
        .collect();

    // Verify no confidence intervals
    assert_eq!(forecast.len(), 1);
    assert!(forecast[0].confidence_interval.is_none());
}

#[test]
fn test_confidence_interval_extraction_mismatched_lengths() {
    let mut confidence_intervals = HashMap::new();
    confidence_intervals.insert(
        "lower".to_string(),
        vec![BigDecimal::from(95), BigDecimal::from(105)],
    ); // Only 2 values
    confidence_intervals.insert(
        "upper".to_string(),
        vec![
            BigDecimal::from(105),
            BigDecimal::from(115),
            BigDecimal::from(125),
        ],
    ); // 3 values

    let prediction_result = ChronosPredictionResponse {
        forecast_timestamp: vec![
            Utc::now(),
            Utc::now() + chrono::Duration::hours(1),
            Utc::now() + chrono::Duration::hours(2),
        ],
        forecast_values: vec![
            BigDecimal::from(100),
            BigDecimal::from(110),
            BigDecimal::from(120),
        ],
        model_name: "test_model".to_string(),
        confidence_intervals: Some(confidence_intervals),
        metrics: None,
    };

    // Test with mismatched array lengths
    let forecast: Vec<PredictionPoint> = prediction_result
        .forecast_timestamp
        .into_iter()
        .zip(prediction_result.forecast_values)
        .enumerate()
        .map(|(i, (timestamp, value))| {
            let confidence_interval =
                prediction_result
                    .confidence_intervals
                    .as_ref()
                    .and_then(|intervals| {
                        let lower_key = intervals
                            .keys()
                            .find(|k| k.contains("lower") || k.contains("0.025"));
                        let upper_key = intervals
                            .keys()
                            .find(|k| k.contains("upper") || k.contains("0.975"));

                        if let (Some(lower_key), Some(upper_key)) = (lower_key, upper_key) {
                            let lower_values = intervals.get(lower_key)?;
                            let upper_values = intervals.get(upper_key)?;

                            if i < lower_values.len() && i < upper_values.len() {
                                Some(ConfidenceInterval {
                                    lower: TokenPrice::from_near_per_token(lower_values[i].clone()),
                                    upper: TokenPrice::from_near_per_token(upper_values[i].clone()),
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    });

            PredictionPoint {
                timestamp,
                value: TokenPrice::from_near_per_token(value),
                confidence_interval,
            }
        })
        .collect();

    // Verify only first 2 have confidence intervals (due to mismatched lengths)
    assert_eq!(forecast.len(), 3);

    assert!(forecast[0].confidence_interval.is_some());
    assert!(forecast[1].confidence_interval.is_some());
    assert!(forecast[2].confidence_interval.is_none()); // No CI due to length mismatch
}

#[test]
fn test_confidence_interval_scaling() {
    let forecast = vec![PredictionPoint {
        timestamp: Utc::now(),
        value: TokenPrice::from_near_per_token(BigDecimal::from(100)),
        confidence_interval: Some(ConfidenceInterval {
            lower: TokenPrice::from_near_per_token(BigDecimal::from(95)),
            upper: TokenPrice::from_near_per_token(BigDecimal::from(105)),
        }),
    }];

    // Test scaling of confidence intervals
    let scale_factor = BigDecimal::from(2);
    let mut scaled_forecast = forecast;

    for point in &mut scaled_forecast {
        let scaled_value = point.value.clone().into_bigdecimal() * &scale_factor;
        point.value = TokenPrice::from_near_per_token(scaled_value);
        if let Some(ref mut ci) = point.confidence_interval {
            let scaled_lower = ci.lower.clone().into_bigdecimal() * &scale_factor;
            let scaled_upper = ci.upper.clone().into_bigdecimal() * &scale_factor;
            ci.lower = TokenPrice::from_near_per_token(scaled_lower);
            ci.upper = TokenPrice::from_near_per_token(scaled_upper);
        }
    }

    let scaled_point = &scaled_forecast[0];
    assert_eq!(scaled_point.value.to_f64(), 200.0);

    let scaled_ci = scaled_point.confidence_interval.as_ref().unwrap();
    assert_eq!(scaled_ci.lower.to_f64(), 190.0);
    assert_eq!(scaled_ci.upper.to_f64(), 210.0);
}
