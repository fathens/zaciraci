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
    let result = plan(&snap, &[token("c.near")], &[token("wrap.near")]);
    let p = result.unwrap();

    println!("plan: {:#?}", p);
    assert!(p.to_unregister.is_empty());
    assert_eq!(p.to_register, vec![token("c.near")]);
    assert!(p.needed.as_yoctonear() <= 80_000);
}

#[test]
fn plan_already_registered() {
    // requested が既に deposits にある → to_register は空
    let snap = snapshot_with_deposits(100_000, 80_000, 1_000, &[("a.near", 100), ("b.near", 200)]);
    let result = plan(&snap, &[token("a.near")], &[token("wrap.near")]);
    let p = result.unwrap();

    assert!(p.to_unregister.is_empty());
    assert!(p.to_register.is_empty());
    assert_eq!(p.needed.as_yoctonear(), 0);
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
    let result = plan(&snap, &[token("new.near")], &[token("wrap.near")]);
    let p = result.unwrap();

    println!("plan: {:#?}", p);
    assert!(!p.to_unregister.is_empty());
    assert_eq!(p.to_register, vec![token("new.near")]);
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
    let result = plan(&snap, &[token("new.near")], &[token("wrap.near")]);
    let p = result.unwrap();

    println!("plan: {:#?}", p);
    assert!(p.to_unregister.is_empty());
    assert_eq!(p.to_register, vec![token("new.near")]);
    assert!(p.needed.as_yoctonear() > 100);
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
    let result = plan(&snap, &[token("new.near")], &[token("wrap.near")]);
    let p = result.unwrap();

    // wrap.near は to_unregister に含まれない
    assert!(!p.to_unregister.contains(&token("wrap.near")));
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
    let result = plan(
        &snap,
        &[token("target.near"), token("new.near")],
        &[token("wrap.near")],
    );
    let p = result.unwrap();

    // target.near は解除されず、to_register にも入らない（既に登録済み）
    assert!(!p.to_unregister.contains(&token("target.near")));
    assert!(!p.to_register.contains(&token("target.near")));
    // new.near は to_register に入る
    assert!(p.to_register.contains(&token("new.near")));
}

// --- 境界値 ---

#[test]
fn plan_empty_deposits_error() {
    let snap = StorageSnapshot {
        deposits: BTreeMap::new(),
        ..StorageSnapshot::test_default()
    };
    let err = plan(&snap, &[token("a.near")], &[]).unwrap_err();
    println!("error: {}", err);
    assert!(matches!(err, PlanError::EmptyDeposits));
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
    let result = plan(&snap, &[token("b.near")], &[]);
    let p = result.unwrap();

    // needed=1100 > available=1000 → shortage=100, unregister候補なし → top-up必要
    assert_eq!(p.to_register, vec![token("b.near")]);
    assert_eq!(p.needed.as_yoctonear(), 1100);
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
    let result = plan(&snap, &[token("b.near")], &[]);
    let p = result.unwrap();

    // per_token_floor により needed ≈ 1100 で available=1500 に収まる
    assert!(p.needed.as_yoctonear() <= 1500);
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
    let result = plan(&snap, &[token("b.near"), token("c.near")], &[]);
    let p = result.unwrap();

    // per_token_floor = bounds.min.0 = 1000
    // needed_raw = 1000 * 2 = 2000
    // needed = 2000 * 11 / 10 = 2200
    assert_eq!(p.needed.as_yoctonear(), 2200);
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
    let result = plan(&snap, &[token("b.near")], &[]);
    let p = result.unwrap();

    assert_eq!(p.needed.as_yoctonear(), 1100);
    assert!(p.to_unregister.is_empty());
}

#[test]
fn plan_no_requested() {
    // requested が空 → to_register 空、needed = 0
    let snap = snapshot_with_deposits(100_000, 100, 1_000, &[("stale.near", 0)]);
    let result = plan(&snap, &[], &[]);
    let p = result.unwrap();

    assert!(p.to_register.is_empty());
    assert!(p.to_unregister.is_empty());
    assert_eq!(p.needed.as_yoctonear(), 0);
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
    let result = plan(
        &snap,
        &[token("new1.near"), token("new2.near")],
        &[token("wrap.near")],
    );
    let p = result.unwrap();

    println!("plan: {:#?}", p);
    // 解除候補が 1 つだけなので 1 件解除
    assert_eq!(p.to_unregister, vec![token("stale.near")]);
    // 2 トークン新規登録
    assert_eq!(p.to_register.len(), 2);
    // needed は available より大きい
    assert!(p.needed.as_yoctonear() > 100);
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
