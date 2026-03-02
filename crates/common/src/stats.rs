use crate::types::{TokenAccount, TokenPrice};
use chrono::{Duration, NaiveDateTime};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DescribesRequest {
    pub quote_token: String,
    pub base_token: String,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub period: Duration,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetValuesRequest {
    pub quote_token: TokenAccount,
    pub base_token: TokenAccount,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GetValuesResponse {
    pub values: Vec<ValueAtTime>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ValueAtTime {
    pub value: TokenPrice,
    pub time: NaiveDateTime,
}

#[cfg(test)]
mod tests {
    use super::*;
    use bigdecimal::BigDecimal;
    use num_traits::ToPrimitive;
    use std::str::FromStr;

    #[test]
    fn test_value_at_time_serialization_compatibility() {
        // 旧形式（BigDecimal）の JSON 文字列
        let old_json = r#"{"value":"123.456789","time":"2024-01-15T10:30:00"}"#;

        // 新形式（TokenPrice）でデシリアライズ
        let value_at_time: ValueAtTime = serde_json::from_str(old_json).unwrap();

        // 値が正しいことを確認
        assert_eq!(
            value_at_time.value.as_bigdecimal().to_f64().unwrap(),
            123.456789
        );

        // 再シリアライズして同じ形式になることを確認
        let new_json = serde_json::to_string(&value_at_time).unwrap();
        assert_eq!(old_json, new_json);
    }

    #[test]
    fn test_value_at_time_roundtrip() {
        let original = ValueAtTime {
            value: TokenPrice::from_near_per_token(BigDecimal::from_str("999.123").unwrap()),
            time: NaiveDateTime::parse_from_str("2024-06-01 12:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: ValueAtTime = serde_json::from_str(&json).unwrap();

        assert_eq!(original, deserialized);
    }
}
