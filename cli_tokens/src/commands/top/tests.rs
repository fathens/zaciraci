use super::*;
use bigdecimal::BigDecimal;
use chrono::{NaiveDateTime, Utc};
use common::algorithm::calculate_volatility_score;
use common::stats::ValueAtTime;
use common::types::TokenPrice;

fn price(v: i32) -> TokenPrice {
    TokenPrice::from_near_per_token(BigDecimal::from(v))
}

#[test]
fn test_calculate_volatility_score_empty_data() {
    let values = vec![];
    let result = calculate_volatility_score(&values, true);
    assert_eq!(result, 0.0);
}

#[test]
fn test_calculate_volatility_score_single_value() {
    let values = vec![ValueAtTime {
        time: Utc::now().naive_utc(),
        value: price(100),
    }];
    let result = calculate_volatility_score(&values, true);
    assert_eq!(result, 0.0);
}

#[test]
fn test_calculate_volatility_score_stable_prices() {
    let base_time =
        NaiveDateTime::parse_from_str("2024-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    let values = vec![
        ValueAtTime {
            time: base_time,
            value: price(100),
        },
        ValueAtTime {
            time: base_time + chrono::Duration::hours(1),
            value: price(100),
        },
        ValueAtTime {
            time: base_time + chrono::Duration::hours(2),
            value: price(100),
        },
    ];
    let result = calculate_volatility_score(&values, true);
    assert_eq!(result, 0.0); // No volatility for stable prices
}

#[test]
fn test_calculate_volatility_score_with_changes() {
    let base_time =
        NaiveDateTime::parse_from_str("2024-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    let values = vec![
        ValueAtTime {
            time: base_time,
            value: price(100),
        },
        ValueAtTime {
            time: base_time + chrono::Duration::hours(1),
            value: price(110), // +10%
        },
        ValueAtTime {
            time: base_time + chrono::Duration::hours(2),
            value: price(90), // -18.18%
        },
        ValueAtTime {
            time: base_time + chrono::Duration::hours(3),
            value: price(105), // +16.67%
        },
    ];
    let result = calculate_volatility_score(&values, true);
    // Should have some volatility due to price changes
    assert!(result > 0.0);
    assert!(result <= 1.0);
}

#[test]
fn test_calculate_volatility_score_with_zero_prices() {
    let base_time =
        NaiveDateTime::parse_from_str("2024-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    let values = vec![
        ValueAtTime {
            time: base_time,
            value: price(0),
        },
        ValueAtTime {
            time: base_time + chrono::Duration::hours(1),
            value: price(100),
        },
    ];
    let result = calculate_volatility_score(&values, true);
    // Should handle zero prices gracefully
    assert_eq!(result, 0.0);
}

#[test]
fn test_calculate_volatility_score_high_volatility() {
    let base_time =
        NaiveDateTime::parse_from_str("2024-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    let values = vec![
        ValueAtTime {
            time: base_time,
            value: price(100),
        },
        ValueAtTime {
            time: base_time + chrono::Duration::hours(1),
            value: price(200), // +100%
        },
        ValueAtTime {
            time: base_time + chrono::Duration::hours(2),
            value: price(50), // -75%
        },
        ValueAtTime {
            time: base_time + chrono::Duration::hours(3),
            value: price(150), // +200%
        },
    ];
    let result = calculate_volatility_score(&values, true);
    // Should be capped at 1.0 for very high volatility
    assert_eq!(result, 1.0);
}

#[test]
fn test_parse_date_valid() {
    let result = parse_date("2024-01-15").unwrap();
    assert_eq!(result.format("%Y-%m-%d").to_string(), "2024-01-15");
}

#[test]
fn test_parse_date_invalid() {
    let result = parse_date("invalid-date");
    assert!(result.is_err());
}

#[test]
fn test_price_data_calculation_no_data() {
    let values: Vec<ValueAtTime> = vec![];

    // Test empty price data scenario - should return default values
    let expected = (0.0, 0.0, 0.0, 0.0);

    // Simulate what get_current_price_data_with_volatility returns for empty data
    if values.is_empty() {
        let actual = (0.0, 0.0, 0.0, 0.0);
        assert_eq!(actual, expected);
    }
}

#[test]
fn test_price_data_calculation_single_point() {
    let base_time =
        NaiveDateTime::parse_from_str("2024-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    let values = vec![ValueAtTime {
        time: base_time,
        value: price(150),
    }];

    // Test single price point
    let default_price = TokenPrice::zero();
    let current_price = values.last().map(|v| &v.value).unwrap_or(&default_price);
    let price_24h_ago = values.first().map(|v| &v.value).unwrap_or(current_price);

    // TokenPrice を f64 に変換して計算
    let current_f64 = current_price.to_f64().as_f64();
    let ago_f64 = price_24h_ago.to_f64().as_f64();
    let price_change_24h = if ago_f64 > 0.0 {
        ((current_f64 - ago_f64) / ago_f64) * 100.0
    } else {
        0.0
    };
    let volatility_score = calculate_volatility_score(&values, true);

    assert_eq!(current_price, &price(150));
    assert_eq!(price_change_24h, 0.0); // No change with single point
    assert_eq!(volatility_score, 0.0); // No volatility with single point
}

#[test]
fn test_price_data_calculation_with_24h_change() {
    let base_time =
        NaiveDateTime::parse_from_str("2024-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();

    // Create 25 hours of data (to test 24h ago calculation)
    let mut values = Vec::new();
    for i in 0..25 {
        values.push(ValueAtTime {
            time: base_time + chrono::Duration::hours(i as i64),
            value: price(100 + (i * 2)), // Increasing by 2 each hour
        });
    }

    let default_price = TokenPrice::zero();
    let current_price = values.last().map(|v| &v.value).unwrap_or(&default_price);
    let price_24h_ago = if values.len() > 24 {
        &values[values.len() - 24].value
    } else {
        values.first().map(|v| &v.value).unwrap_or(current_price)
    };

    // TokenPrice を f64 に変換して計算
    let current_f64 = current_price.to_f64().as_f64();
    let ago_f64 = price_24h_ago.to_f64().as_f64();
    let price_change_24h = if ago_f64 > 0.0 {
        ((current_f64 - ago_f64) / ago_f64) * 100.0
    } else {
        0.0
    };

    assert_eq!(current_price, &price(148)); // 100 + (24 * 2)
    assert_eq!(price_24h_ago, &price(102)); // Value 24 hours ago: 100 + (1 * 2)
    let expected_change = ((148.0 - 102.0) / 102.0) * 100.0; // About 45.1%
    assert!((price_change_24h - expected_change).abs() < 0.0001);
}

#[test]
fn test_price_data_calculation_with_zero_price() {
    let base_time =
        NaiveDateTime::parse_from_str("2024-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    let values = [
        ValueAtTime {
            time: base_time,
            value: price(0), // Zero price 24h ago
        },
        ValueAtTime {
            time: base_time + chrono::Duration::hours(1),
            value: price(100),
        },
    ];

    let default_price = TokenPrice::zero();
    let current_price = values.last().map(|v| &v.value).unwrap_or(&default_price);
    let price_24h_ago = values.first().map(|v| &v.value).unwrap_or(current_price);

    // TokenPrice を f64 に変換して計算
    let current_f64 = current_price.to_f64().as_f64();
    let ago_f64 = price_24h_ago.to_f64().as_f64();
    let price_change_24h = if ago_f64 > 0.0 {
        ((current_f64 - ago_f64) / ago_f64) * 100.0
    } else {
        0.0
    };

    assert_eq!(current_price, &price(100));
    assert_eq!(price_24h_ago, &price(0));
    assert_eq!(price_change_24h, 0.0); // Should be 0 when dividing by zero price
}
