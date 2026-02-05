use super::*;
use std::str::FromStr;
use zaciraci_common::types::{
    TokenInAccount as CommonTokenInAccount, TokenOutAccount as CommonTokenOutAccount,
};

fn price_from_int(v: i64) -> TokenPrice {
    TokenPrice::from_near_per_token(BigDecimal::from(v))
}

#[test]
fn test_calculate_liquidity_score() {
    use chrono::Utc;
    use zaciraci_common::algorithm::types::{PriceHistory, PricePoint};

    // ケース1: 取引量データなし
    let history_no_volume = PriceHistory {
        token: "test.token".parse::<CommonTokenOutAccount>().unwrap(),
        quote_token: "wrap.near".parse::<CommonTokenInAccount>().unwrap(),
        prices: vec![
            PricePoint {
                timestamp: Utc::now(),
                price: price_from_int(100),
                volume: None,
            },
            PricePoint {
                timestamp: Utc::now(),
                price: price_from_int(110),
                volume: None,
            },
        ],
    };
    let score = calculate_liquidity_score(&history_no_volume);
    assert_eq!(score, 0.5, "No volume data should return 0.5");

    // ケース2: 小さい取引量
    let history_small_volume = PriceHistory {
        token: "test.token".parse::<CommonTokenOutAccount>().unwrap(),
        quote_token: "wrap.near".parse::<CommonTokenInAccount>().unwrap(),
        prices: vec![
            PricePoint {
                timestamp: Utc::now(),
                price: price_from_int(100),
                volume: Some(BigDecimal::from(1000)),
            },
            PricePoint {
                timestamp: Utc::now(),
                price: price_from_int(110),
                volume: Some(BigDecimal::from(2000)),
            },
        ],
    };
    let score = calculate_liquidity_score(&history_small_volume);
    assert!(
        (0.0..=0.5).contains(&score),
        "Small volume should return low score, got: {}",
        score
    );

    // ケース3: 大きい取引量
    let history_large_volume = PriceHistory {
        token: "test.token".parse::<CommonTokenOutAccount>().unwrap(),
        quote_token: "wrap.near".parse::<CommonTokenInAccount>().unwrap(),
        prices: vec![
            PricePoint {
                timestamp: Utc::now(),
                price: price_from_int(100),
                volume: Some(BigDecimal::from(10u128.pow(25))), // 10 NEAR相当
            },
            PricePoint {
                timestamp: Utc::now(),
                price: price_from_int(110),
                volume: Some(BigDecimal::from(10u128.pow(25))),
            },
        ],
    };
    let score = calculate_liquidity_score(&history_large_volume);
    assert!(score > 0.4, "Large volume should return higher score");
}

#[tokio::test]
async fn test_estimate_market_cap_async() {
    // モック実装を作成してテスト
    // 1M トークン（decimals=24）の場合、smallest units では 10^30
    struct MockClient;
    impl crate::jsonrpc::ViewContract for MockClient {
        async fn view_contract<T>(
            &self,
            _receiver: &near_sdk::AccountId,
            _method_name: &str,
            _args: &T,
        ) -> crate::Result<near_primitives::views::CallResult>
        where
            T: ?Sized + serde::Serialize + Sync,
        {
            // 1M tokens * 10^24 = 10^30 smallest units
            let total_supply = "1000000000000000000000000000000";
            Ok(near_primitives::views::CallResult {
                result: serde_json::to_vec(total_supply).unwrap(),
                logs: vec![],
            })
        }
    }

    let client = MockClient;

    // ケース1: 1 NEAR/token
    // total_supply = 10^30 smallest_units = 10^6 whole tokens（decimals=24）
    // market_cap = 10^6 tokens × 1 NEAR/token = 1M NEAR
    let price_1_near = TokenPrice::from_near_per_token(BigDecimal::from(1));
    let market_cap = estimate_market_cap_async(&client, "test.token", &price_1_near, 24).await;
    let expected_1m = NearValue::from_near(BigDecimal::from(1_000_000));
    assert_eq!(
        market_cap, expected_1m,
        "1 NEAR/token × 1M tokens = 1M NEAR market cap"
    );

    // ケース2: 10 NEAR/token
    // total_supply = 10^30 smallest_units = 10^6 whole tokens（decimals=24）
    // market_cap = 10^6 tokens × 10 NEAR/token = 10M NEAR
    let price_10_near = TokenPrice::from_near_per_token(BigDecimal::from(10));
    let market_cap = estimate_market_cap_async(&client, "test.token", &price_10_near, 24).await;
    let expected_10m = NearValue::from_near(BigDecimal::from(10_000_000));
    assert_eq!(
        market_cap, expected_10m,
        "10 NEAR/token × 1M tokens = 10M NEAR market cap"
    );
}

#[tokio::test]
async fn test_get_token_total_supply() {
    // モック実装を作成してテスト
    struct MockClient;
    impl crate::jsonrpc::ViewContract for MockClient {
        async fn view_contract<T>(
            &self,
            _receiver: &near_sdk::AccountId,
            method_name: &str,
            _args: &T,
        ) -> crate::Result<near_primitives::views::CallResult>
        where
            T: ?Sized + serde::Serialize + Sync,
        {
            match method_name {
                "ft_total_supply" => {
                    let total_supply = "1000000000000000000000000"; // 10^24 smallest units
                    Ok(near_primitives::views::CallResult {
                        result: serde_json::to_vec(total_supply).unwrap(),
                        logs: vec![],
                    })
                }
                _ => Err(anyhow::anyhow!("Unexpected method: {}", method_name)),
            }
        }
    }

    let client = MockClient;

    // decimals=18 の場合: 10^24 smallest_units = 10^6 whole tokens
    let result = get_token_total_supply(&client, "test.token", 18)
        .await
        .unwrap();
    let expected = TokenAmount::from_smallest_units(
        BigDecimal::from_str("1000000000000000000000000").unwrap(),
        18,
    );
    assert_eq!(result, expected);

    // decimals を確認
    assert_eq!(result.decimals(), 18);

    // whole tokens に変換して確認
    let whole_tokens = result.to_whole();
    assert_eq!(whole_tokens, BigDecimal::from(1_000_000)); // 10^6 whole tokens
}

#[tokio::test]
async fn test_calculate_enhanced_liquidity_score() {
    // 拡張流動性スコアのテスト
    struct MockClient;
    impl crate::jsonrpc::ViewContract for MockClient {
        async fn view_contract<T>(
            &self,
            _receiver: &near_sdk::AccountId,
            method_name: &str,
            _args: &T,
        ) -> crate::Result<near_primitives::views::CallResult>
        where
            T: ?Sized + serde::Serialize + Sync,
        {
            match method_name {
                "ft_balance_of" => {
                    // 高い流動性を模擬（100 NEAR相当のプール残高）
                    let balance = (100u128 * 10u128.pow(24)).to_string(); // 100 NEAR
                    Ok(near_primitives::views::CallResult {
                        result: serde_json::to_vec(&balance).unwrap(),
                        logs: vec![],
                    })
                }
                _ => Err(anyhow::anyhow!("Unexpected method: {}", method_name)),
            }
        }
    }

    let client = MockClient;

    // テスト用の取引履歴（中程度の取引量）
    let history = zaciraci_common::algorithm::types::PriceHistory {
        token: "test.token".parse::<CommonTokenOutAccount>().unwrap(),
        quote_token: "wrap.near".parse::<CommonTokenInAccount>().unwrap(),
        prices: vec![zaciraci_common::algorithm::types::PricePoint {
            timestamp: chrono::Utc::now(),
            price: price_from_int(100),
            volume: Some(BigDecimal::from(5u128 * 10u128.pow(24))), // 5 NEAR相当の取引量
        }],
    };

    let score = calculate_enhanced_liquidity_score(&client, "test.token", &history).await;

    // プール流動性が高いため、スコアは0.5以上になるはず
    assert!(
        score >= 0.5,
        "Enhanced liquidity score should be >= 0.5 with high pool liquidity, got: {}",
        score
    );
    assert!(
        score <= 1.0,
        "Enhanced liquidity score should be <= 1.0, got: {}",
        score
    );
}

#[tokio::test]
async fn test_get_token_pool_liquidity() {
    // プール流動性取得のテスト
    struct MockClient;
    impl crate::jsonrpc::ViewContract for MockClient {
        async fn view_contract<T>(
            &self,
            receiver: &near_sdk::AccountId,
            method_name: &str,
            _args: &T,
        ) -> crate::Result<near_primitives::views::CallResult>
        where
            T: ?Sized + serde::Serialize + Sync,
        {
            match method_name {
                "ft_balance_of" => {
                    // テスト用の残高（50 NEAR相当）
                    let balance = (50u128 * 10u128.pow(24)).to_string();
                    Ok(near_primitives::views::CallResult {
                        result: serde_json::to_vec(&balance).unwrap(),
                        logs: vec![],
                    })
                }
                _ => Err(anyhow::anyhow!(
                    "Unexpected method {} for {}",
                    method_name,
                    receiver
                )),
            }
        }
    }

    let client = MockClient;
    let ref_account = "v2.ref-finance.near"
        .parse::<near_sdk::AccountId>()
        .unwrap();

    let result = get_token_pool_liquidity(&client, &ref_account, "test.token")
        .await
        .unwrap();

    assert_eq!(result, 50u128 * 10u128.pow(24)); // 50 NEAR
}

#[test]
fn test_sqrt_bigdecimal() {
    use std::str::FromStr;

    // ケース1: 完全平方数
    let value = BigDecimal::from(4);
    let result = sqrt_bigdecimal(&value).unwrap();
    let expected = BigDecimal::from(2);
    assert!((result - expected).abs() < BigDecimal::from_str("0.000001").unwrap());

    // ケース2: 非完全平方数
    let value = BigDecimal::from(2);
    let result = sqrt_bigdecimal(&value).unwrap();
    let expected = BigDecimal::from_str("1.41421356").unwrap();
    assert!((result - expected).abs() < BigDecimal::from_str("0.00001").unwrap());

    // ケース3: 小数
    let value = BigDecimal::from_str("0.25").unwrap();
    let result = sqrt_bigdecimal(&value).unwrap();
    let expected = BigDecimal::from_str("0.5").unwrap();
    assert!((result - expected).abs() < BigDecimal::from_str("0.000001").unwrap());

    // ケース4: ゼロ
    let value = BigDecimal::from(0);
    let result = sqrt_bigdecimal(&value).unwrap();
    assert_eq!(result, BigDecimal::from(0));

    // ケース5: 負の数（エラーケース）
    let value = BigDecimal::from(-1);
    let result = sqrt_bigdecimal(&value);
    assert!(result.is_err());
}

#[test]
fn test_calculate_volatility_from_history() {
    use chrono::Utc;
    use zaciraci_common::algorithm::types::{PriceHistory, PricePoint};

    // ケース1: データポイントが不足
    let history_insufficient = PriceHistory {
        token: "test.token".parse::<CommonTokenOutAccount>().unwrap(),
        quote_token: "wrap.near".parse::<CommonTokenInAccount>().unwrap(),
        prices: vec![PricePoint {
            timestamp: Utc::now(),
            price: price_from_int(100),
            volume: None,
        }],
    };
    let result = calculate_volatility_from_history(&history_insufficient);
    assert!(result.is_err(), "Should error with insufficient data");

    // ケース2: 価格変動なし
    let history_no_change = PriceHistory {
        token: "test.token".parse::<CommonTokenOutAccount>().unwrap(),
        quote_token: "wrap.near".parse::<CommonTokenInAccount>().unwrap(),
        prices: vec![
            PricePoint {
                timestamp: Utc::now(),
                price: price_from_int(100),
                volume: None,
            },
            PricePoint {
                timestamp: Utc::now(),
                price: price_from_int(100),
                volume: None,
            },
            PricePoint {
                timestamp: Utc::now(),
                price: price_from_int(100),
                volume: None,
            },
        ],
    };
    let volatility = calculate_volatility_from_history(&history_no_change).unwrap();
    assert_eq!(
        volatility,
        BigDecimal::from(0),
        "No price change should have 0 volatility"
    );

    // ケース3: 価格変動あり
    let history_with_change = PriceHistory {
        token: "test.token".parse::<CommonTokenOutAccount>().unwrap(),
        quote_token: "wrap.near".parse::<CommonTokenInAccount>().unwrap(),
        prices: vec![
            PricePoint {
                timestamp: Utc::now(),
                price: price_from_int(100),
                volume: None,
            },
            PricePoint {
                timestamp: Utc::now(),
                price: price_from_int(110),
                volume: None,
            },
            PricePoint {
                timestamp: Utc::now(),
                price: price_from_int(105),
                volume: None,
            },
        ],
    };
    let volatility = calculate_volatility_from_history(&history_with_change).unwrap();
    assert!(
        volatility > 0,
        "Price changes should result in positive volatility"
    );

    // ケース4: ゼロ価格を含む（スキップされるべき）
    let history_with_zero = PriceHistory {
        token: "test.token".parse::<CommonTokenOutAccount>().unwrap(),
        quote_token: "wrap.near".parse::<CommonTokenInAccount>().unwrap(),
        prices: vec![
            PricePoint {
                timestamp: Utc::now(),
                price: price_from_int(100),
                volume: None,
            },
            PricePoint {
                timestamp: Utc::now(),
                price: price_from_int(0),
                volume: None,
            },
            PricePoint {
                timestamp: Utc::now(),
                price: price_from_int(110),
                volume: None,
            },
        ],
    };
    let volatility = calculate_volatility_from_history(&history_with_zero).unwrap();
    assert!(
        volatility >= 0,
        "Should calculate volatility skipping zero prices, got: {}",
        volatility
    );
}

#[test]
fn test_liquidity_score_calculation_formula() {
    // 流動性スコアの数学的検証
    // score = ratio / (1 + ratio) where ratio = liquidity / threshold

    // liquidity == threshold の場合: ratio = 1 → score = 0.5
    let ratio: f64 = 1.0;
    let score: f64 = ratio / (1.0 + ratio);
    assert!((score - 0.5_f64).abs() < 0.001);

    // liquidity == 2 * threshold の場合: ratio = 2 → score = 0.667
    let ratio: f64 = 2.0;
    let score: f64 = ratio / (1.0 + ratio);
    assert!((score - 0.667_f64).abs() < 0.001);

    // liquidity == 0.5 * threshold の場合: ratio = 0.5 → score = 0.333
    let ratio: f64 = 0.5;
    let score: f64 = ratio / (1.0 + ratio);
    assert!((score - 0.333_f64).abs() < 0.001);
}
