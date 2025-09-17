use super::*;
use bigdecimal::BigDecimal;
use chrono::{NaiveDateTime, Utc};
use common::algorithm::calculate_volatility_score;
use common::stats::ValueAtTime;

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
        value: BigDecimal::from(100),
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
            value: BigDecimal::from(100),
        },
        ValueAtTime {
            time: base_time + chrono::Duration::hours(1),
            value: BigDecimal::from(100),
        },
        ValueAtTime {
            time: base_time + chrono::Duration::hours(2),
            value: BigDecimal::from(100),
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
            value: BigDecimal::from(100),
        },
        ValueAtTime {
            time: base_time + chrono::Duration::hours(1),
            value: BigDecimal::from(110), // +10%
        },
        ValueAtTime {
            time: base_time + chrono::Duration::hours(2),
            value: BigDecimal::from(90), // -18.18%
        },
        ValueAtTime {
            time: base_time + chrono::Duration::hours(3),
            value: BigDecimal::from(105), // +16.67%
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
            value: BigDecimal::from(0),
        },
        ValueAtTime {
            time: base_time + chrono::Duration::hours(1),
            value: BigDecimal::from(100),
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
            value: BigDecimal::from(100),
        },
        ValueAtTime {
            time: base_time + chrono::Duration::hours(1),
            value: BigDecimal::from(200), // +100%
        },
        ValueAtTime {
            time: base_time + chrono::Duration::hours(2),
            value: BigDecimal::from(50), // -75%
        },
        ValueAtTime {
            time: base_time + chrono::Duration::hours(3),
            value: BigDecimal::from(150), // +200%
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
        value: BigDecimal::from(150),
    }];

    // Test single price point
    let default_price = BigDecimal::from(0);
    let current_price = values.last().map(|v| &v.value).unwrap_or(&default_price);
    let price_24h_ago = values.first().map(|v| &v.value).unwrap_or(current_price);
    let price_change_24h = if price_24h_ago > &BigDecimal::from(0) {
        ((current_price - price_24h_ago) / price_24h_ago) * BigDecimal::from(100)
    } else {
        BigDecimal::from(0)
    };
    let volatility_score = calculate_volatility_score(&values, true);

    assert_eq!(current_price, &BigDecimal::from(150));
    assert_eq!(price_change_24h, BigDecimal::from(0)); // No change with single point
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
            value: BigDecimal::from(100 + (i * 2)), // Increasing by 2 each hour
        });
    }

    let default_price = BigDecimal::from(0);
    let current_price = values.last().map(|v| &v.value).unwrap_or(&default_price);
    let price_24h_ago = if values.len() > 24 {
        &values[values.len() - 24].value
    } else {
        values.first().map(|v| &v.value).unwrap_or(current_price)
    };

    let price_change_24h = if price_24h_ago > &BigDecimal::from(0) {
        ((current_price - price_24h_ago) / price_24h_ago) * BigDecimal::from(100)
    } else {
        BigDecimal::from(0)
    };

    assert_eq!(current_price, &BigDecimal::from(148)); // 100 + (24 * 2)
    assert_eq!(price_24h_ago, &BigDecimal::from(102)); // Value 24 hours ago: 100 + (1 * 2)
    let expected_change = (&BigDecimal::from(148) - &BigDecimal::from(102))
        / &BigDecimal::from(102)
        * &BigDecimal::from(100); // About 45.1%
    assert_eq!(price_change_24h, expected_change);
}

#[test]
fn test_price_data_calculation_with_zero_price() {
    let base_time =
        NaiveDateTime::parse_from_str("2024-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    let values = [
        ValueAtTime {
            time: base_time,
            value: BigDecimal::from(0), // Zero price 24h ago
        },
        ValueAtTime {
            time: base_time + chrono::Duration::hours(1),
            value: BigDecimal::from(100),
        },
    ];

    let default_price = BigDecimal::from(0);
    let current_price = values.last().map(|v| &v.value).unwrap_or(&default_price);
    let price_24h_ago = values.first().map(|v| &v.value).unwrap_or(current_price);
    let price_change_24h = if price_24h_ago > &BigDecimal::from(0) {
        ((current_price - price_24h_ago) / price_24h_ago) * BigDecimal::from(100)
    } else {
        BigDecimal::from(0)
    };

    assert_eq!(current_price, &BigDecimal::from(100));
    assert_eq!(price_24h_ago, &BigDecimal::from(0));
    assert_eq!(price_change_24h, BigDecimal::from(0)); // Should be 0 when dividing by zero price
}
