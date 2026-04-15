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
