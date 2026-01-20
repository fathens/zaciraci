//! 予測値のスケーリング処理
//!
//! Chronos API に送信する値を 0〜1,000,000 の範囲に正規化し、
//! 予測結果を元のスケールに復元する。

use bigdecimal::BigDecimal;
use common::types::TokenPrice;
use serde::{Deserialize, Serialize};

/// スケーリングのターゲット最大値
const SCALE_TARGET: i64 = 1_000_000;

/// スケーリングパラメータ（復元に必要な情報）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScaleParams {
    pub original_min: BigDecimal,
    pub original_max: BigDecimal,
}

/// スケーリング結果
pub struct ScaleResult {
    pub values: Vec<BigDecimal>,
    pub params: ScaleParams,
}

/// 値を 0〜1,000,000 の範囲にスケーリング
///
/// # 引数
/// * `values` - スケーリングする TokenPrice のスライス
///
/// # 戻り値
/// スケーリングされた値と復元用パラメータ
///
/// # パニック
/// `values` が空の場合はパニック
pub fn scale_values(values: &[TokenPrice]) -> ScaleResult {
    assert!(!values.is_empty(), "values must not be empty");

    let min = values.iter().min().expect("values is not empty").clone();
    let max = values.iter().max().expect("values is not empty").clone();

    let min_bd = min.as_bigdecimal().clone();
    let max_bd = max.as_bigdecimal().clone();

    let params = ScaleParams {
        original_min: min_bd.clone(),
        original_max: max_bd.clone(),
    };

    let range = &max_bd - &min_bd;
    let target = BigDecimal::from(SCALE_TARGET);

    let scaled_values = if range == BigDecimal::from(0) {
        // min == max の場合は全て中央値にマップ
        vec![BigDecimal::from(SCALE_TARGET / 2); values.len()]
    } else {
        values
            .iter()
            .map(|v| {
                let v_bd = v.as_bigdecimal().clone();
                let normalized = (v_bd - &min_bd) / &range;
                normalized * &target
            })
            .collect()
    };

    ScaleResult {
        values: scaled_values,
        params,
    }
}

/// スケーリングされた値を元のスケールに復元
///
/// # 引数
/// * `scaled` - スケーリングされた値
/// * `params` - スケーリングパラメータ
///
/// # 戻り値
/// 元のスケールに復元された TokenPrice
pub fn restore_value(scaled: &BigDecimal, params: &ScaleParams) -> TokenPrice {
    let range = &params.original_max - &params.original_min;
    let target = BigDecimal::from(SCALE_TARGET);

    let restored = if range == BigDecimal::from(0) {
        // min == max の場合は元の値を返す
        params.original_min.clone()
    } else {
        let normalized = scaled / &target;
        normalized * &range + &params.original_min
    };

    TokenPrice::from_near_per_token(restored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    /// ヘルパー: BigDecimal から TokenPrice を作成
    fn tp(value: i64) -> TokenPrice {
        TokenPrice::from_near_per_token(BigDecimal::from(value))
    }

    /// ヘルパー: 文字列から TokenPrice を作成
    fn tp_str(s: &str) -> TokenPrice {
        TokenPrice::from_near_per_token(BigDecimal::from_str(s).unwrap())
    }

    #[test]
    fn test_scale_and_restore_roundtrip() {
        let original = vec![
            tp(30_000_000_000_000),
            tp(35_000_000_000_000),
            tp(40_000_000_000_000),
        ];

        let result = scale_values(&original);

        // スケール後の値は 0〜1,000,000 の範囲
        assert_eq!(result.values[0], BigDecimal::from(0));
        assert_eq!(result.values[2], BigDecimal::from(SCALE_TARGET));

        // 中間値の確認
        assert_eq!(result.values[1], BigDecimal::from(500_000));

        // 復元の確認
        for (scaled, orig) in result.values.iter().zip(original.iter()) {
            let restored = restore_value(scaled, &result.params);
            assert_eq!(restored, *orig);
        }
    }

    #[test]
    fn test_scale_small_values() {
        // 割り切れる値を使用（無限小数を避ける）
        let values = vec![tp(0), tp(500), tp(1000)];

        let result = scale_values(&values);

        // 小さい値でも常にスケーリング
        assert_eq!(result.values[0], BigDecimal::from(0));
        assert_eq!(result.values[1], BigDecimal::from(500_000));
        assert_eq!(result.values[2], BigDecimal::from(SCALE_TARGET));

        // 復元の確認
        for (scaled, orig) in result.values.iter().zip(values.iter()) {
            let restored = restore_value(scaled, &result.params);
            assert_eq!(restored, *orig);
        }
    }

    #[test]
    fn test_scale_equal_values() {
        let values = vec![tp(100), tp(100), tp(100)];

        let result = scale_values(&values);

        // 全て同じ値の場合は中央値にマップ
        for v in &result.values {
            assert_eq!(*v, BigDecimal::from(500_000));
        }

        // 復元の確認
        for (scaled, orig) in result.values.iter().zip(values.iter()) {
            let restored = restore_value(scaled, &result.params);
            assert_eq!(restored, *orig);
        }
    }

    #[test]
    fn test_scale_single_value() {
        let values = vec![tp(12345)];

        let result = scale_values(&values);

        // 単一値の場合も中央値にマップ
        assert_eq!(result.values[0], BigDecimal::from(500_000));

        // 復元の確認
        let restored = restore_value(&result.values[0], &result.params);
        assert_eq!(restored, values[0]);
    }

    #[test]
    fn test_scale_with_decimals() {
        let values = vec![tp_str("100.123"), tp_str("200.456"), tp_str("300.789")];

        let result = scale_values(&values);

        // 復元の確認（小数点以下も正確に復元）
        for (scaled, orig) in result.values.iter().zip(values.iter()) {
            let restored = restore_value(scaled, &result.params);
            assert_eq!(restored, *orig);
        }
    }

    #[test]
    #[should_panic(expected = "values must not be empty")]
    fn test_scale_empty_values() {
        let values: Vec<TokenPrice> = vec![];
        scale_values(&values);
    }

    #[test]
    fn test_scale_params_serialization() {
        let params = ScaleParams {
            original_min: BigDecimal::from_str("123.456").unwrap(),
            original_max: BigDecimal::from_str("789.012").unwrap(),
        };

        let json = serde_json::to_string(&params).unwrap();
        let deserialized: ScaleParams = serde_json::from_str(&json).unwrap();

        assert_eq!(params, deserialized);
    }
}
