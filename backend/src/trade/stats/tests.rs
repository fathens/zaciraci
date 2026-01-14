use super::*;
use crate::ref_finance::token_account::TokenAccount;
use std::str::FromStr;
use zaciraci_common::types::TokenPrice;

fn price_from_int(v: i64) -> TokenPrice {
    TokenPrice::from_near_per_token(BigDecimal::from(v))
}

#[test]
fn test_describes() {
    let stats: ListStatsInPeriod<BigDecimal> = ListStatsInPeriod(vec![]);
    assert!(stats.describes().is_empty());
}

#[test]
fn test_describes_increase() {
    let stats: ListStatsInPeriod<BigDecimal> = ListStatsInPeriod(vec![
        StatsInPeriod {
            timestamp: NaiveDateTime::parse_from_str(
                "2025-03-26 11:37:48.195977",
                "%Y-%m-%d %H:%M:%S%.f",
            )
            .unwrap(),
            period: Duration::minutes(1),
            start: BigDecimal::from(101),
            end: BigDecimal::from(100),
            max: BigDecimal::from(102),
            min: BigDecimal::from(90),
            average: BigDecimal::from(95),
        },
        StatsInPeriod {
            timestamp: NaiveDateTime::parse_from_str(
                "2025-03-27 11:37:48.196150",
                "%Y-%m-%d %H:%M:%S%.f",
            )
            .unwrap(),
            period: Duration::minutes(1),
            start: BigDecimal::from(100),
            end: BigDecimal::from(150),
            max: BigDecimal::from(155),
            min: BigDecimal::from(140),
            average: BigDecimal::from(147),
        },
    ]);
    let descriptions = stats.describes();
    assert_eq!(descriptions.len(), 2);
    assert!(descriptions[1].contains("increase"));
    assert!(descriptions[1].contains("50 %"));
    assert_eq!(
        descriptions,
        vec![
            "2025-03-26 11:37:48.195977, opened at 101, closed at 100, with a high of 102, a low of 90, and an average of 95",
            "2025-03-27 11:37:48.196150, opened at 100, closed at 150, with a high of 155, a low of 140, and an average of 147, marking a 50 % increase from the previous 1 minutes"
        ]
    );
}

#[test]
fn test_describes_decrease() {
    let stats: ListStatsInPeriod<BigDecimal> = ListStatsInPeriod(vec![
        StatsInPeriod {
            timestamp: NaiveDateTime::parse_from_str(
                "2025-03-26 11:37:48.195977",
                "%Y-%m-%d %H:%M:%S%.f",
            )
            .unwrap(),
            period: Duration::minutes(1),
            start: BigDecimal::from(100),
            end: BigDecimal::from(100),
            max: BigDecimal::from(100),
            min: BigDecimal::from(100),
            average: BigDecimal::from(100),
        },
        StatsInPeriod {
            timestamp: NaiveDateTime::parse_from_str(
                "2025-03-27 11:37:48.196150",
                "%Y-%m-%d %H:%M:%S%.f",
            )
            .unwrap(),
            period: Duration::minutes(1),
            start: BigDecimal::from(100),
            end: BigDecimal::from(50),
            max: BigDecimal::from(50),
            min: BigDecimal::from(50),
            average: BigDecimal::from(50),
        },
    ]);
    let descriptions = stats.describes();
    assert_eq!(descriptions.len(), 2);
    assert!(descriptions[1].contains("decrease"));
    assert!(descriptions[1].contains("50 %"));
    assert_eq!(
        descriptions,
        vec![
            "2025-03-26 11:37:48.195977, opened at 100, closed at 100, with a high of 100, a low of 100, and an average of 100",
            "2025-03-27 11:37:48.196150, opened at 100, closed at 50, with a high of 50, a low of 50, and an average of 50, marking a -50 % decrease from the previous 1 minutes"
        ]
    );
}

#[test]
fn test_describes_no_change() {
    let stats: ListStatsInPeriod<BigDecimal> = ListStatsInPeriod(vec![
        StatsInPeriod {
            timestamp: NaiveDateTime::parse_from_str(
                "2025-03-26 11:37:48.195977",
                "%Y-%m-%d %H:%M:%S%.f",
            )
            .unwrap(),
            period: Duration::minutes(1),
            start: BigDecimal::from_str("100.123456789").unwrap(),
            end: BigDecimal::from_str("100.123456789").unwrap(),
            max: BigDecimal::from_str("100.123456789").unwrap(),
            min: BigDecimal::from_str("100.123456789").unwrap(),
            average: BigDecimal::from_str("100.123456789").unwrap(),
        },
        StatsInPeriod {
            timestamp: NaiveDateTime::parse_from_str(
                "2025-03-27 11:37:48.196150",
                "%Y-%m-%d %H:%M:%S%.f",
            )
            .unwrap(),
            period: Duration::minutes(1),
            start: BigDecimal::from_str("100.123456789").unwrap(),
            end: BigDecimal::from_str("100.123456789").unwrap(),
            max: BigDecimal::from_str("100.123456789").unwrap(),
            min: BigDecimal::from_str("100.123456789").unwrap(),
            average: BigDecimal::from_str("100.123456789").unwrap(),
        },
    ]);
    let descriptions = stats.describes();
    assert_eq!(descriptions.len(), 2);
    assert!(descriptions[1].contains("no change"));
    assert_eq!(
        descriptions,
        vec![
            "2025-03-26 11:37:48.195977, opened at 100.123456789, closed at 100.123456789, with a high of 100.123456789, a low of 100.123456789, and an average of 100.123456789",
            "2025-03-27 11:37:48.196150, opened at 100.123456789, closed at 100.123456789, with a high of 100.123456789, a low of 100.123456789, and an average of 100.123456789, no change from the previous 1 minutes"
        ]
    );
}

#[test]
fn test_stats_empty() {
    // 空のポイントリストを持つSameBaseTokenRatesを作成
    let rates = SameBaseTokenRates {
        points: Vec::new(),
        base: "wrap.near".parse::<TokenAccount>().unwrap().into(),
        quote: "usdt.tether-token.near"
            .parse::<TokenAccount>()
            .unwrap()
            .into(),
    };

    // 1分間の期間で統計を計算
    let stats = rates.aggregate(Duration::minutes(1));

    // 結果が空のベクターであることを確認
    assert!(stats.0.is_empty());
}

#[test]
fn test_stats_single_period() {
    // 1つの期間内に複数のポイントを持つSameBaseTokenRatesを作成
    let base_time =
        NaiveDateTime::parse_from_str("2025-03-26 10:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    let points = vec![
        Point {
            timestamp: base_time,
            rate: BigDecimal::from(100),
        },
        Point {
            timestamp: base_time + Duration::seconds(20),
            rate: BigDecimal::from(110),
        },
        Point {
            timestamp: base_time + Duration::seconds(40),
            rate: BigDecimal::from(90),
        },
    ];

    let rates = SameBaseTokenRates {
        points,
        base: "wrap.near".parse::<TokenAccount>().unwrap().into(),
        quote: "usdt.tether-token.near"
            .parse::<TokenAccount>()
            .unwrap()
            .into(),
    };

    // 1分間の期間で統計を計算
    let stats = rates.aggregate(Duration::minutes(1));

    // 結果を検証
    assert_eq!(stats.0.len(), 1);
    let stat = &stats.0[0];

    assert_eq!(stat.timestamp, base_time);
    assert_eq!(stat.period, Duration::minutes(1));
    assert_eq!(stat.start, BigDecimal::from(100));
    assert_eq!(stat.end, BigDecimal::from(90));
    assert_eq!(stat.max, BigDecimal::from(110));
    assert_eq!(stat.min, BigDecimal::from(90));

    // 平均値の検証 (100 + 110 + 90) / 3 = 100
    assert_eq!(stat.average, BigDecimal::from(100));
}

#[test]
fn test_stats_multiple_periods() {
    // 複数の期間にまたがるポイントを持つSameBaseTokenRatesを作成
    let base_time =
        NaiveDateTime::parse_from_str("2025-03-26 10:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    let points = vec![
        // 最初の期間 (10:00:00 - 10:01:00)
        Point {
            timestamp: base_time,
            rate: BigDecimal::from(100),
        },
        Point {
            timestamp: base_time + Duration::seconds(30),
            rate: BigDecimal::from(110),
        },
        // 2番目の期間 (10:01:00 - 10:02:00)
        Point {
            timestamp: base_time + Duration::minutes(1),
            rate: BigDecimal::from(120),
        },
        Point {
            timestamp: base_time + Duration::minutes(1) + Duration::seconds(30),
            rate: BigDecimal::from(130),
        },
        // 3番目の期間 (10:02:00 - 10:03:00)
        Point {
            timestamp: base_time + Duration::minutes(2),
            rate: BigDecimal::from(140),
        },
        Point {
            timestamp: base_time + Duration::minutes(2) + Duration::seconds(30),
            rate: BigDecimal::from(150),
        },
    ];

    let rates = SameBaseTokenRates {
        points,
        base: "wrap.near".parse::<TokenAccount>().unwrap().into(),
        quote: "usdt.tether-token.near"
            .parse::<TokenAccount>()
            .unwrap()
            .into(),
    };

    // 1分間の期間で統計を計算
    let stats = rates.aggregate(Duration::minutes(1));

    // 結果を検証
    assert_eq!(stats.0.len(), 3);

    // 最初の期間の検証
    {
        let stat = &stats.0[0];
        assert_eq!(stat.timestamp, base_time);
        assert_eq!(stat.period, Duration::minutes(1));
        assert_eq!(stat.start, BigDecimal::from(100));
        assert_eq!(stat.end, BigDecimal::from(110));
        assert_eq!(stat.max, BigDecimal::from(110));
        assert_eq!(stat.min, BigDecimal::from(100));
        assert_eq!(stat.average, BigDecimal::from(105)); // (100 + 110) / 2 = 105
    }

    // 2番目の期間の検証
    {
        let stat = &stats.0[1];
        assert_eq!(stat.timestamp, base_time + Duration::minutes(1));
        assert_eq!(stat.period, Duration::minutes(1));
        assert_eq!(stat.start, BigDecimal::from(120));
        assert_eq!(stat.end, BigDecimal::from(130));
        assert_eq!(stat.max, BigDecimal::from(130));
        assert_eq!(stat.min, BigDecimal::from(120));
        assert_eq!(stat.average, BigDecimal::from(125)); // (120 + 130) / 2 = 125
    }

    // 3番目の期間の検証
    {
        let stat = &stats.0[2];
        assert_eq!(stat.timestamp, base_time + Duration::minutes(2));
        assert_eq!(stat.period, Duration::minutes(1));
        assert_eq!(stat.start, BigDecimal::from(140));
        assert_eq!(stat.end, BigDecimal::from(150));
        assert_eq!(stat.max, BigDecimal::from(150));
        assert_eq!(stat.min, BigDecimal::from(140));
        assert_eq!(stat.average, BigDecimal::from(145)); // (140 + 150) / 2 = 145
    }
}

#[test]
fn test_stats_period_boundary() {
    // 期間の境界値をテストするためのポイントを持つSameBaseTokenRatesを作成
    let base_time =
        NaiveDateTime::parse_from_str("2025-03-26 10:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    let points = vec![
        // 最初の期間 (10:00:00 - 10:05:00)
        Point {
            timestamp: base_time,
            rate: BigDecimal::from(100),
        },
        // 境界値ちょうど (10:05:00) - 次の期間に含まれる
        Point {
            timestamp: base_time + Duration::minutes(5),
            rate: BigDecimal::from(200),
        },
        // 2番目の期間 (10:05:00 - 10:10:00)
        Point {
            timestamp: base_time + Duration::minutes(7),
            rate: BigDecimal::from(300),
        },
    ];

    let rates = SameBaseTokenRates {
        points,
        base: "wrap.near".parse::<TokenAccount>().unwrap().into(),
        quote: "usdt.tether-token.near"
            .parse::<TokenAccount>()
            .unwrap()
            .into(),
    };

    // 5分間の期間で統計を計算
    let stats = rates.aggregate(Duration::minutes(5));

    // 結果を検証
    assert_eq!(stats.0.len(), 2);

    // 最初の期間の検証
    {
        let stat = &stats.0[0];
        assert_eq!(stat.timestamp, base_time);
        assert_eq!(stat.period, Duration::minutes(5));
        assert_eq!(stat.start, BigDecimal::from(100));
        assert_eq!(stat.end, BigDecimal::from(100));
        assert_eq!(stat.max, BigDecimal::from(100));
        assert_eq!(stat.min, BigDecimal::from(100));
        assert_eq!(stat.average, BigDecimal::from(100));
    }

    // 2番目の期間の検証 (境界値を含む)
    {
        let stat = &stats.0[1];
        assert_eq!(stat.timestamp, base_time + Duration::minutes(5));
        assert_eq!(stat.period, Duration::minutes(5));
        assert_eq!(stat.start, BigDecimal::from(200));
        assert_eq!(stat.end, BigDecimal::from(300));
        assert_eq!(stat.max, BigDecimal::from(300));
        assert_eq!(stat.min, BigDecimal::from(200));
        assert_eq!(stat.average, BigDecimal::from(250)); // (200 + 300) / 2 = 250
    }
}

#[test]
fn test_calculate_liquidity_score() {
    use chrono::Utc;
    use zaciraci_common::algorithm::types::{PriceHistory, PricePoint};

    // ケース1: 取引量データなし
    let history_no_volume = PriceHistory {
        token: "test.token".to_string(),
        quote_token: "wrap.near".to_string(),
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
        token: "test.token".to_string(),
        quote_token: "wrap.near".to_string(),
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
        token: "test.token".to_string(),
        quote_token: "wrap.near".to_string(),
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
        token: "test.token".to_string(),
        quote_token: "wrap.near".to_string(),
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
        token: "test.token".to_string(),
        quote_token: "wrap.near".to_string(),
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
        token: "test.token".to_string(),
        quote_token: "wrap.near".to_string(),
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
        token: "test.token".to_string(),
        quote_token: "wrap.near".to_string(),
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
        volatility > BigDecimal::from(0),
        "Price changes should result in positive volatility"
    );

    // ケース4: ゼロ価格を含む（スキップされるべき）
    let history_with_zero = PriceHistory {
        token: "test.token".to_string(),
        quote_token: "wrap.near".to_string(),
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
        volatility >= BigDecimal::from(0),
        "Should calculate volatility skipping zero prices, got: {}",
        volatility
    );
}

#[test]
fn test_format_decimal_digits() {
    // 整数値のテスト
    assert_eq!(
        "100",
        ListStatsInPeriod::<BigDecimal>::format_decimal(BigDecimal::from(100))
    );

    // 小数点以下が全て0の値
    let with_zeros = BigDecimal::from(100) + BigDecimal::from_str("0.000000000").unwrap();
    assert_eq!(
        "100",
        ListStatsInPeriod::<BigDecimal>::format_decimal(with_zeros)
    );

    // 小数点以下が1桁の値
    assert_eq!(
        "0.1",
        ListStatsInPeriod::<BigDecimal>::format_decimal(BigDecimal::from_str("0.1").unwrap())
    );

    // 小数点以下が2桁の値
    assert_eq!(
        "0.12",
        ListStatsInPeriod::<BigDecimal>::format_decimal(BigDecimal::from_str("0.12").unwrap())
    );

    // 小数点以下が3桁の値
    assert_eq!(
        "0.123",
        ListStatsInPeriod::<BigDecimal>::format_decimal(BigDecimal::from_str("0.123").unwrap())
    );

    // 小数点以下が4桁の値
    assert_eq!(
        "0.1234",
        ListStatsInPeriod::<BigDecimal>::format_decimal(BigDecimal::from_str("0.1234").unwrap())
    );

    // 小数点以下が5桁の値
    assert_eq!(
        "0.12345",
        ListStatsInPeriod::<BigDecimal>::format_decimal(BigDecimal::from_str("0.12345").unwrap())
    );

    // 小数点以下が6桁の値
    assert_eq!(
        "0.123456",
        ListStatsInPeriod::<BigDecimal>::format_decimal(BigDecimal::from_str("0.123456").unwrap())
    );

    // 小数点以下が7桁の値
    assert_eq!(
        "0.1234567",
        ListStatsInPeriod::<BigDecimal>::format_decimal(BigDecimal::from_str("0.1234567").unwrap())
    );

    // 小数点以下が8桁の値
    assert_eq!(
        "0.12345678",
        ListStatsInPeriod::<BigDecimal>::format_decimal(
            BigDecimal::from_str("0.12345678").unwrap()
        )
    );

    // 小数点以下が9桁の値
    assert_eq!(
        "0.123456789",
        ListStatsInPeriod::<BigDecimal>::format_decimal(
            BigDecimal::from_str("0.123456789").unwrap()
        )
    );

    // 小数点以下が10桁の値（9桁までに制限される）
    assert_eq!(
        "0.123456789",
        ListStatsInPeriod::<BigDecimal>::format_decimal(
            BigDecimal::from_str("0.1234567891").unwrap()
        )
    );

    // 末尾に0がある場合（末尾の0は削除される）
    assert_eq!(
        "0.12345",
        ListStatsInPeriod::<BigDecimal>::format_decimal(
            BigDecimal::from_str("0.12345000").unwrap()
        )
    );

    // 整数部分あり、小数点以下4桁の値
    assert_eq!(
        "123.4567",
        ListStatsInPeriod::<BigDecimal>::format_decimal(BigDecimal::from_str("123.4567").unwrap())
    );
}

#[test]
fn test_filter_tokens_to_liquidate_excludes_wrap_near() {
    use crate::ref_finance::token_account::TokenAccount;
    use near_sdk::json_types::U128;
    use std::collections::HashMap;

    let wrap_near: TokenAccount = "wrap.near".parse().unwrap();
    let token_a: TokenAccount = "token_a.near".parse().unwrap();

    let mut deposits = HashMap::new();
    deposits.insert(wrap_near.clone(), U128(1000));
    deposits.insert(token_a.clone(), U128(500));

    let result = super::filter_tokens_to_liquidate(&deposits, &wrap_near);

    assert_eq!(result.len(), 1);
    assert!(result.contains(&"token_a.near".to_string()));
    assert!(!result.contains(&"wrap.near".to_string()));
}

#[test]
fn test_filter_tokens_to_liquidate_excludes_zero_balance() {
    use crate::ref_finance::token_account::TokenAccount;
    use near_sdk::json_types::U128;
    use std::collections::HashMap;

    let wrap_near: TokenAccount = "wrap.near".parse().unwrap();
    let token_a: TokenAccount = "token_a.near".parse().unwrap();
    let token_b: TokenAccount = "token_b.near".parse().unwrap();

    let mut deposits = HashMap::new();
    deposits.insert(token_a.clone(), U128(500));
    deposits.insert(token_b.clone(), U128(0)); // ゼロ残高

    let result = super::filter_tokens_to_liquidate(&deposits, &wrap_near);

    assert_eq!(result.len(), 1);
    assert!(result.contains(&"token_a.near".to_string()));
    assert!(!result.contains(&"token_b.near".to_string()));
}

#[test]
fn test_filter_tokens_to_liquidate_includes_tokens_with_balance() {
    use crate::ref_finance::token_account::TokenAccount;
    use near_sdk::json_types::U128;
    use std::collections::HashMap;

    let wrap_near: TokenAccount = "wrap.near".parse().unwrap();
    let token_a: TokenAccount = "token_a.near".parse().unwrap();
    let token_b: TokenAccount = "token_b.near".parse().unwrap();
    let token_c: TokenAccount = "token_c.near".parse().unwrap();

    let mut deposits = HashMap::new();
    deposits.insert(wrap_near.clone(), U128(1000)); // 除外されるべき
    deposits.insert(token_a.clone(), U128(500));
    deposits.insert(token_b.clone(), U128(0)); // 除外されるべき
    deposits.insert(token_c.clone(), U128(750));

    let result = super::filter_tokens_to_liquidate(&deposits, &wrap_near);

    assert_eq!(result.len(), 2);
    assert!(result.contains(&"token_a.near".to_string()));
    assert!(result.contains(&"token_c.near".to_string()));
    assert!(!result.contains(&"wrap.near".to_string()));
    assert!(!result.contains(&"token_b.near".to_string()));
}

#[test]
fn test_filter_tokens_to_liquidate_empty_deposits() {
    use crate::ref_finance::token_account::TokenAccount;
    use std::collections::HashMap;

    let wrap_near: TokenAccount = "wrap.near".parse().unwrap();
    let deposits = HashMap::new();

    let result = super::filter_tokens_to_liquidate(&deposits, &wrap_near);

    assert!(result.is_empty());
}

#[test]
fn test_filter_tokens_to_liquidate_only_wrap_near() {
    use crate::ref_finance::token_account::TokenAccount;
    use near_sdk::json_types::U128;
    use std::collections::HashMap;

    let wrap_near: TokenAccount = "wrap.near".parse().unwrap();

    let mut deposits = HashMap::new();
    deposits.insert(wrap_near.clone(), U128(1000));

    let result = super::filter_tokens_to_liquidate(&deposits, &wrap_near);

    assert!(result.is_empty());
}

// Rebalance logic tests
mod rebalance_tests {
    use bigdecimal::BigDecimal;
    use std::str::FromStr;
    use zaciraci_common::types::{ExchangeRate, NearValue, TokenAmount};

    #[test]
    fn test_rebalance_calculations_sell_only() {
        // Setup: Token A has 200 NEAR value, target is 100 NEAR
        // Should sell 100 NEAR worth of Token A
        let current_value = NearValue::from_near(BigDecimal::from(200));
        let target_value = NearValue::from_near(BigDecimal::from(100));
        let diff = &target_value - &current_value;

        assert_eq!(diff, NearValue::from_near(BigDecimal::from(-100)));
        assert!(diff < NearValue::zero());

        // ExchangeRate: raw_rate = 5e23 smallest_units/NEAR
        // つまり 1 NEAR で 0.5e24 = 0.5 tokens を取得 (price = 2 NEAR/token)
        // 100 NEAR × 5e23 = 5e25 = 50e24 smallest_units = 50 tokens
        let rate = ExchangeRate::from_raw_rate(
            BigDecimal::from_str("500000000000000000000000").unwrap(), // 5e23
            24,
        );
        let token_amount: TokenAmount = &diff.abs() * &rate;

        // Expected: 50 tokens = 50e24 smallest units
        let expected = BigDecimal::from_str("50000000000000000000000000").unwrap(); // 50e24
        assert_eq!(token_amount.smallest_units(), &expected);
    }

    #[test]
    fn test_rebalance_calculations_buy_only() {
        // Setup: Token B has 50 NEAR value, target is 100 NEAR
        // Should buy 50 NEAR worth of Token B
        let current_value = NearValue::from_near(BigDecimal::from(50));
        let target_value = NearValue::from_near(BigDecimal::from(100));
        let diff = &target_value - &current_value;

        assert_eq!(diff, NearValue::from_near(BigDecimal::from(50)));
        assert!(diff > NearValue::zero());

        // For buying, we use wrap.near amount directly (no token conversion needed)
        let wrap_near_amount = diff;
        assert_eq!(wrap_near_amount, NearValue::from_near(BigDecimal::from(50)));
    }

    #[test]
    fn test_rebalance_minimum_trade_size() {
        // Minimum trade size is 1 NEAR
        let min_trade_size = NearValue::one();

        // Small difference: 0.5 NEAR
        let small_diff = NearValue::from_near(BigDecimal::from_str("0.5").unwrap());
        assert!(small_diff < min_trade_size);

        // Large difference: 2 NEAR
        let large_diff = NearValue::from_near(BigDecimal::from(2));
        assert!(large_diff >= min_trade_size);
    }

    #[test]
    fn test_token_amount_conversion() {
        // Test: Convert NEAR value to token amount
        // If 100 NEAR worth should be sold, and price = 2 NEAR/token
        // Then token_amount = 100 NEAR × (0.5 tokens/NEAR) = 50 tokens
        //
        // raw_rate = 5e23 smallest_units/NEAR (価格の逆数)
        // 計算: 100 NEAR × 5e23 = 5e25 = 50e24 = 50 tokens
        let wrap_near_value = NearValue::from_near(BigDecimal::from(100));
        let rate = ExchangeRate::from_raw_rate(
            BigDecimal::from_str("500000000000000000000000").unwrap(), // 5e23
            24,
        );
        let token_amount: TokenAmount = &wrap_near_value * &rate;

        // Expected: 50 tokens = 50e24 smallest units
        let expected = BigDecimal::from_str("50000000000000000000000000").unwrap();
        assert_eq!(token_amount.smallest_units(), &expected);
    }

    #[test]
    fn test_wrap_near_value_calculation() {
        // Test: Calculate current value in NEAR
        // If balance is 100 tokens and price = 2 NEAR/token
        // Then value = 100 tokens × 2 NEAR/token = 200 NEAR
        //
        // raw_rate = 5e23 smallest_units/NEAR (価格 2 NEAR/token の逆数)
        // 計算: 100e24 / 5e23 = 200 NEAR
        let balance = TokenAmount::from_smallest_units(
            BigDecimal::from_str("100000000000000000000000000").unwrap(), // 100e24 = 100 tokens
            24,
        );
        let rate = ExchangeRate::from_raw_rate(
            BigDecimal::from_str("500000000000000000000000").unwrap(), // 5e23
            24,
        );
        let value: NearValue = &balance / &rate;

        assert_eq!(value, NearValue::from_near(BigDecimal::from(200)));
    }

    #[test]
    fn test_two_phase_rebalance_scenario() {
        // Scenario: Portfolio with 2 tokens
        // Total value: 300 NEAR
        // Target weights: Token A = 40%, Token B = 60%
        // Current: Token A = 200 NEAR, Token B = 100 NEAR
        // Expected:
        //   Token A target = 120 NEAR -> sell 80 NEAR worth
        //   Token B target = 180 NEAR -> buy 80 NEAR worth

        let total_value = NearValue::from_near(BigDecimal::from(300));

        // Token A
        let token_a_current = NearValue::from_near(BigDecimal::from(200));
        let token_a_weight = 0.4;
        let token_a_target = &total_value * token_a_weight;
        let token_a_diff = &token_a_target - &token_a_current;

        // f64 は 0.4 を正確に表現できないため、tolerance-based で比較
        let target_a_f64 = token_a_target.to_f64().as_f64();
        assert!(
            (target_a_f64 - 120.0).abs() < 0.0001,
            "Token A target should be ~120 NEAR, got {}",
            target_a_f64
        );

        let diff_a_f64 = token_a_diff.to_f64().as_f64();
        assert!(
            (diff_a_f64 - (-80.0)).abs() < 0.0001,
            "Token A diff should be ~-80 NEAR, got {}",
            diff_a_f64
        );
        assert!(token_a_diff < NearValue::zero()); // Need to sell

        // Token B
        let token_b_current = NearValue::from_near(BigDecimal::from(100));
        let token_b_weight = 0.6;
        let token_b_target = &total_value * token_b_weight;
        let token_b_diff = &token_b_target - &token_b_current;

        let target_b_f64 = token_b_target.to_f64().as_f64();
        assert!(
            (target_b_f64 - 180.0).abs() < 0.0001,
            "Token B target should be ~180 NEAR, got {}",
            target_b_f64
        );

        let diff_b_f64 = token_b_diff.to_f64().as_f64();
        assert!(
            (diff_b_f64 - 80.0).abs() < 0.0001,
            "Token B diff should be ~80 NEAR, got {}",
            diff_b_f64
        );
        assert!(token_b_diff > NearValue::zero()); // Need to buy

        // Verify balance: sell amount ~= buy amount (within tolerance)
        let sell_amount = token_a_diff.abs().to_f64().as_f64();
        let buy_amount = token_b_diff.to_f64().as_f64();
        assert!(
            (sell_amount - buy_amount).abs() < 0.0001,
            "Sell and buy amounts should match: sell={}, buy={}",
            sell_amount,
            buy_amount
        );
    }

    #[test]
    fn test_rate_conversion_accuracy() {
        // Test precise conversion with realistic values
        // 1 Token = 2.5 NEAR (price = 2.5 NEAR/token)
        // raw_rate = 1e24 / 2.5 = 4e23 smallest_units/NEAR
        let rate = ExchangeRate::from_raw_rate(
            BigDecimal::from_str("400000000000000000000000").unwrap(), // 4e23
            24,
        );

        // Selling: 50 NEAR worth
        // token_amount = 50 NEAR × 4e23 = 2e25 = 20e24 = 20 tokens
        let wrap_near_value = NearValue::from_near(BigDecimal::from(50));
        let token_amount: TokenAmount = &wrap_near_value * &rate;

        // Expected: 20 tokens = 20e24 smallest units
        let expected = BigDecimal::from_str("20000000000000000000000000").unwrap();
        assert_eq!(token_amount.smallest_units(), &expected);

        // Verify roundtrip:
        // value * rate = amount (NearValue → TokenAmount, 乗算)
        // amount / rate = value (TokenAmount → NearValue, 除算)
        let reverse_value: NearValue = &token_amount / &rate;
        assert_eq!(reverse_value, wrap_near_value);
    }

    #[test]
    fn test_phase2_purchase_amount_adjustment() {
        // Scenario: Phase 2 needs to buy 3 tokens for total 300 wrap.near
        // But only 100 wrap.near is available after Phase 1
        // Should adjust all purchase amounts proportionally by factor 100/300 = 1/3

        let available_wrap_near = BigDecimal::from(100);
        let buy_operations = [
            BigDecimal::from(100), // Token A
            BigDecimal::from(100), // Token B
            BigDecimal::from(100), // Token C
        ];

        let total_buy_amount: BigDecimal = buy_operations.iter().sum();
        assert_eq!(total_buy_amount, BigDecimal::from(300));

        // Calculate adjustment factor
        let adjustment_factor = &available_wrap_near / &total_buy_amount;
        // Should be approximately 1/3
        let expected_min = BigDecimal::from_str("0.333").unwrap();
        let expected_max = BigDecimal::from_str("0.334").unwrap();
        assert!(adjustment_factor >= expected_min && adjustment_factor <= expected_max);

        // Apply adjustment to each purchase
        let adjusted_operations: Vec<BigDecimal> = buy_operations
            .iter()
            .map(|amount| amount * &adjustment_factor)
            .collect();

        // Each should be adjusted to ~33.33 wrap.near
        for adjusted in &adjusted_operations {
            assert!(
                adjusted > &BigDecimal::from_str("33.33").unwrap()
                    && adjusted < &BigDecimal::from_str("33.34").unwrap()
            );
        }

        // Total should approximately equal available balance (within rounding error)
        let adjusted_total: BigDecimal = adjusted_operations.iter().sum();
        let tolerance = BigDecimal::from_str("0.01").unwrap(); // Allow 0.01 tolerance
        let diff = (&adjusted_total - &available_wrap_near).abs();
        assert!(
            diff < tolerance,
            "Adjusted total {} should be close to available {}",
            adjusted_total,
            available_wrap_near
        );
    }

    #[test]
    fn test_phase2_no_adjustment_needed() {
        // Scenario: Available wrap.near (200) >= total buy amount (150)
        // No adjustment should be applied

        let available_wrap_near = BigDecimal::from(200);
        let buy_operations = vec![
            BigDecimal::from(50),
            BigDecimal::from(50),
            BigDecimal::from(50),
        ];

        let total_buy_amount: BigDecimal = buy_operations.iter().sum();
        assert_eq!(total_buy_amount, BigDecimal::from(150));

        // No adjustment needed
        assert!(total_buy_amount <= available_wrap_near);

        // Adjustment factor would be >= 1
        let adjustment_factor = &available_wrap_near / &total_buy_amount;
        assert!(adjustment_factor >= BigDecimal::from(1));

        // In this case, we use the original amounts
        let adjusted_operations = if total_buy_amount > available_wrap_near {
            buy_operations
                .iter()
                .map(|amount| amount * &adjustment_factor)
                .collect()
        } else {
            buy_operations.clone()
        };

        // Amounts should remain unchanged
        assert_eq!(adjusted_operations, buy_operations);
    }

    #[test]
    fn test_phase2_extreme_shortage() {
        // Scenario: Severe shortage - only 1 wrap.near available for 1000 wrap.near needed
        // Adjustment factor = 0.001

        let available_wrap_near = BigDecimal::from(1);
        let buy_operations = [
            BigDecimal::from(400),
            BigDecimal::from(300),
            BigDecimal::from(300),
        ];

        let total_buy_amount: BigDecimal = buy_operations.iter().sum();
        assert_eq!(total_buy_amount, BigDecimal::from(1000));

        let adjustment_factor = &available_wrap_near / &total_buy_amount;
        assert_eq!(adjustment_factor, BigDecimal::from_str("0.001").unwrap());

        // Apply adjustment
        let adjusted_operations: Vec<BigDecimal> = buy_operations
            .iter()
            .map(|amount| amount * &adjustment_factor)
            .collect();

        // Proportions should be maintained
        assert_eq!(adjusted_operations[0], BigDecimal::from_str("0.4").unwrap());
        assert_eq!(adjusted_operations[1], BigDecimal::from_str("0.3").unwrap());
        assert_eq!(adjusted_operations[2], BigDecimal::from_str("0.3").unwrap());

        // Total should equal available balance
        let adjusted_total: BigDecimal = adjusted_operations.iter().sum();
        assert_eq!(adjusted_total, available_wrap_near);
    }

    #[test]
    fn test_small_rate_scaling_issue() {
        // Test: Very small rates can become 0 when converted to u128
        // This happens for expensive tokens with few decimals
        use num_bigint::ToBigInt;

        // Case 1: Normal rate (token worth 0.001 NEAR, 18 decimals)
        // rate = 1e18 / 1e26 = 1e-8
        let rate_normal = BigDecimal::from_str("0.00000001").unwrap();
        let scale = BigDecimal::from_str("1000000000000000000000000").unwrap(); // 1e24
        let scaled_normal = &rate_normal * &scale;
        let bigint_normal = scaled_normal.to_bigint().unwrap();
        println!(
            "Normal rate: {} -> scaled: {} -> bigint: {}",
            rate_normal, scaled_normal, bigint_normal
        );
        assert!(
            bigint_normal > num_bigint::BigInt::from(0),
            "Normal rate should not become 0"
        );

        // Case 2: Problematic rate (expensive token with 0 decimals, worth 2 NEAR)
        // rate = 50 / 1e26 = 5e-25
        let rate_problem = BigDecimal::from_str("0.0000000000000000000000005").unwrap();
        let scaled_problem = &rate_problem * &scale;
        let bigint_problem = scaled_problem.to_bigint();
        println!(
            "Problem rate: {} -> scaled: {} -> bigint: {:?}",
            rate_problem, scaled_problem, bigint_problem
        );

        // This test documents the known issue: small rates become 0
        // The bigint should be Some(0) or the scaled value should be < 1
        if let Some(bi) = bigint_problem {
            println!("WARNING: Very small rate results in bigint = {}", bi);
            // If this is 0, we have a precision issue
            if bi == num_bigint::BigInt::from(0) {
                println!(
                    "ISSUE CONFIRMED: Rate {} scaled to {} truncates to 0",
                    rate_problem, scaled_problem
                );
            }
        }

        // Case 3: Edge case - rate exactly at boundary
        // rate × 1e24 = 1 -> rate = 1e-24
        let rate_boundary = BigDecimal::from_str("0.000000000000000000000001").unwrap();
        let scaled_boundary = &rate_boundary * &scale;
        let bigint_boundary = scaled_boundary.to_bigint().unwrap();
        println!(
            "Boundary rate: {} -> scaled: {} -> bigint: {}",
            rate_boundary, scaled_boundary, bigint_boundary
        );
        assert_eq!(
            bigint_boundary,
            num_bigint::BigInt::from(1),
            "Boundary rate should be exactly 1"
        );
    }
}
