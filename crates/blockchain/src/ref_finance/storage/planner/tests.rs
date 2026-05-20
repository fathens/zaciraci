use super::*;
use near_sdk::json_types::U128;

fn token(name: &str) -> TokenAccount {
    name.parse().unwrap()
}

fn yocto(v: u128) -> U128 {
    U128(v)
}

fn snapshot_with_deposits(
    total: u128,
    available: u128,
    min_bound: u128,
    deposits: &[(&str, u128)],
) -> StorageSnapshot {
    StorageSnapshot {
        balance: StorageBalance {
            total: yocto(total),
            available: yocto(available),
        },
        deposits: deposits
            .iter()
            .map(|(name, amount)| (token(name), yocto(*amount)))
            .collect(),
        bounds: StorageBalanceBounds {
            min: yocto(min_bound),
            max: None,
        },
    }
}

/// `Plan::Normal` variant を取り出すテスト用ヘルパ。`InitialRegister` が
/// 返ってきた場合は panic する。
fn unwrap_normal(plan: Plan) -> (Vec<TokenAccount>, Vec<TokenAccount>, NearToken) {
    match plan {
        Plan::Normal {
            to_unregister,
            to_register,
            pre_unregister_estimate,
        } => (to_unregister, to_register, pre_unregister_estimate),
        Plan::InitialRegister { to_register } => {
            panic!("expected Plan::Normal, got InitialRegister {{ to_register: {to_register:?} }}")
        }
    }
}

// --- 正常系 ---

#[test]
fn plan_sufficient_available() {
    // available が十分あるので unregister も top-up も不要
    let snap = snapshot_with_deposits(
        100_000, // total
        80_000,  // available
        1_000,   // min
        &[("a.near", 100), ("b.near", 200)],
    );
    let (to_unregister, to_register, estimate) =
        unwrap_normal(plan(&snap, &[token("c.near")], &[token("wrap.near")]).unwrap());

    assert!(to_unregister.is_empty());
    assert_eq!(to_register, vec![token("c.near")]);
    assert!(estimate.as_yoctonear() <= 80_000);
}

#[test]
fn plan_already_registered() {
    // requested が既に deposits にある → to_register は空
    let snap = snapshot_with_deposits(100_000, 80_000, 1_000, &[("a.near", 100), ("b.near", 200)]);
    let (to_unregister, to_register, estimate) =
        unwrap_normal(plan(&snap, &[token("a.near")], &[token("wrap.near")]).unwrap());

    assert!(to_unregister.is_empty());
    assert!(to_register.is_empty());
    assert_eq!(estimate.as_yoctonear(), 0);
}

#[test]
fn plan_unregister_to_make_room() {
    // available が少なく、ゼロ残高の不要トークンを解除して枠を確保
    let snap = snapshot_with_deposits(
        100_000,
        100, // ほぼ空き無し
        1_000,
        &[
            ("a.near", 100),
            ("stale1.near", 0), // 解除候補
            ("stale2.near", 0), // 解除候補
        ],
    );
    let (to_unregister, to_register, _estimate) =
        unwrap_normal(plan(&snap, &[token("new.near")], &[token("wrap.near")]).unwrap());

    assert!(!to_unregister.is_empty());
    assert_eq!(to_register, vec![token("new.near")]);
}

#[test]
fn plan_needed_exceeds_available() {
    // 解除候補がないため追加 storage が必要（needed > available）
    let snap = snapshot_with_deposits(
        100_000,
        100, // ほぼ空き無し
        1_000,
        &[("a.near", 100), ("b.near", 200)],
    );
    let (to_unregister, to_register, estimate) =
        unwrap_normal(plan(&snap, &[token("new.near")], &[token("wrap.near")]).unwrap());

    assert!(to_unregister.is_empty());
    assert_eq!(to_register, vec![token("new.near")]);
    assert!(estimate.as_yoctonear() > 100);
}

#[test]
fn plan_keep_is_preserved() {
    // keep に含まれるゼロ残高トークンは解除しない
    let snap = snapshot_with_deposits(
        100_000,
        100,
        1_000,
        &[
            ("wrap.near", 0),  // keep に含まれる → 解除しない
            ("stale.near", 0), // keep に含まれない → 解除候補
        ],
    );
    let (to_unregister, _to_register, _estimate) =
        unwrap_normal(plan(&snap, &[token("new.near")], &[token("wrap.near")]).unwrap());

    // wrap.near は to_unregister に含まれない
    assert!(!to_unregister.contains(&token("wrap.near")));
}

#[test]
fn plan_requested_not_unregistered() {
    // requested に含まれるゼロ残高トークンは解除しない（まさに登録しようとしているので）
    let snap = snapshot_with_deposits(
        100_000,
        100,
        1_000,
        &[
            ("target.near", 0), // requested に含まれる → 解除しない
            ("stale.near", 0),
        ],
    );
    let (to_unregister, to_register, _estimate) = unwrap_normal(
        plan(
            &snap,
            &[token("target.near"), token("new.near")],
            &[token("wrap.near")],
        )
        .unwrap(),
    );

    // target.near は解除されず、to_register にも入らない（既に登録済み）
    assert!(!to_unregister.contains(&token("target.near")));
    assert!(!to_register.contains(&token("target.near")));
    // new.near は to_register に入る
    assert!(to_register.contains(&token("new.near")));
}

// --- 境界値 ---

#[test]
fn plan_empty_deposits_returns_initial_register() {
    let snap = StorageSnapshot {
        deposits: BTreeMap::new(),
        ..StorageSnapshot::test_default()
    };
    let result = plan(&snap, &[token("a.near")], &[]).unwrap();
    match result {
        Plan::InitialRegister { to_register } => {
            assert_eq!(to_register, vec![token("a.near")]);
        }
        other => panic!("expected InitialRegister, got {other:?}"),
    }
}

#[test]
fn plan_empty_deposits_too_many_tokens() {
    // deposits が空のパスでも MAX_REGISTER_PER_CYCLE ガードは効く。
    let snap = StorageSnapshot {
        deposits: BTreeMap::new(),
        ..StorageSnapshot::test_default()
    };
    let requested: Vec<TokenAccount> = (0..=MAX_REGISTER_PER_CYCLE)
        .map(|i| token(&format!("new{i}.near")))
        .collect();
    let err = plan(&snap, &requested, &[]).unwrap_err();
    match err {
        PlanError::TooManyTokens { requested, max } => {
            assert_eq!(requested, MAX_REGISTER_PER_CYCLE + 1);
            assert_eq!(max, MAX_REGISTER_PER_CYCLE);
        }
        other => panic!("expected TooManyTokens, got {other:?}"),
    }
}

#[test]
fn plan_used_equals_min() {
    // used == min → usable = 0 → per_token_floor により per_token = min
    // needed = 1 * 1000 * 11/10 = 1100
    let snap = snapshot_with_deposits(
        2_000, // total
        1_000, // available = 1000, used = 1000
        1_000, // min = 1000 → usable = 0 → floor 発動
        &[("a.near", 100)],
    );
    let (_to_unregister, to_register, estimate) =
        unwrap_normal(plan(&snap, &[token("b.near")], &[]).unwrap());

    // needed=1100 > available=1000 → shortage=100, unregister候補なし → top-up必要
    assert_eq!(to_register, vec![token("b.near")]);
    assert_eq!(estimate.as_yoctonear(), 1100);
}

#[test]
fn plan_used_less_than_min() {
    // used < min → saturating_sub → usable = 0
    // per_token_floor により per_token = bounds.min = 1000
    // needed = 1 * 1000 * 11/10 = 1100 > available=1500 ... いや 1100 < 1500 なので足りる
    let snap = snapshot_with_deposits(
        2_000, // total
        1_500, // available = 1500, used = 500
        1_000, // min = 1000 > used → usable = 0, per_token = floor 1000
        &[("a.near", 100)],
    );
    let (_to_unregister, _to_register, estimate) =
        unwrap_normal(plan(&snap, &[token("b.near")], &[]).unwrap());

    // per_token_floor により needed ≈ 1100 で available=1500 に収まる
    assert!(estimate.as_yoctonear() <= 1500);
}

#[test]
fn plan_needed_raw_zero_no_margin_overflow() {
    // requested が空なら to_register.len() == 0 → needed_raw = per_token * 0 = 0
    // needed = 0 * 11 / 10 = 0 で overflow せず正常に 0 を返す境界ケース。
    let snap = snapshot_with_deposits(
        u128::MAX / 2, // per_token が大きくても
        0,
        0, // min=0 → usable = used = u128::MAX/2
        &[("a.near", 100)],
    );
    let (to_unregister, to_register, estimate) = unwrap_normal(plan(&snap, &[], &[]).unwrap());
    assert_eq!(estimate.as_yoctonear(), 0);
    assert!(to_register.is_empty());
    assert!(to_unregister.is_empty());
}

#[test]
fn plan_per_token_floor_applied() {
    // used <= min → usable = 0 → per_token_calc = 0 → floor 発動で per_token = min
    // needed = floor * to_register.len() * 11/10
    let snap = snapshot_with_deposits(
        2_000, // total
        1_000, // available = 1000, used = 1000
        1_000, // min = 1000 → usable = 0 → floor 発動
        &[("a.near", 100)],
    );
    let (_to_unregister, _to_register, estimate) =
        unwrap_normal(plan(&snap, &[token("b.near"), token("c.near")], &[]).unwrap());

    // per_token_floor = bounds.min.0 = 1000
    // needed_raw = 1000 * 2 = 2000
    // needed = 2000 * 11 / 10 = 2200
    assert_eq!(estimate.as_yoctonear(), 2200);
}

#[test]
fn plan_total_equals_available() {
    // total == available → used = 0 → usable = 0 → per_token_floor で per_token = min
    // needed = 1 * 1000 * 11/10 = 1100 < available=10000 → 余裕あり
    let snap = snapshot_with_deposits(
        10_000,
        10_000, // available == total
        1_000,
        &[("a.near", 100)],
    );
    let (to_unregister, _to_register, estimate) =
        unwrap_normal(plan(&snap, &[token("b.near")], &[]).unwrap());

    assert_eq!(estimate.as_yoctonear(), 1100);
    assert!(to_unregister.is_empty());
}

#[test]
fn plan_no_requested() {
    // requested が空 → to_register 空、needed = 0
    let snap = snapshot_with_deposits(100_000, 100, 1_000, &[("stale.near", 0)]);
    let (to_unregister, to_register, estimate) = unwrap_normal(plan(&snap, &[], &[]).unwrap());

    assert!(to_register.is_empty());
    assert!(to_unregister.is_empty());
    assert_eq!(estimate.as_yoctonear(), 0);
}

#[test]
fn plan_needed_equals_available_takes_early_return() {
    // 境界条件 `needed_u128 == available` で早期 return 経路を pin する。
    // cap の strict `>` 境界と対称に、`<=` 境界の挙動を regression から守る。
    //
    // 計算:
    //   total=2_000, available=1_100, min=1_000, deposits=[("a.near",100)]
    //   used = total - available = 900
    //   usable = used.saturating_sub(min) = 0 → per_token_calc = 0 → floor で per_token = 1_000
    //   to_register = ["b.near"], needed_raw = 1_000 * 1 = 1_000
    //   needed_u128 = 1_000 * 11 / 10 = 1_100 == available → early return
    let snap = snapshot_with_deposits(2_000, 1_100, 1_000, &[("a.near", 100)]);
    let (to_unregister, to_register, estimate) =
        unwrap_normal(plan(&snap, &[token("b.near")], &[]).unwrap());

    assert!(
        to_unregister.is_empty(),
        "needed == available boundary: early return should skip unregister"
    );
    assert_eq!(to_register, vec![token("b.near")]);
    assert_eq!(estimate.as_yoctonear(), 1_100);
}

#[test]
fn plan_needed_exceeds_available_by_one() {
    // 境界条件 `needed_u128 == available + 1` で shortage=1 の unregister 経路を pin する。
    // `plan_needed_equals_available_takes_early_return` と対称の境界テスト。
    //
    // 計算:
    //   total=2_000, available=1_099, min=1_000
    //   deposits=[("a.near",100), ("stale.near", 0)] (stale は解除候補)
    //   used = 901, usable = 0, per_token = 1_000
    //   needed = 1_100, shortage = 1_100 - 1_099 = 1
    //   unregister_needed = ceil(1 / 1_000) = 1 → stale.near を truncate(1)
    let snap = snapshot_with_deposits(2_000, 1_099, 1_000, &[("a.near", 100), ("stale.near", 0)]);
    let (to_unregister, to_register, estimate) =
        unwrap_normal(plan(&snap, &[token("b.near")], &[]).unwrap());

    assert_eq!(to_unregister, vec![token("stale.near")]);
    assert_eq!(to_register, vec![token("b.near")]);
    assert_eq!(estimate.as_yoctonear(), 1_100);
}

// --- 複合系 ---

#[test]
fn plan_unregister_plus_needed() {
    // 解除候補が 1 つだけで、解除しても不足が残り追加 storage が必要なケース
    // total=100_000, available=100, min=1_000, deposits=[a(100), stale(0)]
    // used = 99_900, usable = 98_900, per_token = ceil(98_900/2) = 49_450
    // to_register = [new1, new2] → needed_raw = 49_450 * 2 = 98_900
    // needed = 98_900 * 11 / 10 = 108_790
    // shortage = 108_790 - 100 = 108_690
    // unregister_needed = ceil(108_690 / 49_450) = 3 → 候補は 1 つだけなので 1 件解除
    let snap = snapshot_with_deposits(
        100_000,
        100, // ほぼ空き無し
        1_000,
        &[
            ("a.near", 100),   // 残高ありなので解除不可
            ("stale.near", 0), // 解除候補（1 つだけ）
        ],
    );
    let (to_unregister, to_register, estimate) = unwrap_normal(
        plan(
            &snap,
            &[token("new1.near"), token("new2.near")],
            &[token("wrap.near")],
        )
        .unwrap(),
    );

    // 解除候補が 1 つだけなので 1 件解除
    assert_eq!(to_unregister, vec![token("stale.near")]);
    // 2 トークン新規登録
    assert_eq!(to_register.len(), 2);
    // needed は available より大きい
    assert!(estimate.as_yoctonear() > 100);
}

// --- エラー系 ---

#[test]
fn plan_arithmetic_overflow_total_less_than_available() {
    // total < available は NEP-145 違反だが、checked_sub で ArithmeticOverflow
    let snap = snapshot_with_deposits(
        1_000,
        2_000, // available > total
        100,
        &[("a.near", 100)],
    );
    let err = plan(&snap, &[token("b.near")], &[]).unwrap_err();
    assert!(matches!(err, PlanError::ArithmeticOverflow));
}

#[test]
fn plan_arithmetic_overflow_needed_raw_multiplication() {
    // per_token * to_register.len() が u128::MAX を超える場合
    // per_token = u128::MAX / 2 + 1 で、to_register が 2 つあればオーバーフロー
    let big_per_token = u128::MAX / 2 + 1;
    let total = big_per_token + 1_000;
    let snap = snapshot_with_deposits(
        total,
        1_000,              // available = 1_000, used = total - 1_000
        0,                  // min_bound = 0 → usable = used
        &[("a.near", 100)], // deposits_len = 1 → per_token ≈ big_per_token
    );
    // 2 トークン登録 → per_token * 2 がオーバーフロー
    let err = plan(&snap, &[token("b.near"), token("c.near")], &[]).unwrap_err();
    assert!(matches!(err, PlanError::ArithmeticOverflow));
}

#[test]
fn plan_arithmetic_overflow_safety_margin_multiplication() {
    // needed_raw * SAFETY_MARGIN_NUMERATOR が u128::MAX を超える場合
    // needed_raw = per_token * to_register.len() なので、per_token を大きくして再現
    let big_per_token = u128::MAX / 11 + 1; // * 11 でオーバーフロー
    let total = big_per_token + 1_000; // used = big_per_token, usable = big_per_token
    let snap = snapshot_with_deposits(
        total,
        1_000,              // available = 1_000, used = total - 1_000
        0,                  // min_bound = 0 → usable = used
        &[("a.near", 100)], // deposits_len = 1 → per_token = usable
    );
    let err = plan(&snap, &[token("b.near")], &[]).unwrap_err();
    assert!(matches!(err, PlanError::ArithmeticOverflow));
}

#[test]
fn plan_too_many_tokens() {
    // MAX_REGISTER_PER_CYCLE = 100 超過で TooManyTokens を返す。
    let snap = snapshot_with_deposits(10_000, 5_000, 1_000, &[("anchor.near", 100)]);
    let requested: Vec<TokenAccount> = (0..=MAX_REGISTER_PER_CYCLE)
        .map(|i| token(&format!("new{i}.near")))
        .collect();
    assert_eq!(requested.len(), MAX_REGISTER_PER_CYCLE + 1);

    let err = plan(&snap, &requested, &[]).unwrap_err();
    match err {
        PlanError::TooManyTokens { requested, max } => {
            assert_eq!(requested, MAX_REGISTER_PER_CYCLE + 1);
            assert_eq!(max, MAX_REGISTER_PER_CYCLE);
        }
        other => panic!("expected TooManyTokens, got {other:?}"),
    }
}

#[test]
fn plan_max_register_per_cycle_boundary_ok() {
    // ちょうど上限値 (100) は通る（境界値テスト）。
    let snap = snapshot_with_deposits(
        u128::MAX / 2, // per_token を大きく保てる total
        u128::MAX / 2, // available も十分
        0,
        &[("anchor.near", 100)],
    );
    let requested: Vec<TokenAccount> = (0..MAX_REGISTER_PER_CYCLE)
        .map(|i| token(&format!("new{i}.near")))
        .collect();
    assert_eq!(requested.len(), MAX_REGISTER_PER_CYCLE);

    let (_to_unregister, to_register, _estimate) =
        unwrap_normal(plan(&snap, &requested, &[]).unwrap());
    assert_eq!(to_register.len(), MAX_REGISTER_PER_CYCLE);
}

// --- cap 不変条件の regression テスト ---
//
// `per_token_floor = bounds.min` に依存するため、 bounds.min が REF 契約 upgrade で
// 増加すると MAX_REGISTER_PER_CYCLE での最悪見積もりが max_top_up を超える可能性が
// ある。以下の pinned test で不変条件が崩れたら直ちに気づけるようにする。
//
// 不変条件: `MAX_REGISTER_PER_CYCLE × bounds.min × 1.1 ≤ max_top_up`
// 現行値:
//   MAX_REGISTER_PER_CYCLE = 100
//   bounds.min            = 1.25e21 yocto (0.00125 NEAR)
//   max_top_up            = 5e23   yocto (0.5 NEAR)
//   worst_case            = 100 × 1.25e21 × 1.1 = 1.375e23 yocto

const CURRENT_MIN_BOUND: u128 = 1_250_000_000_000_000_000_000;
const CURRENT_MAX_TOP_UP: u128 = 500_000_000_000_000_000_000_000;

/// `per_token_floor` 発動時に worst-case (MAX_REGISTER_PER_CYCLE 個登録) で
/// 見積もられる needed 値が `max_top_up` を超えないことを固定値で verify する。
/// 値がずれたら cap 再評価の invariant (planner.rs 冒頭のテーブル参照) が崩れた
/// サインなので、このテストを意図的に更新して確認する。
#[test]
fn plan_cap_invariant_holds_for_current_config() {
    let worst_case_raw = (MAX_REGISTER_PER_CYCLE as u128)
        .checked_mul(CURRENT_MIN_BOUND)
        .unwrap();
    let worst_case = worst_case_raw.checked_mul(11).unwrap().div_ceil(10);
    assert!(
        worst_case <= CURRENT_MAX_TOP_UP,
        "MAX_REGISTER_PER_CYCLE × bounds.min × 1.1 = {worst_case} が \
         max_top_up = {CURRENT_MAX_TOP_UP} を超過しました。\
         bounds.min が増加した場合は MAX_REGISTER_PER_CYCLE または max_top_up の再評価が必要です。"
    );
}

/// `per_token_floor` 発動 × MAX_REGISTER_PER_CYCLE ちょうどで、現行 bounds.min でも
/// pre_unregister_estimate が想定内であることを end-to-end で verify。
/// 具体値: per_token_floor = 1.25e21、requested = 100 個 → needed_raw = 1.25e23
/// → needed = 1.25e23 × 1.1 = 1.375e23 < max_top_up (5e23)。
#[test]
fn plan_per_token_floor_worst_case_fits_in_cap() {
    // used == min で floor 発動させる
    let total = CURRENT_MIN_BOUND + 1_000_000_000_000; // 小さな available を残して floor を狙う
    let snap = snapshot_with_deposits(
        total,
        1_000_000_000_000,
        CURRENT_MIN_BOUND,
        &[("anchor.near", 100)], // deposits_len = 1
    );
    let requested: Vec<TokenAccount> = (0..MAX_REGISTER_PER_CYCLE)
        .map(|i| token(&format!("n{i}.near")))
        .collect();
    let (_to_unregister, _to_register, estimate) =
        unwrap_normal(plan(&snap, &requested, &[]).unwrap());

    // 1.375e23 yocto ちょうどを期待
    let expected = (MAX_REGISTER_PER_CYCLE as u128 * CURRENT_MIN_BOUND)
        .checked_mul(11)
        .unwrap()
        .div_ceil(10);
    assert_eq!(estimate.as_yoctonear(), expected);
    assert!(
        estimate.as_yoctonear() <= CURRENT_MAX_TOP_UP,
        "worst-case estimate must fit in max_top_up"
    );
}

/// bounds.min が 3.6x になっても MAX_REGISTER_PER_CYCLE=100 で cap には収まる
/// ことを確認する境界テスト。このテストは 3.6x で余裕ゼロ、超えると cap 抵触に
/// なることを encode する（planner.rs:150-155 のテーブル参照）。
#[test]
fn plan_per_token_floor_boundary_at_3_6x_min_bound() {
    let min_bound_3_6x = CURRENT_MIN_BOUND * 36 / 10; // 3.6x
    let worst_case = (MAX_REGISTER_PER_CYCLE as u128)
        .checked_mul(min_bound_3_6x)
        .unwrap()
        .checked_mul(11)
        .unwrap()
        .div_ceil(10);
    // 3.6x ちょうどで worst_case が max_top_up とほぼ等しい (≤ で成立)。
    assert!(
        worst_case <= CURRENT_MAX_TOP_UP,
        "at 3.6x bounds.min, worst_case = {worst_case} should still fit in \
         max_top_up = {CURRENT_MAX_TOP_UP}"
    );
}

/// bounds.min が 3.64x 以上になると worst-case が max_top_up を超えることを
/// 明示的にテストする。この時点で MAX_REGISTER_PER_CYCLE の再評価が必須。
#[test]
fn plan_per_token_floor_exceeds_cap_above_3_64x_min_bound() {
    let min_bound_3_64x = CURRENT_MIN_BOUND * 364 / 100; // 3.64x
    let worst_case = (MAX_REGISTER_PER_CYCLE as u128)
        .checked_mul(min_bound_3_64x)
        .unwrap()
        .checked_mul(11)
        .unwrap()
        .div_ceil(10);
    assert!(
        worst_case > CURRENT_MAX_TOP_UP,
        "at 3.64x bounds.min, worst_case = {worst_case} must exceed \
         max_top_up = {CURRENT_MAX_TOP_UP} (trigger for MAX_REGISTER_PER_CYCLE reevaluation)"
    );
}

/// `per_token_floor` 発動 × ちょうど MAX 個登録 × 現行 bounds.min で、needed_raw
/// multiplication 経路が overflow しないことを verify する。u128 allowance の中で
/// まだ余裕があることの sanity check。
#[test]
fn plan_per_token_floor_times_max_register_no_overflow() {
    let product = (MAX_REGISTER_PER_CYCLE as u128).checked_mul(CURRENT_MIN_BOUND);
    assert!(product.is_some());
    let with_margin = product.unwrap().checked_mul(11);
    assert!(with_margin.is_some());
}

// SAFETY_MARGIN / per_token_floor / cap 整合の不変条件を proptest で検証する。
//
// 既存 pinned test は具体値での regression guard として機能するが、proptest は入力空間全体で
// 不変条件を確認することで、`SAFETY_MARGIN_*` や `per_token` floor の計算ロジックを触る
// 改修時に想定外ケースが漏れないよう自動検出する。proptest 依存は既に `Cargo.toml` に
// 含まれており、本モジュールで新規に導入しない。
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// `plan()` の出力に対する 3 つの不変条件を検証する:
        ///
        /// - INV-1 (SAFETY_MARGIN 上限): `Ok(Plan::Normal)` 時
        ///     `pre_unregister_estimate ≤ (per_token × N × 11).div_ceil(10)`
        ///     planner も test も同じ順序で計算するため等号で一致する。
        /// - INV-6 (per_token floor 下限): `to_register.len() > 0` 時
        ///     `pre_unregister_estimate ≥ (N × min_bound × 11) / 10`（切り捨て下限）。
        ///     `per_token ≥ min_bound` が planner の `.max(min_bound)` で保証されるため従う。
        /// - INV-cap (cap 整合): floor 活性化 (`per_token_calc ≤ min_bound`) かつ
        ///     `(N × min_bound × 11).div_ceil(10) ≤ max_top_up_budget` 成立時
        ///     `pre_unregister_estimate ≤ max_top_up_budget`。floor 非活性時は
        ///     `per_token > min_bound` により budget を越える可能性があり、
        ///     `ensure_ref_storage_setup` step 5 の `remaining_cap` チェックで弾く設計と整合。
        ///
        /// `max_top_up_budget` は input として生成し、cap 条件を満たす case のみ INV-cap を
        /// 検査する（満たさない場合は `prop_assume!` でスキップせず、INV-cap 検査を無効化）。
        #[test]
        fn plan_invariants(
            deposits_len in 1usize..=20usize,
            min_bound in 1u128..=10_000_000_000_000_000_000_000u128, // 1..=1e22
            extra_used in 0u128..=500_000_000_000_000_000_000_000u128, // 0..=0.5 NEAR
            available in 0u128..=500_000_000_000_000_000_000_000u128, // 0..=0.5 NEAR
            requested_count in 0usize..=(MAX_REGISTER_PER_CYCLE + 10),
            max_top_up_budget in 1u128..=10_000_000_000_000_000_000_000_000u128, // 1..=1e25
        ) {
            // snapshot を組み立てる。
            //   used = min_bound + extra_used（min_bound 以上の used を保証）
            //   total = used + available
            let used = min_bound.saturating_add(extra_used);
            let total = used.saturating_add(available);

            let deposits: BTreeMap<TokenAccount, U128> = (0..deposits_len)
                .map(|i| (token(&format!("d{i}.near")), U128(0)))
                .collect();

            let snap = StorageSnapshot {
                balance: StorageBalance {
                    total: U128(total),
                    available: U128(available),
                },
                deposits,
                bounds: StorageBalanceBounds {
                    min: U128(min_bound),
                    max: None,
                },
            };

            // requested は deposits と衝突しない新規トークン列にして、filter 前後で個数が
            // 変わらないようにする（= `to_register.len() == requested.len()`）。
            let requested: Vec<TokenAccount> = (0..requested_count)
                .map(|i| token(&format!("r{i}.near")))
                .collect();

            match plan(&snap, &requested, &[]) {
                Ok(Plan::Normal { to_register, pre_unregister_estimate, .. }) => {
                    let needed = pre_unregister_estimate.as_yoctonear();
                    let n = to_register.len() as u128;

                    // 期待する per_token を再計算（planner 内部の計算と一致させる）
                    let usable = used.saturating_sub(min_bound);
                    let per_token_calc = usable.div_ceil(deposits_len as u128);
                    let per_token = per_token_calc.max(min_bound);

                    // INV-1: SAFETY_MARGIN 上限
                    //   planner は `per_token * N * 11` を `checked_mul` で計算し
                    //   `div_ceil(10)` している。同じ計算順序なので結果は等号で一致する。
                    //   planner が `Ok` を返した時点で overflow なしが保証されているため、
                    //   test 側は `saturating_mul` で算式を一致させる（実際には overflow しない）。
                    let upper = per_token
                        .saturating_mul(n)
                        .saturating_mul(11)
                        .div_ceil(10);
                    prop_assert!(
                        needed <= upper,
                        "INV-1 violated: needed={needed} > upper={upper} \
                         (per_token={per_token}, n={n})"
                    );

                    // INV-6: per_token floor 下限
                    //   to_register.len() > 0 のとき、per_token >= min_bound なので
                    //   needed >= min_bound × N × 11 / 10（div_ceil の ε なし厳密下限）。
                    if n > 0 {
                        let lower = min_bound
                            .saturating_mul(n)
                            .saturating_mul(11)
                            / 10;
                        prop_assert!(
                            needed >= lower,
                            "INV-6 violated: needed={needed} < lower={lower} \
                             (min_bound={min_bound}, n={n})"
                        );
                    }

                    // INV-cap: floor 活性化 (`per_token_calc <= min_bound`) かつ
                    //   `N × min_bound × 11/10 ≤ max_top_up_budget` のとき
                    //   pre_estimate ≤ max_top_up_budget が保証される。
                    //   floor 非活性時は per_token > min_bound となり、needed が budget を
                    //   越える可能性があるため検査しない（ensure_ref_storage_setup 側の
                    //   step 5 cap check で弾く）。
                    let floor_active = per_token_calc <= min_bound;
                    if floor_active && n > 0 {
                        let cap_condition = min_bound
                            .checked_mul(n)
                            .and_then(|v| v.checked_mul(11))
                            .map(|v| v.div_ceil(10));
                        if let Some(cap_needed) = cap_condition
                            && cap_needed <= max_top_up_budget
                        {
                            prop_assert!(
                                needed <= max_top_up_budget,
                                "INV-cap violated: needed={needed} > budget={max_top_up_budget} \
                                 (floor active, cap_needed={cap_needed})"
                            );
                        }
                    }
                }
                Ok(Plan::InitialRegister { to_register }) => {
                    // deposits が空なら InitialRegister。今回は deposits_len >= 1 なので到達しない想定。
                    prop_assert!(to_register.len() <= MAX_REGISTER_PER_CYCLE);
                }
                Err(PlanError::TooManyTokens { requested, max }) => {
                    prop_assert!(requested > max);
                    prop_assert_eq!(max, MAX_REGISTER_PER_CYCLE);
                }
                Err(PlanError::ArithmeticOverflow) => {
                    // u128 算術 overflow は許容。入力範囲の上限で発生しうる。
                }
            }
        }
    }
}
