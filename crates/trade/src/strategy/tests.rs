use super::*;
use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use common::types::{ExchangeRate, NearValue, TokenAccount, TokenInAccount};
use near_sdk::AccountId;
use near_sdk::json_types::U128;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

fn make_token(name: &str) -> TokenAccount {
    TokenAccount::from_str(name).unwrap()
}

fn make_pool(id: u32, tokens: Vec<&str>, amounts: Vec<u128>) -> Arc<dex::PoolInfo> {
    Arc::new(dex::PoolInfo::new(
        id,
        dex::pool_info::PoolInfoBared {
            pool_kind: "SIMPLE_POOL".to_string(),
            token_account_ids: tokens.into_iter().map(make_token).collect(),
            amounts: amounts.into_iter().map(U128).collect(),
            total_fee: 30,
            shares_total_supply: U128(0),
            amp: 0,
        },
        NaiveDate::from_ymd_opt(2024, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap(),
    ))
}

fn wnear() -> TokenAccount {
    make_token("wrap.near")
}

// =============================================================================
// estimate_pool_liquidity_in_near テスト
// =============================================================================

#[test]
fn test_balanced_wnear_pool() {
    // 500 NEAR + 500 NEAR 相当のトークン → min = 500 NEAR
    let wnear = wnear();
    // 500 NEAR = 500 * 10^24 yoctoNEAR
    let near_500_yocto: u128 = 500 * 10u128.pow(24);
    // USDT: 500 NEAR 相当 = 500 * rate
    // rate = 5_000_000 (decimals=6) → 1 NEAR = 5_000_000 smallest USDT
    // 500 NEAR → 500 * 5_000_000 = 2_500_000_000 smallest USDT
    let usdt_amount: u128 = 2_500_000_000;

    let pool = make_pool(
        1,
        vec!["wrap.near", "usdt.tether-token.near"],
        vec![near_500_yocto, usdt_amount],
    );

    let mut rates = HashMap::new();
    rates.insert(
        make_token("usdt.tether-token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6),
    );

    let result = estimate_pool_liquidity_in_near(&pool, &wnear, &rates);
    assert!(result.is_some());
    let liquidity = result.unwrap();
    assert_eq!(liquidity, NearValue::from_near(BigDecimal::from(500)));
}

#[test]
fn test_non_wnear_balanced_pool() {
    // USDT-USDC プール（wnear なし）、両側 100 NEAR 相当
    // USDT: rate=5_000_000 (dec=6), 100 NEAR = 500_000_000 smallest
    // USDC: rate=5_100_000 (dec=6), 100 NEAR = 510_000_000 smallest
    let wnear = wnear();
    let pool = make_pool(
        2,
        vec!["usdt.tether-token.near", "usdc.token.near"],
        vec![500_000_000, 510_000_000],
    );

    let mut rates = HashMap::new();
    rates.insert(
        make_token("usdt.tether-token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6),
    );
    rates.insert(
        make_token("usdc.token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_100_000), 6),
    );

    let result = estimate_pool_liquidity_in_near(&pool, &wnear, &rates);
    assert!(result.is_some());
    let liquidity = result.unwrap();
    // USDT side: 500_000_000 / 5_000_000 = 100 NEAR
    // USDC side: 510_000_000 / 5_100_000 = 100 NEAR
    assert_eq!(liquidity, NearValue::from_near(BigDecimal::from(100)));
}

#[test]
fn test_unbalanced_pool() {
    // アンバランスなプール: wnear=1000 NEAR, トークン=1 NEAR 相当 → min=1
    let wnear = wnear();
    let near_1000_yocto: u128 = 1000 * 10u128.pow(24);
    // 1 NEAR 相当の USDT = 5_000_000 smallest
    let pool = make_pool(
        3,
        vec!["wrap.near", "usdt.tether-token.near"],
        vec![near_1000_yocto, 5_000_000],
    );

    let mut rates = HashMap::new();
    rates.insert(
        make_token("usdt.tether-token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6),
    );

    let result = estimate_pool_liquidity_in_near(&pool, &wnear, &rates);
    assert!(result.is_some());
    let liquidity = result.unwrap();
    assert_eq!(liquidity, NearValue::from_near(BigDecimal::from(1)));
}

#[test]
fn test_partial_rate_missing() {
    // 片方のレートのみ存在: wnear 側は直接変換可能
    let wnear = wnear();
    let near_200_yocto: u128 = 200 * 10u128.pow(24);
    let pool = make_pool(
        4,
        vec!["wrap.near", "unknown-token.near"],
        vec![near_200_yocto, 999999],
    );

    let rates = HashMap::new(); // レートなし

    let result = estimate_pool_liquidity_in_near(&pool, &wnear, &rates);
    assert!(result.is_some());
    // wnear 側のみで判定: 200 NEAR
    let liquidity = result.unwrap();
    assert_eq!(liquidity, NearValue::from_near(BigDecimal::from(200)));
}

#[test]
fn test_all_rates_missing() {
    // 全トークンのレートが不明（wnear も含まないプール）
    let wnear = wnear();
    let pool = make_pool(
        5,
        vec!["unknown-a.near", "unknown-b.near"],
        vec![1000, 2000],
    );

    let rates = HashMap::new();

    let result = estimate_pool_liquidity_in_near(&pool, &wnear, &rates);
    assert!(result.is_none());
}

#[test]
fn test_zero_amounts() {
    // amounts がゼロのプール → Some(NearValue::zero())
    let wnear = wnear();
    let pool = make_pool(6, vec!["wrap.near", "usdt.tether-token.near"], vec![0, 0]);

    let mut rates = HashMap::new();
    rates.insert(
        make_token("usdt.tether-token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6),
    );

    let result = estimate_pool_liquidity_in_near(&pool, &wnear, &rates);
    assert!(result.is_some());
    assert!(result.unwrap().is_zero());
}

#[test]
fn test_empty_pool() {
    // トークンが無い空プール → None
    let wnear = wnear();
    let pool = make_pool(7, vec![], vec![]);

    let rates = HashMap::new();

    let result = estimate_pool_liquidity_in_near(&pool, &wnear, &rates);
    assert!(result.is_none());
}

#[test]
fn test_zero_rate_token() {
    // ゼロレート (raw_rate=0) のトークンを含むプール → 流動性ゼロと判定
    let wnear = wnear();
    let near_500_yocto: u128 = 500 * 10u128.pow(24);
    let pool = make_pool(
        8,
        vec!["wrap.near", "worthless-token.near"],
        vec![near_500_yocto, 1_000_000],
    );

    let mut rates = HashMap::new();
    rates.insert(
        make_token("worthless-token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(0), 6),
    );

    let result = estimate_pool_liquidity_in_near(&pool, &wnear, &rates);
    assert!(result.is_some());
    // ゼロレートのトークンは NearValue::zero() として min 計算に含まれるため、
    // min(500 NEAR, 0 NEAR) = 0 NEAR
    assert!(result.unwrap().is_zero());
}

#[test]
fn test_effectively_zero_rate_token() {
    // raw_rate < 1 (取引不能) のトークンを含むプール → 流動性ゼロと判定
    let wnear = wnear();
    let near_500_yocto: u128 = 500 * 10u128.pow(24);
    let pool = make_pool(
        9,
        vec!["wrap.near", "micro-token.near"],
        vec![near_500_yocto, 1_000_000],
    );

    let mut rates = HashMap::new();
    rates.insert(
        make_token("micro-token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from_str("0.5").unwrap(), 6),
    );

    let result = estimate_pool_liquidity_in_near(&pool, &wnear, &rates);
    assert!(result.is_some());
    // raw_rate=0.5 < 1 は is_effectively_zero() で取引不能と判定され、
    // min(500 NEAR, 0 NEAR) = 0 NEAR
    assert!(result.unwrap().is_zero());
}

// =============================================================================
// filter_pools_by_liquidity テスト
// =============================================================================

#[test]
fn test_filter_keeps_sufficient_pools() {
    let wnear = wnear();
    let min_liquidity = NearValue::from_near(BigDecimal::from(100));

    let near_500_yocto: u128 = 500 * 10u128.pow(24);
    let pool = make_pool(
        1,
        vec!["wrap.near", "usdt.tether-token.near"],
        vec![near_500_yocto, 2_500_000_000],
    );

    let pools = Arc::new(dex::PoolInfoList::new(vec![pool]));

    let mut rates = HashMap::new();
    rates.insert(
        make_token("usdt.tether-token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6),
    );

    let filtered = filter_pools_by_liquidity(&pools, &wnear, &min_liquidity, &rates);
    assert_eq!(filtered.list().len(), 1);
}

#[test]
fn test_filter_removes_insufficient_pools() {
    let wnear = wnear();
    let min_liquidity = NearValue::from_near(BigDecimal::from(100));

    // 1 NEAR しかないプール
    let near_1_yocto: u128 = 10u128.pow(24);
    let pool = make_pool(
        1,
        vec!["wrap.near", "usdt.tether-token.near"],
        vec![near_1_yocto, 5_000_000],
    );

    let pools = Arc::new(dex::PoolInfoList::new(vec![pool]));

    let mut rates = HashMap::new();
    rates.insert(
        make_token("usdt.tether-token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6),
    );

    let filtered = filter_pools_by_liquidity(&pools, &wnear, &min_liquidity, &rates);
    assert_eq!(filtered.list().len(), 0);
}

#[test]
fn test_filter_empty_pool_list() {
    let wnear = wnear();
    let min_liquidity = NearValue::from_near(BigDecimal::from(100));
    let pools = Arc::new(dex::PoolInfoList::new(vec![]));
    let rates = HashMap::new();

    let filtered = filter_pools_by_liquidity(&pools, &wnear, &min_liquidity, &rates);
    assert_eq!(filtered.list().len(), 0);
}

#[test]
fn test_filter_mixed_pools() {
    // 3つのプール: 十分な流動性 / 不十分 / 全レート不明
    let wnear = wnear();
    let min_liquidity = NearValue::from_near(BigDecimal::from(100));

    // プール1: 500 NEAR（十分）
    let near_500_yocto: u128 = 500 * 10u128.pow(24);
    let pool_good = make_pool(
        1,
        vec!["wrap.near", "usdt.tether-token.near"],
        vec![near_500_yocto, 2_500_000_000],
    );

    // プール2: 1 NEAR（不十分）
    let near_1_yocto: u128 = 10u128.pow(24);
    let pool_bad = make_pool(
        2,
        vec!["wrap.near", "usdt.tether-token.near"],
        vec![near_1_yocto, 5_000_000],
    );

    // プール3: 全レート不明（除外される）
    let pool_unknown = make_pool(
        3,
        vec!["unknown-a.near", "unknown-b.near"],
        vec![1_000_000, 2_000_000],
    );

    let pools = Arc::new(dex::PoolInfoList::new(vec![
        pool_good,
        pool_bad,
        pool_unknown,
    ]));

    let mut rates = HashMap::new();
    rates.insert(
        make_token("usdt.tether-token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6),
    );

    let filtered = filter_pools_by_liquidity(&pools, &wnear, &min_liquidity, &rates);
    assert_eq!(
        filtered.list().len(),
        1,
        "Only the pool with sufficient liquidity should pass"
    );
}

#[test]
fn test_three_token_pool() {
    // 3トークンプール（stable pool のようなケース）
    // wnear: 100 NEAR, usdt: 100 NEAR 相当, usdc: 50 NEAR 相当
    let wnear = wnear();
    let near_100_yocto: u128 = 100 * 10u128.pow(24);
    // USDT: 100 NEAR = 500_000_000 smallest (rate=5_000_000, dec=6)
    // USDC: 50 NEAR = 255_000_000 smallest (rate=5_100_000, dec=6)
    let pool = make_pool(
        10,
        vec!["wrap.near", "usdt.tether-token.near", "usdc.token.near"],
        vec![near_100_yocto, 500_000_000, 255_000_000],
    );

    let mut rates = HashMap::new();
    rates.insert(
        make_token("usdt.tether-token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6),
    );
    rates.insert(
        make_token("usdc.token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_100_000), 6),
    );

    let result = estimate_pool_liquidity_in_near(&pool, &wnear, &rates);
    assert!(result.is_some());
    let liquidity = result.unwrap();
    // USDC side: 255_000_000 / 5_100_000 = 50 NEAR → min(100, 100, 50) = 50
    assert_eq!(liquidity, NearValue::from_near(BigDecimal::from(50)));
}

#[test]
fn test_filter_boundary_exactly_at_threshold() {
    // ちょうど閾値と等しいプール → >= なので通過する
    let wnear = wnear();
    let min_liquidity = NearValue::from_near(BigDecimal::from(100));

    // wnear 側がちょうど 100 NEAR
    let near_100_yocto: u128 = 100 * 10u128.pow(24);
    // USDT 側も 100 NEAR 相当 = 500_000_000 smallest
    let pool = make_pool(
        1,
        vec!["wrap.near", "usdt.tether-token.near"],
        vec![near_100_yocto, 500_000_000],
    );

    let pools = Arc::new(dex::PoolInfoList::new(vec![pool]));

    let mut rates = HashMap::new();
    rates.insert(
        make_token("usdt.tether-token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6),
    );

    let filtered = filter_pools_by_liquidity(&pools, &wnear, &min_liquidity, &rates);
    assert_eq!(
        filtered.list().len(),
        1,
        "Pool exactly at threshold should pass (>= check)"
    );
}

#[test]
fn test_wnear_in_rates_map_ignored() {
    // rates に wrap.near のレートが含まれていても、wnear 側は yocto→NEAR 直接変換を使う
    let wnear = wnear();
    let near_100_yocto: u128 = 100 * 10u128.pow(24);
    let pool = make_pool(
        1,
        vec!["wrap.near", "usdt.tether-token.near"],
        vec![near_100_yocto, 500_000_000],
    );

    let mut rates = HashMap::new();
    rates.insert(
        make_token("usdt.tether-token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6),
    );
    // wrap.near に巨大なレートを設定（もし参照されれば結果が大きく変わるはず）
    rates.insert(
        make_token("wrap.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(999_999_999_999_i64), 24),
    );

    let result = estimate_pool_liquidity_in_near(&pool, &wnear, &rates);
    assert!(result.is_some());
    let liquidity = result.unwrap();
    // wnear 側は rates を参照せず直接 yocto→NEAR 変換: 100 NEAR
    // USDT 側: 500_000_000 / 5_000_000 = 100 NEAR
    // min(100, 100) = 100
    assert_eq!(liquidity, NearValue::from_near(BigDecimal::from(100)));
}

// =============================================================================
// apply_liquidity_filter_and_select テスト（統合レベル）
// =============================================================================

fn make_account_id(name: &str) -> AccountId {
    name.parse().unwrap()
}

/// 低流動性トークンのプールが除外され、最終結果に含まれない
#[test]
fn test_select_excludes_low_liquidity_token() {
    let wnear = wnear();
    let wnear_in: TokenInAccount = wnear.clone().into();
    let min_liquidity = NearValue::from_near(BigDecimal::from(100));

    // プール1: wrap.near ↔ good-token.near (500 NEAR — 十分な流動性)
    let pool_good = make_pool(
        1,
        vec!["wrap.near", "good-token.near"],
        vec![500 * 10u128.pow(24), 2_500_000_000],
    );

    // プール2: wrap.near ↔ lowliq-token.near (1 NEAR — 不十分な流動性)
    let pool_low = make_pool(
        2,
        vec!["wrap.near", "lowliq-token.near"],
        vec![10u128.pow(24), 5_000_000],
    );

    let pools = Arc::new(dex::PoolInfoList::new(vec![pool_good, pool_low]));

    let mut rates = HashMap::new();
    rates.insert(
        make_token("good-token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6),
    );
    rates.insert(
        make_token("lowliq-token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6),
    );

    let tokens = vec![
        make_account_id("good-token.near"),
        make_account_id("lowliq-token.near"),
    ];

    let result = apply_liquidity_filter_and_select(
        tokens,
        &pools,
        &rates,
        &wnear,
        &wnear_in,
        &min_liquidity,
        10,
    )
    .unwrap();

    assert_eq!(result.len(), 1);
    assert_eq!(result[0], make_account_id("good-token.near"));
}

/// 片道しか到達できないトークンは最終結果から除外される
#[test]
fn test_select_excludes_one_way_token() {
    let wnear = wnear();
    let wnear_in: TokenInAccount = wnear.clone().into();
    let min_liquidity = NearValue::from_near(BigDecimal::from(10));

    // プール1: wrap.near ↔ good-token.near (双方向、十分な流動性)
    // good-token: rate=5_000_000(dec=6), 500_000_000 smallest = 100 NEAR 相当
    let pool_good = make_pool(
        1,
        vec!["wrap.near", "good-token.near"],
        vec![500 * 10u128.pow(24), 500_000_000],
    );

    // プール2: wrap.near → oneway-token.near のみ
    // oneway-token.near の amount=1 → 1/2=0 (integer div) → 復路エッジ生成不可
    let pool_oneway = make_pool(
        2,
        vec!["wrap.near", "oneway-token.near"],
        vec![500 * 10u128.pow(24), 1],
    );

    let pools = Arc::new(dex::PoolInfoList::new(vec![pool_good, pool_oneway]));

    let mut rates = HashMap::new();
    rates.insert(
        make_token("good-token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6),
    );
    // oneway-token.near のレートは不要（wrap.near 側の流動性だけで閾値を超える）

    let tokens = vec![
        make_account_id("good-token.near"),
        make_account_id("oneway-token.near"),
    ];

    let result = apply_liquidity_filter_and_select(
        tokens,
        &pools,
        &rates,
        &wnear,
        &wnear_in,
        &min_liquidity,
        10,
    )
    .unwrap();

    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0],
        make_account_id("good-token.near"),
        "One-way token should be excluded by graph reachability filter"
    );
}

/// 全プールが流動性フィルタで除外され、空グラフになった場合、
/// update_graph が Err → エラーを返す
#[test]
fn test_select_returns_error_on_graph_error() {
    let wnear = wnear();
    let wnear_in: TokenInAccount = wnear.clone().into();
    // min_liquidity=100 NEAR で全プールを除外する
    let min_liquidity = NearValue::from_near(BigDecimal::from(100));

    // プール: wrap.near ↔ token-a.near (1 NEAR — min_liquidity=100 で除外される)
    let pool = make_pool(
        1,
        vec!["wrap.near", "token-a.near"],
        vec![10u128.pow(24), 5_000_000],
    );

    let pools = Arc::new(dex::PoolInfoList::new(vec![pool]));

    let mut rates = HashMap::new();
    rates.insert(
        make_token("token-a.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6),
    );

    let tokens = vec![
        make_account_id("token-a.near"),
        make_account_id("token-b.near"),
    ];

    // 全プール除外 → グラフ空 → update_graph Err → エラーを返す
    let result = apply_liquidity_filter_and_select(
        tokens,
        &pools,
        &rates,
        &wnear,
        &wnear_in,
        &min_liquidity,
        10,
    );

    assert!(
        result.is_err(),
        "Should return error when graph construction fails"
    );
}

/// limit パラメータが正しくトークン数を制限する
#[test]
fn test_select_respects_limit() {
    let wnear = wnear();
    let wnear_in: TokenInAccount = wnear.clone().into();
    let min_liquidity = NearValue::from_near(BigDecimal::from(10));

    // 3つのプール: 全て十分な流動性
    let pool_a = make_pool(
        1,
        vec!["wrap.near", "token-a.near"],
        vec![500 * 10u128.pow(24), 500_000_000],
    );
    let pool_b = make_pool(
        2,
        vec!["wrap.near", "token-b.near"],
        vec![500 * 10u128.pow(24), 500_000_000],
    );
    let pool_c = make_pool(
        3,
        vec!["wrap.near", "token-c.near"],
        vec![500 * 10u128.pow(24), 500_000_000],
    );

    let pools = Arc::new(dex::PoolInfoList::new(vec![pool_a, pool_b, pool_c]));

    let mut rates = HashMap::new();
    for name in &["token-a.near", "token-b.near", "token-c.near"] {
        rates.insert(
            make_token(name),
            ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6),
        );
    }

    let tokens = vec![
        make_account_id("token-a.near"),
        make_account_id("token-b.near"),
        make_account_id("token-c.near"),
    ];

    // limit=2 で 3 個中 2 個のみ返る
    let result = apply_liquidity_filter_and_select(
        tokens,
        &pools,
        &rates,
        &wnear,
        &wnear_in,
        &min_liquidity,
        2,
    )
    .unwrap();

    assert_eq!(
        result.len(),
        2,
        "Result should be limited to 2 tokens when limit=2"
    );
}

/// グラフ上に到達可能だがボラティリティトークンに含まれない場合、
/// 全ボラティリティトークンがフィルタで除外されエラーを返す
#[test]
fn test_select_error_when_all_filtered_out() {
    let wnear = wnear();
    let wnear_in: TokenInAccount = wnear.clone().into();
    let min_liquidity = NearValue::from_near(BigDecimal::from(10));

    // プール: wrap.near ↔ other-token.near（グラフには存在するが volatility token ではない）
    let pool = make_pool(
        1,
        vec!["wrap.near", "other-token.near"],
        vec![500 * 10u128.pow(24), 500_000_000],
    );

    let pools = Arc::new(dex::PoolInfoList::new(vec![pool]));

    let mut rates = HashMap::new();
    rates.insert(
        make_token("other-token.near"),
        ExchangeRate::from_raw_rate(BigDecimal::from(5_000_000), 6),
    );

    // volatility token は unrelated-token.near（どのプールにも存在しない）
    let tokens = vec![make_account_id("unrelated-token.near")];

    let result = apply_liquidity_filter_and_select(
        tokens,
        &pools,
        &rates,
        &wnear,
        &wnear_in,
        &min_liquidity,
        10,
    );

    assert!(
        result.is_err(),
        "Should return error when no volatility tokens match buyable tokens"
    );
}
