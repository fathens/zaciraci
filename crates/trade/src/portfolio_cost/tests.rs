//! `compute_cost_deductions` の境界条件・defense-in-depth テスト
//!
//! `run_cost_aware_optimization` / `run_one_iteration` / `collect_cost_inputs`
//! は async + DB/RPC 依存のため、純粋ロジック関数 `compute_cost_deductions`
//! を直接突く形でユニットテストする。`PortfolioCostInputs` は BTreeMap で
//! 直構築可能なため、モックを用意せずとも以下の境界が網羅できる:
//!
//! - NaN / ±Infinity / 負値 weight が `is_finite() && >= 0.0` 境界で 0 に
//!   クランプされ、Markowitz への NaN cascade 経路が型レベルで塞がれる
//! - tokens / weights 長さ不一致が `debug_assert_eq!` で fail-loud
//! - total_value=0 や全 weight=0 で全 token が `estimation_failures` 経路に
//!   合流（NaN を埋めず除外する設計の検証）
//! - `inputs.paths` / `inputs.rates` の片方欠損は防御的にスキップ
//! - `existing_deposits` ヒットで storage 固定費が抑制される

use super::*;
use bigdecimal::BigDecimal;
use blockchain::types::gas_price::GasPrice;
use common::algorithm::types::TokenData;
use common::types::{ExchangeRate, NearValue, TokenAccount, TokenOutAccount, YoctoValue};
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
// (a) NaN / ±Infinity / 負値 weight クランプ
// ---------------------------------------------------------------------------

#[test]
fn test_compute_cost_deductions_nan_weight_clamps_to_zero_and_falls_through() {
    // NaN weight は入口で 0.0 にクランプされ、assumed_in=0 → ZeroPosition
    // → estimation_failures 経路に合流する（NaN を Markowitz に流入させない）。
    let sym = token("nan");
    let tokens = vec![token_data(sym.clone())];
    let inputs = make_inputs(std::slice::from_ref(&sym), HashSet::new());
    let total = BigDecimal::from(ONE_NEAR_YOCTO);
    let result = compute_cost_deductions(&[f64::NAN], &tokens, &inputs, &total);
    assert!(
        result.deductions.is_empty(),
        "NaN weight must not produce a deduction entry"
    );
    assert_eq!(result.estimation_failures, vec![sym]);
}

#[test]
fn test_compute_cost_deductions_positive_infinity_weight_clamps_to_zero() {
    // +Infinity も is_finite() == false により 0 化、ZeroPosition 経路へ。
    let sym = token("posinf");
    let tokens = vec![token_data(sym.clone())];
    let inputs = make_inputs(std::slice::from_ref(&sym), HashSet::new());
    let total = BigDecimal::from(ONE_NEAR_YOCTO);
    let result = compute_cost_deductions(&[f64::INFINITY], &tokens, &inputs, &total);
    assert!(result.deductions.is_empty());
    assert_eq!(result.estimation_failures, vec![sym]);
}

#[test]
fn test_compute_cost_deductions_negative_infinity_weight_clamps_to_zero() {
    let sym = token("neginf");
    let tokens = vec![token_data(sym.clone())];
    let inputs = make_inputs(std::slice::from_ref(&sym), HashSet::new());
    let total = BigDecimal::from(ONE_NEAR_YOCTO);
    let result = compute_cost_deductions(&[f64::NEG_INFINITY], &tokens, &inputs, &total);
    assert!(result.deductions.is_empty());
    assert_eq!(result.estimation_failures, vec![sym]);
}

#[test]
fn test_compute_cost_deductions_negative_finite_weight_clamps_to_zero() {
    // 有限の負値も `.max(0.0)` で 0 化される（不変条件: w >= 0.0）。
    let sym = token("neg");
    let tokens = vec![token_data(sym.clone())];
    let inputs = make_inputs(std::slice::from_ref(&sym), HashSet::new());
    let total = BigDecimal::from(ONE_NEAR_YOCTO);
    let result = compute_cost_deductions(&[-0.5], &tokens, &inputs, &total);
    assert!(result.deductions.is_empty());
    assert_eq!(result.estimation_failures, vec![sym]);
}

// ---------------------------------------------------------------------------
// (b) tokens / weights 長さ不一致
// ---------------------------------------------------------------------------

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "tokens and weights must have the same length")]
fn test_compute_cost_deductions_length_mismatch_panics_in_debug() {
    // G2 の zip + debug_assert_eq! 構造の検証。release ビルドでは silent
    // truncation するが、debug ビルドで早期検出する設計を確認する。
    let a = token("a");
    let b = token("b");
    let tokens = vec![token_data(a.clone()), token_data(b.clone())];
    let inputs = make_inputs(&[a, b], HashSet::new());
    let total = BigDecimal::from(ONE_NEAR_YOCTO);
    // weights の長さ (1) が tokens の長さ (2) と一致しない。
    let _ = compute_cost_deductions(&[0.5], &tokens, &inputs, &total);
}

// ---------------------------------------------------------------------------
// (c) total_value=0 ケース（全 token 脱落の代表シナリオ）
// ---------------------------------------------------------------------------

#[test]
fn test_compute_cost_deductions_total_value_zero_drops_all_tokens() {
    // total_value=0 → assumed_in = 0 × weight = 0 で全 token が
    // ZeroPosition により estimation_failures に合流する。
    // これは「全 token が脱落するため Hold に倒れる」病理パスの入口で、
    // run_one_iteration が None を返す前提条件を担保する。
    let a = token("a");
    let b = token("b");
    let tokens = vec![token_data(a.clone()), token_data(b.clone())];
    let inputs = make_inputs(&[a.clone(), b.clone()], HashSet::new());
    let total = BigDecimal::from(0);
    let result = compute_cost_deductions(&[0.5, 0.5], &tokens, &inputs, &total);
    assert!(result.deductions.is_empty());
    assert_eq!(result.estimation_failures, vec![a, b]);
}

#[test]
fn test_compute_cost_deductions_all_zero_weights_drop_all_tokens() {
    // 全 weight=0 でも assumed_in=0 → ZeroPosition が同じく全 token を
    // estimation_failures に合流させる（total=0 と等価な経路）。
    let a = token("a");
    let b = token("b");
    let tokens = vec![token_data(a.clone()), token_data(b.clone())];
    let inputs = make_inputs(&[a.clone(), b.clone()], HashSet::new());
    let total = BigDecimal::from(ONE_NEAR_YOCTO);
    let result = compute_cost_deductions(&[0.0, 0.0], &tokens, &inputs, &total);
    assert!(result.deductions.is_empty());
    assert_eq!(result.estimation_failures, vec![a, b]);
}

// ---------------------------------------------------------------------------
// 空入力 / defense-in-depth スキップ
// ---------------------------------------------------------------------------

#[test]
fn test_compute_cost_deductions_empty_tokens_returns_empty_result() {
    let inputs = make_inputs(&[], HashSet::new());
    let total = BigDecimal::from(ONE_NEAR_YOCTO);
    let result = compute_cost_deductions(&[], &[], &inputs, &total);
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
    let result = compute_cost_deductions(&[0.5, 0.5], &tokens, &inputs, &total);
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
    let result = compute_cost_deductions(&[0.5, 0.5], &tokens, &inputs, &total);
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
    // 有限 weight + 揃った path/rate + 非ゼロ total で deduction が得られる。
    // 値は EXPECTED_SLIPPAGE_DEDUCTION (variable) + storage/gas (fixed) の和で、
    // CostDeduction::new の不変条件 (`is_finite() && >= 0.0`) を満たす。
    let sym = token("normal");
    let tokens = vec![token_data(sym.clone())];
    let inputs = make_inputs(std::slice::from_ref(&sym), HashSet::new());
    let total = BigDecimal::from(ONE_NEAR_YOCTO);
    let result = compute_cost_deductions(&[1.0], &tokens, &inputs, &total);
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
    let r_no_dep = compute_cost_deductions(&[1.0], &tokens, &inputs_no_dep, &total);
    let v_no_dep = *r_no_dep
        .deductions
        .get(&sym)
        .expect("non-deposit token must succeed");

    // case B: deposit あり → storage 固定費 0 → variable + gas のみ
    let mut deposits = HashSet::new();
    deposits.insert(TokenAccount::from(sym.clone()));
    let inputs_with_dep = make_inputs(std::slice::from_ref(&sym), deposits);
    let r_with_dep = compute_cost_deductions(&[1.0], &tokens, &inputs_with_dep, &total);
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
