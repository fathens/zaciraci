use super::*;
use std::str::FromStr;

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
    let rates = SameBaseTokenRates { points: Vec::new() };

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
            price: price_from_int(100),
        },
        Point {
            timestamp: base_time + Duration::seconds(20),
            price: price_from_int(110),
        },
        Point {
            timestamp: base_time + Duration::seconds(40),
            price: price_from_int(90),
        },
    ];

    let rates = SameBaseTokenRates { points };

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
            price: price_from_int(100),
        },
        Point {
            timestamp: base_time + Duration::seconds(30),
            price: price_from_int(110),
        },
        // 2番目の期間 (10:01:00 - 10:02:00)
        Point {
            timestamp: base_time + Duration::minutes(1),
            price: price_from_int(120),
        },
        Point {
            timestamp: base_time + Duration::minutes(1) + Duration::seconds(30),
            price: price_from_int(130),
        },
        // 3番目の期間 (10:02:00 - 10:03:00)
        Point {
            timestamp: base_time + Duration::minutes(2),
            price: price_from_int(140),
        },
        Point {
            timestamp: base_time + Duration::minutes(2) + Duration::seconds(30),
            price: price_from_int(150),
        },
    ];

    let rates = SameBaseTokenRates { points };

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
            price: price_from_int(100),
        },
        // 境界値ちょうど (10:05:00) - 次の期間に含まれる
        Point {
            timestamp: base_time + Duration::minutes(5),
            price: price_from_int(200),
        },
        // 2番目の期間 (10:05:00 - 10:10:00)
        Point {
            timestamp: base_time + Duration::minutes(7),
            price: price_from_int(300),
        },
    ];

    let rates = SameBaseTokenRates { points };

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
fn test_format_decimal_digits() {
    // 整数値のテスト
    assert_eq!(
        "100",
        ListStatsInPeriod::<BigDecimal>::format_decimal(&BigDecimal::from(100))
    );

    // 小数点以下が全て0の値
    let with_zeros = BigDecimal::from(100) + BigDecimal::from_str("0.000000000").unwrap();
    assert_eq!(
        "100",
        ListStatsInPeriod::<BigDecimal>::format_decimal(&with_zeros)
    );

    // 小数点以下が1桁の値
    assert_eq!(
        "0.1",
        ListStatsInPeriod::<BigDecimal>::format_decimal(&BigDecimal::from_str("0.1").unwrap())
    );

    // 小数点以下が2桁の値
    assert_eq!(
        "0.12",
        ListStatsInPeriod::<BigDecimal>::format_decimal(&BigDecimal::from_str("0.12").unwrap())
    );

    // 小数点以下が3桁の値
    assert_eq!(
        "0.123",
        ListStatsInPeriod::<BigDecimal>::format_decimal(&BigDecimal::from_str("0.123").unwrap())
    );

    // 小数点以下が4桁の値
    assert_eq!(
        "0.1234",
        ListStatsInPeriod::<BigDecimal>::format_decimal(&BigDecimal::from_str("0.1234").unwrap())
    );

    // 小数点以下が5桁の値
    assert_eq!(
        "0.12345",
        ListStatsInPeriod::<BigDecimal>::format_decimal(&BigDecimal::from_str("0.12345").unwrap())
    );

    // 小数点以下が6桁の値
    assert_eq!(
        "0.123456",
        ListStatsInPeriod::<BigDecimal>::format_decimal(&BigDecimal::from_str("0.123456").unwrap())
    );

    // 小数点以下が7桁の値
    assert_eq!(
        "0.1234567",
        ListStatsInPeriod::<BigDecimal>::format_decimal(
            &BigDecimal::from_str("0.1234567").unwrap()
        )
    );

    // 小数点以下が8桁の値
    assert_eq!(
        "0.12345678",
        ListStatsInPeriod::<BigDecimal>::format_decimal(
            &BigDecimal::from_str("0.12345678").unwrap()
        )
    );

    // 小数点以下が9桁の値
    assert_eq!(
        "0.123456789",
        ListStatsInPeriod::<BigDecimal>::format_decimal(
            &BigDecimal::from_str("0.123456789").unwrap()
        )
    );

    // 小数点以下が10桁の値（9桁までに制限される）
    assert_eq!(
        "0.123456789",
        ListStatsInPeriod::<BigDecimal>::format_decimal(
            &BigDecimal::from_str("0.1234567891").unwrap()
        )
    );

    // 末尾に0がある場合（末尾の0は削除される）
    assert_eq!(
        "0.12345",
        ListStatsInPeriod::<BigDecimal>::format_decimal(
            &BigDecimal::from_str("0.12345000").unwrap()
        )
    );

    // 整数部分あり、小数点以下4桁の値
    assert_eq!(
        "123.4567",
        ListStatsInPeriod::<BigDecimal>::format_decimal(&BigDecimal::from_str("123.4567").unwrap())
    );
}
