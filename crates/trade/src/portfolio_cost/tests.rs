//! `compute_cost_deductions` の境界条件・defense-in-depth テスト
//!
//! `run_cost_aware_optimization` / `run_one_iteration` / `collect_cost_inputs`
//! は async + DB/RPC 依存のため、純粋ロジック関数 `compute_cost_deductions`
//! を直接突く形でユニットテストする。`PortfolioCostInputs` は BTreeMap で
//! 直構築可能なため、モックを用意せずとも以下の境界が網羅できる:
//!
//! - NaN / ±Infinity / 負値 target_w が `estimation_failures` に倒される
//!   （0 にクランプして Markowitz が「コストなし」と誤認する経路を遮断）
//! - tokens / target_weights / current_weights 長さ不一致が `debug_assert_eq!` で fail-loud
//! - total_value=0（全 held_size=0）で全 token が `estimation_failures` に合流
//! - 全 target_w=0 は estimation_failures ではなく deductions[i] = 0
//!   （Δw refactor の semantics: target_w=0 は full exit 経路、Markowitz
//!   は weight=0 で当該銘柄を 0 寄与にする）
//! - `target_w == current_w` で Δw ≈ 0 → deductions[i] = 0 (取引なし)
//! - 部分 exit (current=1.0 → target=0.6) は full entry (current=0 → target=0.6)
//!   より低い deduction（trade size がより小さいため variable_cost が縮む）
//! - `inputs.paths` / `inputs.rates` の片方欠損は防御的にスキップ
//! - `existing_deposits` ヒットで storage 固定費が抑制される

use super::*;
use bigdecimal::{BigDecimal, FromPrimitive};
use blockchain::types::gas_price::GasPrice;
use chrono::{TimeDelta, Utc};
use common::algorithm::portfolio::PortfolioData;
use common::algorithm::types::{PriceHistory, PricePoint, TokenData, WalletInfo};
use common::types::{
    ExchangeRate, NearValue, TokenAccount, TokenInAccount, TokenOutAccount, TokenPrice, YoctoValue,
};
use dex::TokenPath;
use near_sdk::NearToken;
use std::collections::{BTreeMap, HashSet};
use std::str::FromStr;

const ONE_NEAR_YOCTO: u128 = 1_000_000_000_000_000_000_000_000;

/// テスト用の TokenOutAccount を `name.test` 形式で構築する。
fn token(name: &str) -> TokenOutAccount {
    TokenOutAccount::from_str(&format!("{name}.test")).expect("valid account id")
}

/// テスト用 `TokenData`。`compute_cost_deductions` は symbol / current_rate のみ
/// 参照する（他フィールドは未使用）ので、最低限の値で構築する。
fn token_data(symbol: TokenOutAccount) -> TokenData {
    TokenData {
        symbol,
        current_rate: ExchangeRate::wnear(),
        historical_volatility: 0.1,
        liquidity_score: Some(0.5),
        market_cap: Some(NearValue::from_near(BigDecimal::from(1_000))),
    }
}

/// テスト用 `GasPrice`（実運用相当の 1e8 yoctoNEAR/gas）。
fn gas_price() -> GasPrice {
    GasPrice::from_balance(NearToken::from_yoctonear(100_000_000))
}

/// 空の `TokenPath`。`calc_value(initial)` は `initial` をそのまま返すため、
/// AMM 損失部分は 0 になり variable_ratio は `EXPECTED_SLIPPAGE_DEDUCTION` のみ。
fn empty_path() -> TokenPath {
    TokenPath(vec![])
}

/// 全 token に空 path / wnear レートが用意された `PortfolioCostInputs`。
///
/// `compute_cost_deductions` の入力としては必要十分（estimate_trade_cost が
/// 成功する経路）。`existing_deposits` を渡すと当該 token の storage 固定費が
/// 0 化される。
fn make_inputs(
    tokens: &[TokenOutAccount],
    existing_deposits: HashSet<TokenAccount>,
) -> PortfolioCostInputs {
    let mut paths = BTreeMap::new();
    let mut rates = BTreeMap::new();
    for t in tokens {
        paths.insert(t.clone(), empty_path());
        rates.insert(t.clone(), ExchangeRate::wnear());
    }
    PortfolioCostInputs {
        gas_price: gas_price(),
        // 0.1 NEAR — 実運用相当
        storage_min: YoctoValue::from_yocto_u128(100_000_000_000_000_000_000_000),
        existing_deposits,
        paths,
        rates,
        failed_tokens: vec![],
    }
}

// ---------------------------------------------------------------------------
// (a) NaN / ±Infinity / 負値 target_w
// ---------------------------------------------------------------------------

#[test]
fn test_compute_cost_deductions_nan_target_w_falls_through_to_failures() {
    // NaN target_w は upstream のロジック異常シグナル。silent に 0 へ
    // 倒すと optimizer が「コストなし」で誤認するため、estimation_failures
    // に合流させて retain_excluding で除外する。
    let sym = token("nan");
    let tokens = vec![token_data(sym.clone())];
    let inputs = make_inputs(std::slice::from_ref(&sym), HashSet::new());
    let total = BigDecimal::from(ONE_NEAR_YOCTO);
    let result = compute_cost_deductions(&[f64::NAN], &[0.0], &tokens, &inputs, &total);
    assert!(
        result.deductions.is_empty(),
        "NaN target_w must not produce a deduction entry"
    );
    assert_eq!(result.estimation_failures, vec![sym]);
}

#[test]
fn test_compute_cost_deductions_positive_infinity_target_w_falls_through_to_failures() {
    let sym = token("posinf");
    let tokens = vec![token_data(sym.clone())];
    let inputs = make_inputs(std::slice::from_ref(&sym), HashSet::new());
    let total = BigDecimal::from(ONE_NEAR_YOCTO);
    let result = compute_cost_deductions(&[f64::INFINITY], &[0.0], &tokens, &inputs, &total);
    assert!(result.deductions.is_empty());
    assert_eq!(result.estimation_failures, vec![sym]);
}

#[test]
fn test_compute_cost_deductions_negative_infinity_target_w_falls_through_to_failures() {
    let sym = token("neginf");
    let tokens = vec![token_data(sym.clone())];
    let inputs = make_inputs(std::slice::from_ref(&sym), HashSet::new());
    let total = BigDecimal::from(ONE_NEAR_YOCTO);
    let result = compute_cost_deductions(&[f64::NEG_INFINITY], &[0.0], &tokens, &inputs, &total);
    assert!(result.deductions.is_empty());
    assert_eq!(result.estimation_failures, vec![sym]);
}

#[test]
fn test_compute_cost_deductions_negative_finite_target_w_falls_through_to_failures() {
    let sym = token("neg");
    let tokens = vec![token_data(sym.clone())];
    let inputs = make_inputs(std::slice::from_ref(&sym), HashSet::new());
    let total = BigDecimal::from(ONE_NEAR_YOCTO);
    let result = compute_cost_deductions(&[-0.5], &[0.0], &tokens, &inputs, &total);
    assert!(result.deductions.is_empty());
    assert_eq!(result.estimation_failures, vec![sym]);
}

// ---------------------------------------------------------------------------
// (b) tokens / weights 長さ不一致
// ---------------------------------------------------------------------------

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "tokens and target_weights must have the same length")]
fn test_compute_cost_deductions_length_mismatch_panics_in_debug() {
    let a = token("a");
    let b = token("b");
    let tokens = vec![token_data(a.clone()), token_data(b.clone())];
    let inputs = make_inputs(&[a, b], HashSet::new());
    let total = BigDecimal::from(ONE_NEAR_YOCTO);
    let _ = compute_cost_deductions(&[0.5], &[0.0, 0.0], &tokens, &inputs, &total);
}

// ---------------------------------------------------------------------------
// (c) total_value=0 / 全 weight=0 ケース
// ---------------------------------------------------------------------------

#[test]
fn test_compute_cost_deductions_total_value_zero_drops_all_held_tokens_to_failures() {
    // total_value=0 → held_size = 0 で ZeroPosition → estimation_failures。
    // Δw refactor 後も「held=0 はコスト比率を割れない」不変条件は維持される。
    let a = token("a");
    let b = token("b");
    let tokens = vec![token_data(a.clone()), token_data(b.clone())];
    let inputs = make_inputs(&[a.clone(), b.clone()], HashSet::new());
    let total = BigDecimal::from(0);
    let result = compute_cost_deductions(&[0.5, 0.5], &[0.0, 0.0], &tokens, &inputs, &total);
    assert!(result.deductions.is_empty());
    assert_eq!(result.estimation_failures, vec![a, b]);
}

#[test]
fn test_compute_cost_deductions_all_zero_target_w_yields_zero_deductions() {
    // Δw refactor: 全 target_w = 0 でも estimation_failures ではなく
    // deductions[i] = 0 を返す。Markowitz 側で `weight × (r - 0) = 0` に
    // なって当該銘柄に依存しないため、portfolio 比較に影響しない。
    // current_w も 0 なので「保有なし、目標も 0」で実取引も発生しない。
    let a = token("a");
    let b = token("b");
    let tokens = vec![token_data(a.clone()), token_data(b.clone())];
    let inputs = make_inputs(&[a.clone(), b.clone()], HashSet::new());
    let total = BigDecimal::from(ONE_NEAR_YOCTO);
    let result = compute_cost_deductions(&[0.0, 0.0], &[0.0, 0.0], &tokens, &inputs, &total);
    assert!(result.estimation_failures.is_empty());
    assert_eq!(result.deductions.len(), 2);
    for sym in [&a, &b] {
        assert_eq!(result.deductions.get(sym).copied(), Some(0.0));
    }
}

#[test]
fn test_compute_cost_deductions_target_w_zero_with_current_w_yields_zero_deduction() {
    // target_w=0 で current_w > 0 (full exit)。Markowitz の構造的限界として
    // SELL コストは weight=0 で打ち消されるが、deduction フィールド自体は
    // 0 として埋めて failures に倒さない（optimizer 入力との単位整合）。
    let sym = token("exit-only");
    let tokens = vec![token_data(sym.clone())];
    let inputs = make_inputs(std::slice::from_ref(&sym), HashSet::new());
    let total = BigDecimal::from(ONE_NEAR_YOCTO);
    let result = compute_cost_deductions(&[0.0], &[0.5], &tokens, &inputs, &total);
    assert!(result.estimation_failures.is_empty());
    assert_eq!(result.deductions.get(&sym).copied(), Some(0.0));
}

// ---------------------------------------------------------------------------
// (c.1) target_w=0 を含む mixed weight シナリオ
// ---------------------------------------------------------------------------

#[test]
fn test_compute_cost_deductions_mixed_target_w_zero_yields_zero_deduction_with_full_exit() {
    // Δw refactor 後: target_w=0 で current_w>0 (full exit) は estimation_failures
    // ではなく deductions[i] = 0 を返す。Markowitz は weight=0 で当該銘柄を
    // 0 寄与にするため exit cost は portfolio 比較に流れないが、deductions マップ
    // としては有効値として埋まる。entry tokens は通常通り cost 計算される。
    let entry_a = token("entry-a");
    let exit_b = token("exit-b");
    let entry_c = token("entry-c");
    let tokens = vec![
        token_data(entry_a.clone()),
        token_data(exit_b.clone()),
        token_data(entry_c.clone()),
    ];
    let inputs = make_inputs(
        &[entry_a.clone(), exit_b.clone(), entry_c.clone()],
        HashSet::new(),
    );
    let total = BigDecimal::from(ONE_NEAR_YOCTO);

    // current_w: exit_b は 0.5 を保有中 (full exit シナリオ)、他は entry from cash
    let result =
        compute_cost_deductions(&[0.6, 0.0, 0.4], &[0.0, 0.5, 0.0], &tokens, &inputs, &total);

    // exit token は deductions に 0.0 として現れる（failures ではない）
    assert!(result.estimation_failures.is_empty());
    assert_eq!(result.deductions.get(&exit_b).copied(), Some(0.0));

    // entry tokens は通常の cost 計算で deduction を得る
    for entry in [&entry_a, &entry_c] {
        let v = *result
            .deductions
            .get(entry)
            .unwrap_or_else(|| panic!("entry token {entry} must produce a deduction"));
        assert!(
            v.is_finite() && v >= 0.0,
            "deduction for {entry} must be finite-non-negative: got {v}"
        );
    }
    assert_eq!(result.deductions.len(), 3);
}

#[test]
fn test_compute_cost_deductions_higher_held_dilutes_fixed_cost_ratio() {
    // Entry from cash の比較: target_w が大きい銘柄ほど held_size も大きく、
    // fixed_cost が希釈されて deduction ratio が小さくなる。
    // [0.9, 0.1] vs current=[0,0] → trade==held で旧モデルと同等の振る舞い。
    let entry_a = token("big");
    let entry_b = token("small");
    let tokens = vec![token_data(entry_a.clone()), token_data(entry_b.clone())];
    let inputs = make_inputs(&[entry_a.clone(), entry_b.clone()], HashSet::new());
    let total = BigDecimal::from(ONE_NEAR_YOCTO);

    let result = compute_cost_deductions(&[0.9, 0.1], &[0.0, 0.0], &tokens, &inputs, &total);

    let v_big = *result
        .deductions
        .get(&entry_a)
        .expect("big token deduction");
    let v_small = *result
        .deductions
        .get(&entry_b)
        .expect("small token deduction");

    assert!(
        v_big < v_small,
        "larger held dilutes fixed-cost share: big={v_big} small={v_small}"
    );
    assert!(v_big.is_finite() && v_big >= 0.0);
    assert!(v_small.is_finite() && v_small >= 0.0);
}

#[test]
fn test_compute_cost_deductions_no_op_when_target_matches_current() {
    // Δw ≈ 0 (target_w == current_w) なら取引なし → deduction = 0。
    // estimate_trade_cost を呼ばない短絡経路で、cost 推定の AMM ロード /
    // gas 推定を回避するパフォーマンス上の利点もある。
    let sym = token("hold");
    let tokens = vec![token_data(sym.clone())];
    let inputs = make_inputs(std::slice::from_ref(&sym), HashSet::new());
    let total = BigDecimal::from(ONE_NEAR_YOCTO);

    let result = compute_cost_deductions(&[0.6], &[0.6], &tokens, &inputs, &total);

    assert!(result.estimation_failures.is_empty());
    assert_eq!(result.deductions.get(&sym).copied(), Some(0.0));
}

#[test]
fn test_compute_cost_deductions_partial_exit_lower_than_full_entry() {
    // 部分 exit (current=1.0 → target=0.6, |Δw|=0.4) は held=0.6 で
    // 同 held を full entry (current=0 → target=0.6) した場合より trade size が
    // 小さい (0.4 vs 0.6) ので variable_cost 部分が減り、deduction ratio が
    // 小さくなる。Δw refactor の本質を pin する property test。
    let sym = token("partial");
    let tokens = vec![token_data(sym.clone())];
    let inputs = make_inputs(std::slice::from_ref(&sym), HashSet::new());
    let total = BigDecimal::from(ONE_NEAR_YOCTO);

    let partial_exit = compute_cost_deductions(&[0.6], &[1.0], &tokens, &inputs, &total);
    let full_entry = compute_cost_deductions(&[0.6], &[0.0], &tokens, &inputs, &total);

    let v_partial = *partial_exit
        .deductions
        .get(&sym)
        .expect("partial-exit deduction");
    let v_full = *full_entry
        .deductions
        .get(&sym)
        .expect("full-entry deduction");

    assert!(
        v_partial < v_full,
        "partial exit (smaller |Δw|) must yield lower deduction than full entry: \
         partial={v_partial} full={v_full}"
    );
    assert!(v_partial.is_finite() && v_partial >= 0.0);
    assert!(v_full.is_finite() && v_full >= 0.0);
}

// ---------------------------------------------------------------------------
// 空入力 / defense-in-depth スキップ
// ---------------------------------------------------------------------------

#[test]
fn test_compute_cost_deductions_empty_tokens_returns_empty_result() {
    let inputs = make_inputs(&[], HashSet::new());
    let total = BigDecimal::from(ONE_NEAR_YOCTO);
    let result = compute_cost_deductions(&[], &[], &[], &inputs, &total);
    assert!(result.deductions.is_empty());
    assert!(result.estimation_failures.is_empty());
}

#[test]
fn test_compute_cost_deductions_missing_path_silently_skips_token() {
    // path が `inputs.paths` にない token は estimation_failures にも入らず
    // silent に skip される（defense-in-depth: retain_excluding 後の残留異常
    // に備えた防御的 continue）。
    let present = token("present");
    let missing = token("missing");
    let tokens = vec![token_data(present.clone()), token_data(missing.clone())];
    // make_inputs に渡すのは present だけ → missing.path は欠損
    let inputs = make_inputs(std::slice::from_ref(&present), HashSet::new());
    let total = BigDecimal::from(ONE_NEAR_YOCTO);
    let result = compute_cost_deductions(&[0.5, 0.5], &[0.0, 0.0], &tokens, &inputs, &total);
    // present は正常経路で deduction or failures いずれかに入る
    let total_referenced = result.deductions.len() + result.estimation_failures.len();
    assert_eq!(
        total_referenced, 1,
        "missing-path token must not appear in either map (silent skip)"
    );
    assert!(
        !result.deductions.contains_key(&missing) && !result.estimation_failures.contains(&missing),
        "missing token must not appear anywhere"
    );
}

#[test]
fn test_compute_cost_deductions_missing_rate_silently_skips_token() {
    // rate のみ欠損ケース（path はあるが rate が retain_excluding ですり抜けた
    // 異常状態のシミュレート）。同様に silent skip する。
    let present = token("present");
    let missing_rate = token("missing_rate");
    let tokens = vec![
        token_data(present.clone()),
        token_data(missing_rate.clone()),
    ];
    let mut inputs = make_inputs(&[present.clone(), missing_rate.clone()], HashSet::new());
    inputs.rates.remove(&missing_rate);
    let total = BigDecimal::from(ONE_NEAR_YOCTO);
    let result = compute_cost_deductions(&[0.5, 0.5], &[0.0, 0.0], &tokens, &inputs, &total);
    assert!(
        !result.deductions.contains_key(&missing_rate)
            && !result.estimation_failures.contains(&missing_rate),
        "rate-missing token must be silently skipped"
    );
}

// ---------------------------------------------------------------------------
// 正常経路 + existing_deposits の効果
// ---------------------------------------------------------------------------

#[test]
fn test_compute_cost_deductions_normal_token_produces_finite_deduction() {
    // entry from cash (current=0 → target=1.0) で deduction が得られる。
    // 値は EXPECTED_SLIPPAGE_DEDUCTION (variable) + storage/gas (fixed) の和で、
    // CostDeduction::new の不変条件 (`is_finite() && >= 0.0`) を満たす。
    let sym = token("normal");
    let tokens = vec![token_data(sym.clone())];
    let inputs = make_inputs(std::slice::from_ref(&sym), HashSet::new());
    let total = BigDecimal::from(ONE_NEAR_YOCTO);
    let result = compute_cost_deductions(&[1.0], &[0.0], &tokens, &inputs, &total);
    assert!(result.estimation_failures.is_empty());
    let value = *result
        .deductions
        .get(&sym)
        .expect("normal-path token must produce a deduction");
    assert!(value.is_finite() && value >= 0.0);
}

#[test]
fn test_compute_cost_deductions_existing_deposit_lowers_fixed_cost() {
    // 同条件で existing_deposits を切り替えると、deposit 済み (new_token_count=0)
    // 側の fixed_cost (storage 部分) が 0 化されて deduction が小さくなる。
    let sym = token("dep");
    let tokens = vec![token_data(sym.clone())];
    let total = BigDecimal::from(ONE_NEAR_YOCTO);

    // case A: deposit なし → storage 固定費が掛かる
    let inputs_no_dep = make_inputs(std::slice::from_ref(&sym), HashSet::new());
    let r_no_dep = compute_cost_deductions(&[1.0], &[0.0], &tokens, &inputs_no_dep, &total);
    let v_no_dep = *r_no_dep
        .deductions
        .get(&sym)
        .expect("non-deposit token must succeed");

    // case B: deposit あり → storage 固定費 0 → variable + gas のみ
    let mut deposits = HashSet::new();
    deposits.insert(TokenAccount::from(sym.clone()));
    let inputs_with_dep = make_inputs(std::slice::from_ref(&sym), deposits);
    let r_with_dep = compute_cost_deductions(&[1.0], &[0.0], &tokens, &inputs_with_dep, &total);
    let v_with_dep = *r_with_dep
        .deductions
        .get(&sym)
        .expect("deposit token must succeed");

    assert!(
        v_with_dep < v_no_dep,
        "existing_deposits must reduce fixed cost: with_dep={v_with_dep} vs no_dep={v_no_dep}"
    );
    assert!(v_with_dep.is_finite() && v_with_dep >= 0.0);
}

// ---------------------------------------------------------------------------
// (g.5) run_cost_aware_optimization integration: 病理パスのみ
// ---------------------------------------------------------------------------

fn empty_wallet() -> WalletInfo {
    WalletInfo {
        holdings: BTreeMap::new(),
        total_value: NearValue::from_near(BigDecimal::from(1000)),
        cash_balance: NearValue::zero(),
    }
}

#[tokio::test]
async fn test_run_cost_aware_optimization_zero_tokens_returns_hold() {
    // n = 0: 一度も最適化に入らず Hold で早期リターン。
    // first iteration の `Some(state)` が成立しない経路の代わりに、
    // tokens 配列が空のとき即 Hold する分岐を直接検証する。
    let wallet = empty_wallet();
    let pd = PortfolioData::default();
    let inputs = make_inputs(&[], HashSet::new());
    let total = BigDecimal::from(ONE_NEAR_YOCTO);

    let outcome = run_cost_aware_optimization(&wallet, pd, &inputs, &total, 10, 0.5, 0.05)
        .await
        .expect("Hold path returns Ok");

    assert!(matches!(outcome, CostAwareOutcome::Hold));
}

/// `hard_filter_tokens` を通過する `TokenData`（market_cap >= 10000 NEAR、
/// liquidity_score >= 0.5）。
fn passing_token_data(symbol: TokenOutAccount) -> TokenData {
    TokenData {
        symbol,
        current_rate: ExchangeRate::wnear(),
        historical_volatility: 0.1,
        liquidity_score: Some(0.8),
        market_cap: Some(NearValue::from_near(BigDecimal::from(100_000))),
    }
}

/// 30 日分の単調増加 PriceHistory（共分散・期待リターン計算で発散しない値域）。
fn linear_price_history(symbol: &TokenOutAccount, base: f64) -> PriceHistory {
    let base_time = Utc::now() - TimeDelta::days(30);
    let prices: Vec<PricePoint> = (0..30)
        .map(|i| PricePoint {
            timestamp: base_time + TimeDelta::days(i),
            price: TokenPrice::from_near_per_token(
                BigDecimal::from_f64(base + (i as f64) * 0.01).expect("finite"),
            ),
            volume: Some(BigDecimal::from(1000)),
        })
        .collect();
    PriceHistory {
        token: symbol.clone(),
        quote_token: TokenInAccount::from_str("wrap.near").expect("valid wnear"),
        prices,
    }
}

/// 「first iter で得られる optimal weights ≈ uniform」になるよう対称な
/// 2-token portfolio を組み立てる。
fn symmetric_two_token_portfolio() -> (PortfolioData, [TokenOutAccount; 2]) {
    let a = token("sym-a");
    let b = token("sym-b");
    let tokens = vec![passing_token_data(a.clone()), passing_token_data(b.clone())];

    let mut historical_prices = BTreeMap::new();
    historical_prices.insert(a.clone(), linear_price_history(&a, 1.0));
    historical_prices.insert(b.clone(), linear_price_history(&b, 1.0));

    // 同一トレンドで上昇率も同じ → 期待リターンが対称
    let mut predictions = BTreeMap::new();
    let target = TokenPrice::from_near_per_token(BigDecimal::from_f64(1.5).expect("finite"));
    predictions.insert(a.clone(), target.clone());
    predictions.insert(b.clone(), target);

    let pd = PortfolioData {
        tokens,
        predictions,
        historical_prices,
        ..Default::default()
    };
    (pd, [a, b])
}

#[tokio::test]
async fn test_run_cost_aware_optimization_converges_within_tolerance() {
    // 早期 break (`max_diff < CONVERGENCE_TOLERANCE`) パスを pin する。
    // 対称 2-token portfolio で first iter が ~uniform [0.5, 0.5] を返し、
    // damping = 0.001 では max_diff = 0.001 × |Δw| ≤ 0.001 × 0.5 = 5e-4 < 1e-3
    // → loop 開始直後に break。max_iter = 1 (scaled to 10) で hard-cap も pin。
    //
    // 検証ポイント:
    //   - Optimized outcome を返す（Hold ではない）
    //   - 反復が hard-cap 内 (≤ 100) で完了し DoS にならない
    //   - 最終 weights が 0.0 ≤ w ≤ 1.0 で finite（NaN cascade 経路に流入していない）
    let (pd, [a, b]) = symmetric_two_token_portfolio();
    let inputs = make_inputs(&[a, b], HashSet::new());
    let total = BigDecimal::from(ONE_NEAR_YOCTO);
    let wallet = empty_wallet();

    // damping = 0.001 (clamp 漏れ想定値) で early break を強制
    let outcome = run_cost_aware_optimization(&wallet, pd, &inputs, &total, 1, 0.001, 0.05)
        .await
        .expect("convergence path returns Ok");

    let report = match outcome {
        CostAwareOutcome::Optimized(r) => r,
        CostAwareOutcome::Hold => panic!("symmetric portfolio must produce Optimized outcome"),
    };

    // 最終 weights が finite 範囲内
    for (sym, w) in report.optimal_weights.weights.iter() {
        let v = w.to_f64().unwrap_or(f64::NAN);
        assert!(
            v.is_finite() && (0.0..=1.0).contains(&v),
            "weight for {sym} out of valid range: {v}"
        );
    }
}

#[tokio::test]
async fn test_run_cost_aware_optimization_all_tokens_fail_cost_returns_hold() {
    // 全 token が cost 推定で失敗する経路: total_value=0 で全 weight が
    // assumed_in=0 経由 ZeroPosition → estimation_failures に合流 →
    // retain_excluding で全除外 → first iteration が None → Hold。
    let sym_a = token("hold-a");
    let sym_b = token("hold-b");
    let pd = PortfolioData {
        tokens: vec![token_data(sym_a.clone()), token_data(sym_b.clone())],
        ..Default::default()
    };
    let inputs = make_inputs(&[sym_a, sym_b], HashSet::new());
    // total_value_yocto = 0 → assumed_in = 0 → ZeroPosition → 全 token 脱落
    let total = BigDecimal::from(0);
    let wallet = empty_wallet();

    let outcome = run_cost_aware_optimization(&wallet, pd, &inputs, &total, 5, 0.5, 0.05)
        .await
        .expect("Hold path returns Ok");

    assert!(matches!(outcome, CostAwareOutcome::Hold));
}

// ---------------------------------------------------------------------------
// (h) scale_max_iter_by_damping: damping に応じた反復上限スケーリング
// ---------------------------------------------------------------------------

#[test]
fn test_scale_max_iter_by_damping_no_damping_returns_max_iter() {
    // damping = 1.0 (full step) → 倍率 1 → max_iter そのまま
    assert_eq!(scale_max_iter_by_damping(10, 1.0), 10);
}

#[test]
fn test_scale_max_iter_by_damping_half_doubles() {
    // damping = 0.5 → ⌈1/0.5⌉ = 2 → 反復上限を 2 倍
    assert_eq!(scale_max_iter_by_damping(10, 0.5), 20);
}

#[test]
fn test_scale_max_iter_by_damping_lower_bound_scales_to_ten_times() {
    // damping = 0.1 (防御下限) → ⌈1/0.1⌉ = 10 → 反復上限を 10 倍
    // (0.9)^100 ≈ 2.7e-5 < CONVERGENCE_TOLERANCE で確実に収束する headroom
    assert_eq!(scale_max_iter_by_damping(10, 0.1), 100);
}

#[test]
fn test_scale_max_iter_by_damping_zero_damping_returns_max_iter() {
    // 想定外の damping = 0.0（NaN フォールバックや事前 clamp で起こらないが
    // defense-in-depth として）。0 除算を避けて max_iter そのまま。
    assert_eq!(scale_max_iter_by_damping(10, 0.0), 10);
}

#[test]
fn test_scale_max_iter_by_damping_zero_max_iter_returns_one() {
    // max_iter = 0（不正値）でも最低 1 反復は保証
    assert_eq!(scale_max_iter_by_damping(0, 0.5), 2);
}

#[test]
fn test_scale_max_iter_by_damping_within_clamped_range_caps_at_100() {
    // production clamped 範囲 (max_iter ≤ 10, damping ≥ 0.1) で合計 100 反復以下
    for max_iter in 1..=10 {
        for damping_int in 1..=10 {
            let damping = damping_int as f64 / 10.0;
            let total = scale_max_iter_by_damping(max_iter, damping);
            assert!(
                total <= 100,
                "max_iter={max_iter} damping={damping} total={total} exceeds 100"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// (h.1) typed config bypass 経路での fail-loud hard-cap
// ---------------------------------------------------------------------------

#[test]
fn test_scale_max_iter_by_damping_subnormal_damping_caps_at_max_total() {
    // typed config の [0.1, 1.0] clamp が bypass された経路で
    // `damping = 1e-300` が漏れ込んだ場合、`1.0 / damping = 1e300` を直接
    // `as usize` すると platform-dependent に `usize::MAX` 等になる。
    // hard-cap (MAX_DAMPING_SCALE * max_iter ≤ MAX_TOTAL_ITERATIONS) で
    // 構造的に上限以下に収まることを pin する。
    assert_eq!(scale_max_iter_by_damping(10, 1e-300), 100);
}

#[test]
fn test_scale_max_iter_by_damping_below_clamp_lower_bound_caps() {
    // damping = 0.01 (clamp 下限 0.1 より小さい異常値) でも
    // 倍率は MAX_DAMPING_SCALE = 10 で止まり、`max_iter * 10` で頭打ち。
    assert_eq!(scale_max_iter_by_damping(5, 0.01), 50);
    assert_eq!(scale_max_iter_by_damping(10, 0.01), 100);
}

#[test]
fn test_scale_max_iter_by_damping_oversize_max_iter_caps_at_max_total() {
    // max_iter が typed config の clamp (≤ 10) を bypass した経路でも
    // MAX_TOTAL_ITERATIONS = 100 で頭打ち。
    assert_eq!(scale_max_iter_by_damping(1_000, 0.5), 100);
    assert_eq!(scale_max_iter_by_damping(usize::MAX, 1.0), 100);
}

#[test]
fn test_scale_max_iter_by_damping_nan_damping_returns_max_iter() {
    // NaN は `damping > 0.0` ガードで else 経路 → scale = 1 → max_iter のみ。
    // 旧実装で `(1.0 / NaN) as usize` が platform-dependent な結果を返す
    // 経路を構造的に閉じる。
    assert_eq!(scale_max_iter_by_damping(10, f64::NAN), 10);
}

#[test]
fn test_scale_max_iter_by_damping_negative_damping_returns_max_iter() {
    // 負の damping も `damping > 0.0` ガードで else 経路 → scale = 1。
    assert_eq!(scale_max_iter_by_damping(10, -0.5), 10);
}
