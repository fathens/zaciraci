use super::*;

// ==================== calculate_returns_from_prices 直接テスト ====================

#[test]
fn test_calculate_returns_from_prices_basic() {
    // 既知の価格系列から正しいリターンが計算されること
    let prices = vec![
        PricePoint {
            timestamp: Utc::now() - TimeDelta::days(2),
            price: price(100.0),
            volume: None,
        },
        PricePoint {
            timestamp: Utc::now() - TimeDelta::days(1),
            price: price(110.0),
            volume: None,
        },
        PricePoint {
            timestamp: Utc::now(),
            price: price(99.0),
            volume: None,
        },
    ];

    let returns = calculate_returns_from_prices(&prices);
    assert_eq!(returns.len(), 2);
    assert!((returns[0] - 0.1).abs() < 1e-10, "110/100 - 1 = 0.1");
    assert!((returns[1] - (-0.1)).abs() < 1e-10, "99/110 - 1 = -0.1");
}

#[test]
fn test_calculate_returns_from_prices_unsorted_input() {
    // タイムスタンプが昇順でない入力でも正しくソートされてリターンが計算されること
    let now = Utc::now();
    let prices = vec![
        PricePoint {
            timestamp: now, // 最新（3番目に来るべき）
            price: price(120.0),
            volume: None,
        },
        PricePoint {
            timestamp: now - TimeDelta::days(2), // 最古（1番目に来るべき）
            price: price(100.0),
            volume: None,
        },
        PricePoint {
            timestamp: now - TimeDelta::days(1), // 中間（2番目に来るべき）
            price: price(110.0),
            volume: None,
        },
    ];

    let returns = calculate_returns_from_prices(&prices);
    // ソート後: 100 → 110 → 120
    assert_eq!(returns.len(), 2);
    assert!(
        (returns[0] - 0.1).abs() < 1e-10,
        "110/100 - 1 = 0.1, got {}",
        returns[0]
    );
    assert!(
        (returns[1] - (10.0 / 110.0)).abs() < 1e-10,
        "120/110 - 1 ≈ 0.0909, got {}",
        returns[1]
    );
}

#[test]
fn test_calculate_returns_from_prices_empty_and_single() {
    // 空入力 → 空のVec
    let empty: Vec<PricePoint> = vec![];
    assert!(calculate_returns_from_prices(&empty).is_empty());

    // 1要素 → 空のVec（リターン計算不可）
    let single = vec![PricePoint {
        timestamp: Utc::now(),
        price: price(100.0),
        volume: None,
    }];
    assert!(calculate_returns_from_prices(&single).is_empty());
}

#[test]
fn test_calculate_daily_returns_duplicate_tokens() {
    // 同一トークンが複数回含まれる場合、最初の出現のみが使われること
    let now = Utc::now();
    let prices = vec![
        PriceHistory {
            token: token_out("token-a"),
            quote_token: token_in("wrap.near"),
            prices: vec![
                PricePoint {
                    timestamp: now - TimeDelta::days(1),
                    price: price(100.0),
                    volume: None,
                },
                PricePoint {
                    timestamp: now,
                    price: price(110.0),
                    volume: None,
                },
            ],
        },
        // 同一トークンの重複エントリ（異なる価格）
        PriceHistory {
            token: token_out("token-a"),
            quote_token: token_in("wrap.near"),
            prices: vec![
                PricePoint {
                    timestamp: now - TimeDelta::days(1),
                    price: price(200.0),
                    volume: None,
                },
                PricePoint {
                    timestamp: now,
                    price: price(300.0),
                    volume: None,
                },
            ],
        },
        PriceHistory {
            token: token_out("token-b"),
            quote_token: token_in("wrap.near"),
            prices: vec![
                PricePoint {
                    timestamp: now - TimeDelta::days(1),
                    price: price(50.0),
                    volume: None,
                },
                PricePoint {
                    timestamp: now,
                    price: price(55.0),
                    volume: None,
                },
            ],
        },
    ];

    let returns = calculate_daily_returns(&prices);

    // token-a は1回だけ、token-b は1回 → 2トークン
    assert_eq!(returns.len(), 2, "重複トークンは除去されるべき");

    // 最初の token-a エントリのリターン: (110-100)/100 = 0.1
    assert_eq!(returns[0].len(), 1);
    assert!(
        (returns[0][0] - 0.1).abs() < 1e-10,
        "最初の出現の価格が使われるべき, got {}",
        returns[0][0]
    );

    // token-b のリターン: (55-50)/50 = 0.1
    assert_eq!(returns[1].len(), 1);
    assert!(
        (returns[1][0] - 0.1).abs() < 1e-10,
        "token-b return should be 0.1, got {}",
        returns[1][0]
    );
}
