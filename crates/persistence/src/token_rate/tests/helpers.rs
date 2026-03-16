use super::*;

/// テスト用ヘルパー: BigDecimal からデフォルト decimals (24) の ExchangeRate を作成
pub fn make_rate(value: i64) -> ExchangeRate {
    ExchangeRate::from_raw_rate(BigDecimal::from(value), 24)
}

/// テスト用ヘルパー: 文字列からデフォルト decimals (24) の ExchangeRate を作成
pub fn make_rate_str(value: &str) -> ExchangeRate {
    ExchangeRate::from_raw_rate(BigDecimal::from_str(value).unwrap(), 24)
}

/// テスト用ヘルパー: TokenRate を簡潔に作成
pub fn make_token_rate(
    base: TokenOutAccount,
    quote: TokenInAccount,
    rate: i64,
    timestamp: NaiveDateTime,
) -> TokenRate {
    TokenRate {
        base,
        quote,
        exchange_rate: make_rate(rate),
        timestamp,
        rate_calc_near: 10,
        swap_path: None,
    }
}

/// テスト用ヘルパー: TokenRate を文字列レートで作成
pub fn make_token_rate_str(
    base: TokenOutAccount,
    quote: TokenInAccount,
    rate: &str,
    timestamp: NaiveDateTime,
) -> TokenRate {
    TokenRate {
        base,
        quote,
        exchange_rate: make_rate_str(rate),
        timestamp,
        rate_calc_near: 10,
        swap_path: None,
    }
}

/// テスト用ヘルパー: BigDecimal の近似比較（マルチホップ補正で無限小数が発生するため）
pub fn assert_rate_approx_eq(actual: &BigDecimal, expected: &BigDecimal, message: &str) {
    let diff = (actual - expected).abs();
    let tolerance = BigDecimal::from_str("0.0000000001").unwrap();
    assert!(
        diff < tolerance,
        "{}: expected ~{}, got {} (diff={})",
        message,
        expected,
        actual,
        diff,
    );
}

// TokenRateインスタンス比較用マクロ
macro_rules! assert_token_rate_eq {
    ($left:expr, $right:expr, $message:expr) => {{
        const PRECISION: u16 = 3; // ミリ秒精度

        // 各フィールドを個別に比較
        assert_eq!(
            $left.base, $right.base,
            "{} - ベーストークンが一致しません",
            $message
        );
        assert_eq!(
            $left.quote, $right.quote,
            "{} - クォートトークンが一致しません",
            $message
        );
        assert_eq!(
            $left.exchange_rate.raw_rate(),
            $right.exchange_rate.raw_rate(),
            "{} - レートが一致しません",
            $message
        );

        // タイムスタンプだけ精度調整して比較
        let left_ts = $left.timestamp.trunc_subsecs(PRECISION);
        let right_ts = $right.timestamp.trunc_subsecs(PRECISION);
        assert_eq!(
            left_ts, right_ts,
            "{} - タイムスタンプが一致しません ({}ミリ秒精度) - 元: {} vs {}",
            $message, PRECISION, $left.timestamp, $right.timestamp
        );
    }};
}

// テーブルからすべてのレコードを削除する補助関数
pub async fn clean_table() -> Result<()> {
    let conn = connection_pool::get().await?;
    conn.interact(|conn| diesel::delete(token_rates::table).execute(conn))
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    // トランザクションがDBに反映されるのを少し待つ
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    Ok(())
}
