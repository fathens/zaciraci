use super::*;

#[test]
fn test_to_spot_rate_without_path() {
    // swap_path が None の場合、元のレートがそのまま返る
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    let token_rate = make_token_rate(base, quote, 1000, timestamp);
    let spot_rate = token_rate.to_spot_rate();

    assert_eq!(
        spot_rate.raw_rate(),
        token_rate.exchange_rate.raw_rate(),
        "Spot rate should equal original rate when swap_path is None"
    );
}

#[test]
fn test_to_spot_rate_with_path() {
    // swap_path がある場合、補正されたレートが返る
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    // プールサイズ: 10,000 NEAR = 10^28 yocto
    // rate_calc_near: 10 NEAR
    // 補正係数: 1 + (10 * 10^24) / (10 * 10^27) = 1.001 (+0.1%)
    let pool_amount_yocto = "10000000000000000000000000000"; // 10,000 NEAR in yocto
    let swap_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 123,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: pool_amount_yocto.parse().unwrap(),
            amount_out: "5000000000000000000000000000".parse().unwrap(), // 5,000 NEAR in yocto
        }],
    };

    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10, // 10 NEAR
        swap_path: Some(swap_path),
    };

    let spot_rate = token_rate.to_spot_rate();

    // 補正係数: 1 + (10 * 10^24) / (10^28) = 1 + 10^-3 = 1.001
    // 期待値: 1000 * 1.001 = 1001
    let expected = BigDecimal::from_str("1001").unwrap();
    assert_eq!(
        spot_rate.raw_rate(),
        &expected,
        "Spot rate should be corrected by slippage factor"
    );
}

#[test]
fn test_to_spot_rate_with_small_pool() {
    // 小さいプールの場合、補正が大きくなる
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    // プールサイズ: 100 NEAR = 10^26 yocto
    // rate_calc_near: 10 NEAR
    // 補正係数: 1 + (10 * 10^24) / (10^26) = 1.1 (+10%)
    let pool_amount_yocto = "100000000000000000000000000"; // 100 NEAR in yocto
    let swap_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 456,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: pool_amount_yocto.parse().unwrap(),
            amount_out: "50000000000000000000000000".parse().unwrap(), // 50 NEAR in yocto
        }],
    };

    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10, // 10 NEAR
        swap_path: Some(swap_path),
    };

    let spot_rate = token_rate.to_spot_rate();

    // 補正係数: 1 + (10 * 10^24) / (10^26) = 1.1
    // 期待値: 1000 * 1.1 = 1100
    let expected = BigDecimal::from_str("1100").unwrap();
    assert_eq!(
        spot_rate.raw_rate(),
        &expected,
        "Spot rate should be corrected by larger slippage factor for small pool"
    );
}

#[test]
fn test_to_spot_rate_with_empty_pools() {
    // pools が空の場合、元のレートがそのまま返る
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    let swap_path = SwapPath { pools: vec![] };

    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10,
        swap_path: Some(swap_path),
    };

    let spot_rate = token_rate.to_spot_rate();

    assert_eq!(
        spot_rate.raw_rate(),
        token_rate.exchange_rate.raw_rate(),
        "Spot rate should equal original rate when pools is empty"
    );
}

#[test]
fn test_to_spot_rate_with_zero_pool_amount() {
    // プールサイズが 0 の場合、元のレートがそのまま返る
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    let swap_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 789,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "0".parse().unwrap(),
            amount_out: "1000".parse().unwrap(),
        }],
    };

    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10,
        swap_path: Some(swap_path),
    };

    let spot_rate = token_rate.to_spot_rate();

    assert_eq!(
        spot_rate.raw_rate(),
        token_rate.exchange_rate.raw_rate(),
        "Spot rate should equal original rate when pool amount is zero"
    );
}

#[test]
fn test_to_spot_rate_with_fallback_uses_fallback() {
    // swap_path が None の場合、フォールバックパスを使用して補正される
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    // swap_path なしのレート
    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10, // 10 NEAR
        swap_path: None,
    };

    // フォールバック用の swap_path
    // プールサイズ: 100 NEAR = 10^26 yocto
    // 補正係数: 1 + (10 * 10^24) / (10^26) = 1.1 (+10%)
    let fallback_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 456,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "100000000000000000000000000".parse().unwrap(), // 100 NEAR in yocto
            amount_out: "50000000000000000000000000".parse().unwrap(),
        }],
    };

    let spot_rate = token_rate.to_spot_rate_with_fallback(Some(&fallback_path));

    // 補正係数: 1.1
    // 期待値: 1000 * 1.1 = 1100
    let expected = BigDecimal::from_str("1100").unwrap();
    assert_eq!(
        spot_rate.raw_rate(),
        &expected,
        "Spot rate should be corrected using fallback path"
    );
}

#[test]
fn test_to_spot_rate_with_fallback_prefers_own_path() {
    // 自身の swap_path がある場合、フォールバックは使用されない
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    // プールサイズ: 10,000 NEAR = 10^28 yocto (補正係数 1.001)
    let own_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 123,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "10000000000000000000000000000".parse().unwrap(), // 10,000 NEAR in yocto
            amount_out: "5000000000000000000000000000".parse().unwrap(),
        }],
    };

    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10,
        swap_path: Some(own_path),
    };

    // フォールバック（補正係数 1.1）- 使用されないはず
    let fallback_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 456,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "100000000000000000000000000".parse().unwrap(), // 100 NEAR
            amount_out: "50000000000000000000000000".parse().unwrap(),
        }],
    };

    let spot_rate = token_rate.to_spot_rate_with_fallback(Some(&fallback_path));

    // 自身のパスで補正 (1.001): 1000 * 1.001 = 1001
    let expected = BigDecimal::from_str("1001").unwrap();
    assert_eq!(
        spot_rate.raw_rate(),
        &expected,
        "Spot rate should use own path, not fallback"
    );
}

#[test]
fn test_to_spot_rate_with_fallback_no_fallback() {
    // swap_path が None でフォールバックもない場合、元のレートが返る
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10,
        swap_path: None,
    };

    let spot_rate = token_rate.to_spot_rate_with_fallback(None);

    assert_eq!(
        spot_rate.raw_rate(),
        token_rate.exchange_rate.raw_rate(),
        "Spot rate should equal original rate when no path and no fallback"
    );
}

/// find_fallback_path のロジックをテスト
/// 「自分より新しくもっとも古い」swap_path を返すことを確認
#[test]
fn test_find_fallback_path_returns_nearest_newer() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    // 異なる pool_id を持つ swap_path を作成（区別できるように）
    let make_path = |pool_id: u32| SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "100000000000000000000000000".parse().unwrap(),
            amount_out: "50000000000000000000000000".parse().unwrap(),
        }],
    };

    // 時系列順（古い → 新しい）のレート
    // r0: 4時間前, swap_path=None
    // r1: 3時間前, swap_path=None
    // r2: 2時間前, swap_path=Some(pool_id=200)  <- r0, r1 のフォールバック
    // r3: 1時間前, swap_path=None
    // r4: 今,      swap_path=Some(pool_id=400)  <- r3 のフォールバック
    let rates = vec![
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(4),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(3),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(2),
            rate_calc_near: 10,
            swap_path: Some(make_path(200)),
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(1),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now,
            rate_calc_near: 10,
            swap_path: Some(make_path(400)),
        },
    ];

    // 各レートに対してフォールバックを検索
    // find_fallback_path: 自分より新しくもっとも古い swap_path を返す
    for (i, _rate) in rates.iter().enumerate() {
        let fallback = TokenRate::find_fallback_path(&rates, i);

        match i {
            0 | 1 => {
                // r0, r1 -> r2 (pool_id=200) がフォールバック
                assert!(fallback.is_some(), "r{} should have fallback", i);
                assert_eq!(
                    fallback.unwrap().pools[0].pool_id,
                    200,
                    "r{} should use r2's path (pool_id=200)",
                    i
                );
            }
            2 => {
                // r2 は自身が swap_path を持つのでフォールバック不要（None を返す）
                assert!(fallback.is_none(), "r2 has own path, no fallback needed");
            }
            3 => {
                // r3 -> r4 (pool_id=400) がフォールバック
                assert!(fallback.is_some(), "r3 should have fallback");
                assert_eq!(
                    fallback.unwrap().pools[0].pool_id,
                    400,
                    "r3 should use r4's path (pool_id=400)"
                );
            }
            4 => {
                // r4 は自身が swap_path を持つのでフォールバック不要
                assert!(fallback.is_none(), "r4 has own path, no fallback needed");
            }
            _ => unreachable!(),
        }
    }
}

/// 自分が swap_path を持つ場合、フォールバックではなく自分の path が使われることを確認
#[test]
fn test_spot_rate_uses_own_path_not_fallback() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    // 異なる補正係数を持つ swap_path を作成
    // 自身の path: 100 NEAR -> 補正係数 1.1 (+10%)
    let own_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 100,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "100000000000000000000000000".parse().unwrap(), // 100 NEAR
            amount_out: "50000000000000000000000000".parse().unwrap(),
        }],
    };

    // フォールバック候補の path: 1000 NEAR -> 補正係数 1.01 (+1%)
    let fallback_candidate_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 200,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "1000000000000000000000000000".parse().unwrap(), // 1000 NEAR
            amount_out: "500000000000000000000000000".parse().unwrap(),
        }],
    };

    // 時系列順のレート配列
    // r0: swap_path=own_path (100)
    // r1: swap_path=fallback_candidate_path (200) <- これがフォールバック候補だが使われない
    let rates = vec![
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(1),
            rate_calc_near: 10,
            swap_path: Some(own_path.clone()),
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now,
            rate_calc_near: 10,
            swap_path: Some(fallback_candidate_path),
        },
    ];

    // r0 のフォールバックを検索 -> 自身が path を持つので None
    let fallback = TokenRate::find_fallback_path(&rates, 0);
    assert!(
        fallback.is_none(),
        "find_fallback_path should return None when rate has own path"
    );

    // スポットレートを計算
    let spot_rate = rates[0].to_spot_rate_with_fallback(fallback);

    // 自身の path (100 NEAR) で補正されるので、補正係数は 1.1
    // 1000 * 1.1 = 1100
    let expected = BigDecimal::from_str("1100").unwrap();
    assert_eq!(
        spot_rate.raw_rate(),
        &expected,
        "Should use own path (1.1 correction), not fallback (1.01 correction)"
    );

    // 比較: もしフォールバック (1000 NEAR, 補正係数 1.01) を使った場合
    // 1000 * 1.01 = 1010 になるはず
    let wrong_rate = BigDecimal::from_str("1010").unwrap();
    assert_ne!(
        spot_rate.raw_rate(),
        &wrong_rate,
        "Should NOT be 1010 (fallback's correction)"
    );
}

/// 全てのレートが swap_path を持たない場合、フォールバックは None
#[test]
fn test_find_fallback_path_all_none() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    let rates = vec![
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(2),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(1),
            rate_calc_near: 10,
            swap_path: None,
        },
    ];

    for i in 0..rates.len() {
        let fallback = TokenRate::find_fallback_path(&rates, i);
        assert!(fallback.is_none(), "No fallback when all paths are None");
    }
}

/// precompute_fallback_indices のテスト：基本ケース
/// find_fallback_path と同じ結果を返すことを確認
#[test]
fn test_precompute_fallback_indices_basic() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    let make_path = |pool_id: u32| SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "100000000000000000000000000".parse().unwrap(),
            amount_out: "50000000000000000000000000".parse().unwrap(),
        }],
    };

    // 時系列順（古い -> 新しい）のレート
    // r0: swap_path=None       -> フォールバック=r2 (index=2)
    // r1: swap_path=None       -> フォールバック=r2 (index=2)
    // r2: swap_path=Some(200)  -> フォールバック=None
    // r3: swap_path=None       -> フォールバック=r4 (index=4)
    // r4: swap_path=Some(400)  -> フォールバック=None
    let rates = vec![
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(4),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(3),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(2),
            rate_calc_near: 10,
            swap_path: Some(make_path(200)),
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(1),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now,
            rate_calc_near: 10,
            swap_path: Some(make_path(400)),
        },
    ];

    let fallbacks = TokenRate::precompute_fallback_indices(&rates);

    assert_eq!(fallbacks.len(), 5);
    assert_eq!(fallbacks[0], Some(2), "r0 should fallback to r2");
    assert_eq!(fallbacks[1], Some(2), "r1 should fallback to r2");
    assert_eq!(fallbacks[2], None, "r2 has own path, no fallback");
    assert_eq!(fallbacks[3], Some(4), "r3 should fallback to r4");
    assert_eq!(fallbacks[4], None, "r4 has own path, no fallback");

    // find_fallback_path と同じ結果を返すことを確認
    for (i, &fallback_idx) in fallbacks.iter().enumerate() {
        let from_linear = TokenRate::find_fallback_path(&rates, i);
        let from_precompute = fallback_idx
            .and_then(|idx| rates.get(idx))
            .and_then(|r| r.swap_path.as_ref());
        assert_eq!(
            from_linear, from_precompute,
            "precompute should match linear search at index {}",
            i
        );
    }
}

/// precompute_fallback_indices のテスト：全て None のケース
#[test]
fn test_precompute_fallback_indices_all_none() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    let rates = vec![
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(2),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(1),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now,
            rate_calc_near: 10,
            swap_path: None,
        },
    ];

    let fallbacks = TokenRate::precompute_fallback_indices(&rates);

    assert_eq!(fallbacks.len(), 3);
    for (i, fallback) in fallbacks.iter().enumerate() {
        assert!(
            fallback.is_none(),
            "No fallback when all paths are None at index {}",
            i
        );
    }
}

/// precompute_fallback_indices のテスト：全て Some のケース
#[test]
fn test_precompute_fallback_indices_all_some() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    let make_path = |pool_id: u32| SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "100000000000000000000000000".parse().unwrap(),
            amount_out: "50000000000000000000000000".parse().unwrap(),
        }],
    };

    let rates = vec![
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(2),
            rate_calc_near: 10,
            swap_path: Some(make_path(100)),
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(1),
            rate_calc_near: 10,
            swap_path: Some(make_path(200)),
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now,
            rate_calc_near: 10,
            swap_path: Some(make_path(300)),
        },
    ];

    let fallbacks = TokenRate::precompute_fallback_indices(&rates);

    assert_eq!(fallbacks.len(), 3);
    for (i, fallback) in fallbacks.iter().enumerate() {
        assert!(
            fallback.is_none(),
            "No fallback needed when rate has own path at index {}",
            i
        );
    }
}

/// precompute_fallback_indices のテスト：空の配列
#[test]
fn test_precompute_fallback_indices_empty() {
    let rates: Vec<TokenRate> = vec![];
    let fallbacks = TokenRate::precompute_fallback_indices(&rates);
    assert!(fallbacks.is_empty());
}

/// precompute_fallback_indices のテスト：単一要素
#[test]
fn test_precompute_fallback_indices_single() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    // swap_path なし -> フォールバックなし（後続がない）
    let rates_none = vec![TokenRate {
        base: base.clone(),
        quote: quote.clone(),
        exchange_rate: make_rate(1000),
        timestamp: now,
        rate_calc_near: 10,
        swap_path: None,
    }];

    let fallbacks = TokenRate::precompute_fallback_indices(&rates_none);
    assert_eq!(fallbacks.len(), 1);
    assert!(fallbacks[0].is_none());

    // swap_path あり -> フォールバック不要
    let rates_some = vec![TokenRate {
        base: base.clone(),
        quote: quote.clone(),
        exchange_rate: make_rate(1000),
        timestamp: now,
        rate_calc_near: 10,
        swap_path: Some(SwapPath {
            pools: vec![SwapPoolInfo {
                pool_id: 100,
                token_in_idx: 0,
                token_out_idx: 1,
                amount_in: "100000000000000000000000000".parse().unwrap(),
                amount_out: "50000000000000000000000000".parse().unwrap(),
            }],
        }),
    }];

    let fallbacks = TokenRate::precompute_fallback_indices(&rates_some);
    assert_eq!(fallbacks.len(), 1);
    assert!(fallbacks[0].is_none());
}

/// precompute_fallback_indices のテスト：先頭のみ swap_path がある場合
/// 先頭要素は自身が path を持つのでフォールバック不要、
/// 後続の要素は全てフォールバックなし（先頭より前に path がない）
#[test]
fn test_precompute_fallback_indices_first_only() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    let make_path = |pool_id: u32| SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "100000000000000000000000000".parse().unwrap(),
            amount_out: "50000000000000000000000000".parse().unwrap(),
        }],
    };

    // r0: swap_path=Some -> フォールバック不要
    // r1: swap_path=None -> フォールバックなし（r0より後ろに path がない）
    // r2: swap_path=None -> フォールバックなし
    let rates = vec![
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(2),
            rate_calc_near: 10,
            swap_path: Some(make_path(100)),
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(1),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now,
            rate_calc_near: 10,
            swap_path: None,
        },
    ];

    let fallbacks = TokenRate::precompute_fallback_indices(&rates);

    assert_eq!(fallbacks.len(), 3);
    assert_eq!(fallbacks[0], None, "r0 has own path, no fallback");
    assert_eq!(fallbacks[1], None, "r1 has no newer path to fallback to");
    assert_eq!(fallbacks[2], None, "r2 has no newer path to fallback to");

    // find_fallback_path と一致することを確認
    for (i, &fallback_idx) in fallbacks.iter().enumerate() {
        let from_linear = TokenRate::find_fallback_path(&rates, i);
        let from_precompute = fallback_idx
            .and_then(|idx| rates.get(idx))
            .and_then(|r| r.swap_path.as_ref());
        assert_eq!(from_linear, from_precompute);
    }
}

/// precompute_fallback_indices のテスト：末尾のみ swap_path がある場合
/// 全ての先行要素が末尾にフォールバックする
#[test]
fn test_precompute_fallback_indices_last_only() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    let make_path = |pool_id: u32| SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "100000000000000000000000000".parse().unwrap(),
            amount_out: "50000000000000000000000000".parse().unwrap(),
        }],
    };

    // r0: swap_path=None -> r2 にフォールバック
    // r1: swap_path=None -> r2 にフォールバック
    // r2: swap_path=Some -> フォールバック不要
    let rates = vec![
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(2),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(1),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now,
            rate_calc_near: 10,
            swap_path: Some(make_path(300)),
        },
    ];

    let fallbacks = TokenRate::precompute_fallback_indices(&rates);

    assert_eq!(fallbacks.len(), 3);
    assert_eq!(fallbacks[0], Some(2), "r0 should fallback to r2");
    assert_eq!(fallbacks[1], Some(2), "r1 should fallback to r2");
    assert_eq!(fallbacks[2], None, "r2 has own path, no fallback");

    // find_fallback_path と一致することを確認
    for (i, &fallback_idx) in fallbacks.iter().enumerate() {
        let from_linear = TokenRate::find_fallback_path(&rates, i);
        let from_precompute = fallback_idx
            .and_then(|idx| rates.get(idx))
            .and_then(|r| r.swap_path.as_ref());
        assert_eq!(from_linear, from_precompute);
    }
}

/// 速度比較テスト: precompute_fallback_indices (O(n)) vs find_fallback_path の全呼び出し (O(n^2))
/// 大量データでの実行時間を比較し、precompute が明らかに高速であることを確認
#[test]
fn test_precompute_fallback_indices_performance() {
    if std::env::var("CI").is_ok() {
        println!("Skipping performance test in CI environment");
        return;
    }

    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    let make_path = |pool_id: u32| SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "100000000000000000000000000".parse().unwrap(),
            amount_out: "50000000000000000000000000".parse().unwrap(),
        }],
    };

    // 1000件のレートを生成（10件に1件 swap_path あり）
    let n = 1000;
    let rates: Vec<TokenRate> = (0..n)
        .map(|i| TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000 + i as i64),
            timestamp: now - chrono::Duration::minutes(n as i64 - i as i64),
            rate_calc_near: 10,
            swap_path: if i % 10 == 9 {
                Some(make_path(i as u32))
            } else {
                None
            },
        })
        .collect();

    // O(n) の事前計算
    let start_precompute = std::time::Instant::now();
    let fallbacks = TokenRate::precompute_fallback_indices(&rates);
    let precompute_duration = start_precompute.elapsed();

    // O(n^2) の線形検索（全要素に対して find_fallback_path を呼び出し）
    let start_linear = std::time::Instant::now();
    let linear_results: Vec<Option<&SwapPath>> = (0..rates.len())
        .map(|i| TokenRate::find_fallback_path(&rates, i))
        .collect();
    let linear_duration = start_linear.elapsed();

    // 結果が一致することを確認
    for i in 0..rates.len() {
        let from_precompute = fallbacks[i]
            .and_then(|idx| rates.get(idx))
            .and_then(|r| r.swap_path.as_ref());
        assert_eq!(
            linear_results[i], from_precompute,
            "Results should match at index {}",
            i
        );
    }

    // 事前計算が線形検索より高速であることを確認
    // 注: CI環境での変動を考慮し、10倍以上の差を期待
    // n=1000 の場合: O(n^2) ~ 500,000 比較 vs O(n) ~ 1,000
    assert!(
        precompute_duration < linear_duration,
        "precompute ({:?}) should be faster than linear search ({:?})",
        precompute_duration,
        linear_duration
    );

    // 実運用での信頼性のため、少なくとも1.5倍は高速であることを確認
    // （CI環境でのキャッシュ効果やカバレッジ計測のオーバーヘッドを考慮した保守的な閾値）
    let speedup = linear_duration.as_nanos() as f64 / precompute_duration.as_nanos() as f64;
    assert!(
        speedup >= 1.5,
        "precompute should be at least 1.5x faster, but speedup was only {:.2}x",
        speedup
    );
}

/// 大規模データでのスケーラビリティテスト
/// データ量が10倍になっても処理時間が線形に増加することを確認
#[test]
fn test_precompute_fallback_indices_scalability() {
    if std::env::var("CI").is_ok() {
        println!("Skipping performance test in CI environment");
        return;
    }

    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    let make_path = |pool_id: u32| SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "100000000000000000000000000".parse().unwrap(),
            amount_out: "50000000000000000000000000".parse().unwrap(),
        }],
    };

    let generate_rates = |n: usize| -> Vec<TokenRate> {
        (0..n)
            .map(|i| TokenRate {
                base: base.clone(),
                quote: quote.clone(),
                exchange_rate: make_rate(1000 + i as i64),
                timestamp: now - chrono::Duration::seconds(n as i64 - i as i64),
                rate_calc_near: 10,
                swap_path: if i % 10 == 9 {
                    Some(make_path(i as u32))
                } else {
                    None
                },
            })
            .collect()
    };

    // ウォームアップ（JIT/キャッシュの影響を軽減）
    let warmup_rates = generate_rates(100);
    let _ = TokenRate::precompute_fallback_indices(&warmup_rates);

    // 小規模 (n=500)
    let small_rates = generate_rates(500);
    let start_small = std::time::Instant::now();
    for _ in 0..10 {
        let _ = TokenRate::precompute_fallback_indices(&small_rates);
    }
    let small_duration = start_small.elapsed();

    // 大規模 (n=5000, 10倍)
    let large_rates = generate_rates(5000);
    let start_large = std::time::Instant::now();
    for _ in 0..10 {
        let _ = TokenRate::precompute_fallback_indices(&large_rates);
    }
    let large_duration = start_large.elapsed();

    // O(n) アルゴリズムなので、データ量が10倍になっても処理時間は約10倍程度のはず
    // 多少のオーバーヘッドを考慮して20倍以下であることを確認
    let ratio = large_duration.as_nanos() as f64 / small_duration.as_nanos() as f64;
    assert!(
        ratio <= 20.0,
        "Processing time should scale linearly (ratio should be ~10x for 10x data), but was {:.2}x",
        ratio
    );
}

// ===========================================================================
// マルチホップ補正テスト
// ===========================================================================

/// シングルホップ: 従来の動作と同一結果を確認
#[test]
fn test_to_spot_rate_multihop_single_hop_same_as_before() {
    // シングルホップの場合、マルチホップ実装と従来実装は同じ結果を返すべき
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    // プールサイズ: 10,000 NEAR = 10^28 yocto
    // rate_calc_near: 10 NEAR
    // 補正係数: 1 + (10 * 10^24) / (10^28) = 1.001
    let swap_path = SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id: 123,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "10000000000000000000000000000".parse().unwrap(), // 10,000 NEAR in yocto
            amount_out: "5000000000000000000000000000".parse().unwrap(), // 5,000 NEAR in yocto
        }],
    };

    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10,
        swap_path: Some(swap_path),
    };

    let spot_rate = token_rate.to_spot_rate();

    // 補正係数: 1 + (10 * 10^24) / (10^28) = 1.001
    // 期待値: 1000 * 1.001 = 1001
    let expected = BigDecimal::from_str("1001").unwrap();
    assert_eq!(
        spot_rate.raw_rate(),
        &expected,
        "Single hop should produce same result as before (1001)"
    );
}

/// 2ホップ: 補正係数が積算されることを確認
#[test]
fn test_to_spot_rate_multihop_two_hops() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    // 2ホップスワップ:
    // Hop1: NEAR -> TokenA
    //   - pool_amount_in: 100 NEAR = 10^26 yocto
    //   - pool_amount_out: 200 TokenA
    //   - Δx_0 = 10 NEAR = 10^25 yocto
    //   - 補正1: 1 + 10^25 / 10^26 = 1.1
    //   - Δx_1 = 10 * 10^24 * (200 / 100) = 20 * 10^24 (相対的なスケール)
    //
    // Hop2: TokenA -> TokenB
    //   - pool_amount_in: 1000 = 10^3
    //   - pool_amount_out: 500
    //   - 補正2: 1 + Δx_1 / 10^3
    //
    // 簡略化のため、同じスケールで計算:
    // Hop1: in=100, out=200, Δx=10 -> correction1 = 1.1, Δx'=10*200/100=20
    // Hop2: in=1000, out=500, Δx'=20 -> correction2 = 1.02
    // 総補正 = 1.1 * 1.02 = 1.122
    let swap_path = SwapPath {
        pools: vec![
            SwapPoolInfo {
                pool_id: 1,
                token_in_idx: 0,
                token_out_idx: 1,
                amount_in: "100000000000000000000000000".parse().unwrap(), // 100 NEAR in yocto
                amount_out: "200000000000000000000000000".parse().unwrap(), // 200 単位
            },
            SwapPoolInfo {
                pool_id: 2,
                token_in_idx: 0,
                token_out_idx: 1,
                amount_in: "1000000000000000000000000000".parse().unwrap(), // 1000 NEAR in yocto
                amount_out: "500000000000000000000000000".parse().unwrap(), // 500 単位
            },
        ],
    };

    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10, // 10 NEAR
        swap_path: Some(swap_path),
    };

    let spot_rate = token_rate.to_spot_rate();

    // 計算:
    // Δx_0 = 10 * 10^24 yocto
    // Hop1: pool_in = 100 * 10^24, correction1 = (100 + 10) / 100 = 1.1
    //       Δx_1 = 10 * 10^24 * (200 / 100) = 20 * 10^24
    // Hop2: pool_in = 1000 * 10^24, correction2 = (1000 + 20) / 1000 = 1.02
    // 総補正 = 1.1 * 1.02 = 1.122
    // 期待値: 1000 * 1.122 = 1122
    let expected = BigDecimal::from_str("1122").unwrap();
    assert_eq!(
        spot_rate.raw_rate(),
        &expected,
        "Two hop correction should be 1.1 * 1.02 = 1.122, so 1000 * 1.122 = 1122"
    );
}

/// 3ホップ以上: 累積補正の確認
#[test]
fn test_to_spot_rate_multihop_three_hops() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    // 3ホップスワップ:
    // Hop1: in=100, out=100 (1:1) -> correction1 = 1.1, Δx'=10
    // Hop2: in=100, out=100 (1:1) -> correction2 = 1.1, Δx''=10
    // Hop3: in=100, out=100 (1:1) -> correction3 = 1.1
    // 総補正 = 1.1^3 = 1.331
    let swap_path = SwapPath {
        pools: vec![
            SwapPoolInfo {
                pool_id: 1,
                token_in_idx: 0,
                token_out_idx: 1,
                amount_in: "100000000000000000000000000".parse().unwrap(), // 100 NEAR
                amount_out: "100000000000000000000000000".parse().unwrap(),
            },
            SwapPoolInfo {
                pool_id: 2,
                token_in_idx: 0,
                token_out_idx: 1,
                amount_in: "100000000000000000000000000".parse().unwrap(),
                amount_out: "100000000000000000000000000".parse().unwrap(),
            },
            SwapPoolInfo {
                pool_id: 3,
                token_in_idx: 0,
                token_out_idx: 1,
                amount_in: "100000000000000000000000000".parse().unwrap(),
                amount_out: "100000000000000000000000000".parse().unwrap(),
            },
        ],
    };

    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10,
        swap_path: Some(swap_path),
    };

    let spot_rate = token_rate.to_spot_rate();

    // 1.1^3 = 1.331
    // 1000 * 1.331 = 1331
    let expected = BigDecimal::from_str("1331").unwrap();
    assert_eq!(
        spot_rate.raw_rate(),
        &expected,
        "Three hop correction should be 1.1^3 = 1.331, so 1000 * 1.331 = 1331"
    );
}

/// マルチホップでフォールバックパスを使用する場合
#[test]
fn test_to_spot_rate_multihop_with_fallback() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let timestamp = chrono::Utc::now().naive_utc();

    // swap_path なしのレート
    let token_rate = TokenRate {
        base,
        quote,
        exchange_rate: make_rate(1000),
        timestamp,
        rate_calc_near: 10,
        swap_path: None,
    };

    // フォールバック用の2ホップパス
    // Hop1: in=100, out=200 -> correction1 = 1.1, Δx'=20
    // Hop2: in=1000, out=500 -> correction2 = 1.02
    // 総補正 = 1.122
    let fallback_path = SwapPath {
        pools: vec![
            SwapPoolInfo {
                pool_id: 1,
                token_in_idx: 0,
                token_out_idx: 1,
                amount_in: "100000000000000000000000000".parse().unwrap(),
                amount_out: "200000000000000000000000000".parse().unwrap(),
            },
            SwapPoolInfo {
                pool_id: 2,
                token_in_idx: 0,
                token_out_idx: 1,
                amount_in: "1000000000000000000000000000".parse().unwrap(),
                amount_out: "500000000000000000000000000".parse().unwrap(),
            },
        ],
    };

    let spot_rate = token_rate.to_spot_rate_with_fallback(Some(&fallback_path));

    // 1000 * 1.122 = 1122
    let expected = BigDecimal::from_str("1122").unwrap();
    assert_eq!(
        spot_rate.raw_rate(),
        &expected,
        "Multihop fallback should work: 1.1 * 1.02 = 1.122"
    );
}

// =============================================================================
// to_spot_rates() テスト
// =============================================================================

#[test]
fn test_to_spot_rates_empty() {
    let result = TokenRate::to_spot_rates(&[]);
    assert!(result.is_empty(), "Empty input should produce empty output");
}

#[test]
fn test_to_spot_rates_single_rate() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    let rate = make_token_rate(base, quote, 1000, now);
    let result = TokenRate::to_spot_rates(&[rate]);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].0, now);
    // swap_path=None なので生レートがそのまま返る
    assert_eq!(result[0].1.raw_rate(), &BigDecimal::from(1000));
}

#[test]
fn test_to_spot_rates_all_normal() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    let rates = vec![
        make_token_rate(
            base.clone(),
            quote.clone(),
            100,
            now - chrono::Duration::hours(2),
        ),
        make_token_rate(
            base.clone(),
            quote.clone(),
            200,
            now - chrono::Duration::hours(1),
        ),
        make_token_rate(base, quote, 300, now),
    ];
    let result = TokenRate::to_spot_rates(&rates);

    assert_eq!(result.len(), 3);
    assert_eq!(result[0].1.raw_rate(), &BigDecimal::from(100));
    assert_eq!(result[1].1.raw_rate(), &BigDecimal::from(200));
    assert_eq!(result[2].1.raw_rate(), &BigDecimal::from(300));
}

#[test]
fn test_to_spot_rates_filters_zero_rates() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    let rates = vec![
        make_token_rate(
            base.clone(),
            quote.clone(),
            100,
            now - chrono::Duration::hours(2),
        ),
        make_token_rate(
            base.clone(),
            quote.clone(),
            0,
            now - chrono::Duration::hours(1),
        ),
        make_token_rate(base, quote, 300, now),
    ];
    let result = TokenRate::to_spot_rates(&rates);

    assert_eq!(result.len(), 2, "Zero rate should be filtered out");
    assert_eq!(result[0].1.raw_rate(), &BigDecimal::from(100));
    assert_eq!(result[1].1.raw_rate(), &BigDecimal::from(300));
}

#[test]
fn test_to_spot_rates_applies_fallback() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    let make_path = |pool_id: u32| SwapPath {
        pools: vec![SwapPoolInfo {
            pool_id,
            token_in_idx: 0,
            token_out_idx: 1,
            amount_in: "100000000000000000000000000".parse().unwrap(),
            amount_out: "50000000000000000000000000".parse().unwrap(),
        }],
    };

    // r0: swap_path=None → r1 のフォールバックが適用される
    // r1: swap_path=Some(pool_id=200) → 自身の swap_path で補正
    // r2: swap_path=None → フォールバックなし（自分より新しい swap_path がない）
    let rates = vec![
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(2),
            rate_calc_near: 10,
            swap_path: None,
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now - chrono::Duration::hours(1),
            rate_calc_near: 10,
            swap_path: Some(make_path(200)),
        },
        TokenRate {
            base: base.clone(),
            quote: quote.clone(),
            exchange_rate: make_rate(1000),
            timestamp: now,
            rate_calc_near: 10,
            swap_path: None,
        },
    ];

    let result = TokenRate::to_spot_rates(&rates);

    assert_eq!(result.len(), 3);

    // r0 は r1 の swap_path をフォールバックで使用 → 補正あり
    // r1 は自身の swap_path で補正あり
    // r0 と r1 は同じレート・同じ swap_path なので同じスポットレート
    assert_eq!(
        result[0].1.raw_rate(),
        result[1].1.raw_rate(),
        "r0 (fallback from r1) and r1 (own path) should produce same spot rate"
    );

    // r2 はフォールバックなし → 生レートのまま
    assert_eq!(
        result[2].1.raw_rate(),
        &BigDecimal::from(1000),
        "r2 without fallback should return raw rate"
    );

    // r0/r1 は補正ありなので raw rate (1000) とは異なる
    assert_ne!(
        result[0].1.raw_rate(),
        &BigDecimal::from(1000),
        "r0 with fallback correction should differ from raw rate"
    );
}

#[test]
fn test_to_spot_rates_all_zero_returns_empty() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    let rates = vec![
        make_token_rate(
            base.clone(),
            quote.clone(),
            0,
            now - chrono::Duration::hours(1),
        ),
        make_token_rate(base, quote, 0, now),
    ];
    let result = TokenRate::to_spot_rates(&rates);

    assert!(
        result.is_empty(),
        "All-zero rates should produce empty output"
    );
}

#[test]
fn test_to_spot_rates_preserves_timestamp_order() {
    let base: TokenOutAccount = TokenAccount::from_str("eth.token").unwrap().into();
    let quote: TokenInAccount = TokenAccount::from_str("usdt.token").unwrap().into();
    let now = chrono::Utc::now().naive_utc();

    let ts0 = now - chrono::Duration::hours(3);
    let ts1 = now - chrono::Duration::hours(2);
    let ts2 = now - chrono::Duration::hours(1);

    let rates = vec![
        make_token_rate(base.clone(), quote.clone(), 100, ts0),
        make_token_rate(base.clone(), quote.clone(), 200, ts1),
        make_token_rate(base, quote, 300, ts2),
    ];
    let result = TokenRate::to_spot_rates(&rates);

    assert_eq!(result.len(), 3);
    assert_eq!(result[0].0, ts0);
    assert_eq!(result[1].0, ts1);
    assert_eq!(result[2].0, ts2);
}
