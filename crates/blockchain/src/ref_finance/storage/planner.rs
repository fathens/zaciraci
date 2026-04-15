use std::collections::BTreeMap;
use std::num::NonZeroUsize;

use common::types::TokenAccount;
use near_sdk::NearToken;
use near_sdk::json_types::U128;
use thiserror::Error;

use super::{StorageBalance, StorageBalanceBounds};

/// storage 見積もりの安全マージン。per_token × 必要枠に掛ける係数 (1.1 = 10% 余裕)。
///
/// per_token は切り上げ除算による推定値であり、コントラクト内部の実コストとの乖離を
/// 吸収するための 10% マージン。不足時は次サイクルで balance_of を再取得し再計算される。
const SAFETY_MARGIN_NUMERATOR: u128 = 11;
const SAFETY_MARGIN_DENOMINATOR: u128 = 10;

/// 1 呼び出しで register できる新規トークン数の上限。
///
/// 値の根拠:
///   `N × min_bound × 1.1 ≤ max_top_up` の理論上限は
///   `max_top_up = 0.5 NEAR` / `min_bound = 1.25e21 yocto` で約 N=363。
///   100 は 27.5% 利用にあたり、per_token_calc が floor を発動させる過渡状態でも
///   cap に到達しない余裕を持たせた sanity guard。
///   `storage.rs` 側の `CHUNK_SIZE = 10`（unregister チャンクサイズ）とも整合する。
///
/// 真の cap 保護は `storage.rs` 側の `remaining_cap` チェックが担う。本値はその前段で
/// 明らかに過剰な同時登録量を `PlanError::TooManyTokens` として弾くためのもの。
pub(super) const MAX_REGISTER_PER_CYCLE: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StorageSnapshot {
    pub balance: StorageBalance,
    pub deposits: BTreeMap<TokenAccount, U128>,
    pub bounds: StorageBalanceBounds,
}

#[cfg(test)]
impl StorageSnapshot {
    pub(super) fn test_default() -> Self {
        Self {
            balance: StorageBalance {
                total: U128(100_000_000_000_000_000_000_000), // 0.1 NEAR
                available: U128(50_000_000_000_000_000_000_000), // 0.05 NEAR
            },
            deposits: BTreeMap::new(),
            bounds: StorageBalanceBounds {
                min: U128(1_250_000_000_000_000_000_000), // 0.00125 NEAR
                max: None,
            },
        }
    }
}

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Plan {
    pub to_unregister: Vec<TokenAccount>,
    pub to_register: Vec<TokenAccount>,
    /// 新規トークン登録に必要な storage 総量の見積もり（安全マージン 1.1 倍を適用済み）。
    ///
    /// この値は `plan()` 実行時点の snapshot（unregister *前*）から算出された
    /// `per_token × to_register.len() × 1.1` の stale な見積もり。unregister で
    /// `deposits_len` が減少すると実際の per_token はわずかに上昇するが、それは
    /// ここでは反映されない。
    ///
    /// 実際の top-up 額は `ensure_ref_storage_setup` 側（`storage.rs` ステップ 4）で
    /// `balance_of` を再取得した `new_available` を用いて
    /// `estimated_needed.saturating_sub(NearToken::from_yoctonear(new_available))` として
    /// 補正されるため、ここで stale なままでも最終的な整合性は保たれる。詳細は
    /// `storage.rs` のステップ 4 コメントを参照。
    pub estimated_needed: NearToken,
}

#[derive(Error, Debug)]
pub(super) enum PlanError {
    #[error("per_token arithmetic overflow")]
    ArithmeticOverflow,
    #[error("too many tokens to register in one cycle: requested={requested} max={max}")]
    TooManyTokens { requested: usize, max: usize },
}

/// storage 管理計画を立てる純関数。I/O を一切行わない。
///
/// - `snapshot`: 現在の storage 状態（balance, deposits, bounds）
/// - `requested`: 今回必要なトークン（register したい）
/// - `keep`: 解除してはいけないトークン（wnear + 次期候補等）
///
/// 返り値:
/// - `Ok(None)`: deposits が空（初期登録直後等）で per_token を算出できない。
///   呼び出し側は cap 検証を通らない `register_tokens` 直接呼び出しパスを選択する。
///   cap-bypass の安全性前提については呼び出し側
///   ([`super::ensure_ref_storage_setup`] の `None` arm) を参照。
/// - `Ok(Some(Plan))`: 通常計画
///   - `to_unregister`: ゼロ残高かつ keep に含まれない既存登録を解除して枠を空ける
///   - `to_register`: まだ登録されていない requested トークン
///   - `estimated_needed`: 新規登録に必要な storage 総量（安全マージン適用済み、stale 見積もり）
pub(super) fn plan(
    snapshot: &StorageSnapshot,
    requested: &[TokenAccount],
    keep: &[TokenAccount],
) -> Result<Option<Plan>, PlanError> {
    let deposits = &snapshot.deposits;

    // deposits が空の場合は per_token を算出できない
    // (初回登録時は ensure 側で initial deposit → register_tokens の別パスを通る)
    let Some(deposits_len) = NonZeroUsize::new(deposits.len()) else {
        return Ok(None);
    };

    let total = snapshot.balance.total.0;
    let available = snapshot.balance.available.0;
    let min_bound = snapshot.bounds.min.0;

    // used = total - available (型安全: total >= available は NEP-145 の不変条件)
    let used = total
        .checked_sub(available)
        .ok_or(PlanError::ArithmeticOverflow)?;

    // usable = used - min_bound (min_bound はアカウント登録自体のコスト)
    // used < min_bound の場合は per_token = 0 として扱う（全枠が min_bound 以内に収まっている）。
    //
    // Self-healing フロー:
    //   per_token = 0 → needed = 0 → ensure 側で register_tokens を試行
    //   → storage 不足ならコントラクトが拒否（資金損失なし）
    //   → 呼び出し元に Err が返る
    //   → 次サイクルでは deposits が増加し per_token > 0 に回復
    //
    // 初期登録直後の極初期状態（used ≈ min_bound）でのみ発生する過渡状態であり、
    // 定常運用では現れない。楽観的なゼロコスト扱いを維持することで、
    // self-healing ループが正しく機能する。
    let usable = used.saturating_sub(min_bound);

    // per_token = usable / deposits_len (切り上げ除算で過小評価を防ぐ)
    //
    // 初期登録直後の過渡状態では used ≈ min_bound となり usable = 0 → per_token = 0 と
    // なる。このままだと needed = 0 で register_tokens を試行し、contract 側で storage
    // 不足として拒否されてガスを浪費する（次サイクルで自然回復するが stall の間は無駄）。
    //
    // `min_bound` は本来「アカウント登録 1 件分の最小コスト」であり per-token の下限として
    // 意味論的には厳密ではないが、この過渡状態で契約要求の最小コストにまで持ち上げる
    // 保守的下限として流用する。定常運用（deposits 10+ 個）では per_token_calc >>
    // min_bound なので floor は発動せず、以後この流用は自然失効する。
    //
    // cap 整合チェック式: `N × bounds.min × 1.1 ≤ max_top_up`
    //   現行設定 (`max_top_up=0.5 NEAR`, `bounds.min=1.25e21`):
    //     理論上限 `N_max ≈ 363`。`MAX_REGISTER_PER_CYCLE=100` で安全マージン約 3.6x。
    //   `bounds.min` が 3.6x 以上増加した場合（REF 契約 upgrade 等）、
    //     `MAX_REGISTER_PER_CYCLE` と `max_top_up` の再評価が必要。
    //   倍率と N_max の対応:
    //     | bounds.min 倍率 | N_max | MAX=100 運用可否       |
    //     | 1x (現行)       |  363  | ✓ 3.63x 余裕           |
    //     | 3x              |  121  | ⚠ 1.21x 余裕薄い       |
    //     | 3.6x            |  100  | ⚠ 余裕ゼロ             |
    //     | 4x              |   90  | ✗ 要再評価             |
    let per_token_calc = usable.div_ceil(deposits_len.get() as u128);
    let per_token = per_token_calc.max(min_bound);

    // 新規登録が必要なトークン
    let to_register: Vec<TokenAccount> = requested
        .iter()
        .filter(|token| !deposits.contains_key(*token))
        .cloned()
        .collect();

    // sanity guard: 同時登録トークン数が上限を超える場合は即エラー（累積 cap 保護の前段）。
    // `needed_raw` 計算より前に判定することで `ArithmeticOverflow` より明確な診断を返す。
    if to_register.len() > MAX_REGISTER_PER_CYCLE {
        return Err(PlanError::TooManyTokens {
            requested: to_register.len(),
            max: MAX_REGISTER_PER_CYCLE,
        });
    }

    // 新規登録に必要な storage
    let needed_raw = per_token
        .checked_mul(to_register.len() as u128)
        .ok_or(PlanError::ArithmeticOverflow)?;

    // 安全マージン適用 (1.1x)
    let needed_u128 = needed_raw
        .checked_mul(SAFETY_MARGIN_NUMERATOR)
        .ok_or(PlanError::ArithmeticOverflow)?
        .div_ceil(SAFETY_MARGIN_DENOMINATOR);

    if needed_u128 <= available {
        // 余裕あり: unregister も top-up も不要
        return Ok(Some(Plan {
            to_unregister: vec![],
            to_register,
            estimated_needed: NearToken::from_yoctonear(needed_u128),
        }));
    }

    let shortage = needed_u128
        .checked_sub(available)
        .ok_or(PlanError::ArithmeticOverflow)?;

    // ゼロ残高かつ keep に含まれない既存登録 → 解除候補
    let mut unregister_candidates: Vec<TokenAccount> = deposits
        .iter()
        .filter(|(token, amount)| {
            amount.0 == 0 && !keep.contains(token) && !requested.contains(token)
        })
        .map(|(token, _)| token.clone())
        .collect();

    // 必要な解除数 = shortage / per_token (切り上げ)
    // u128 → usize 変換が溢れる場合は usize::MAX にサチュレーション。
    // 結果は truncate で候補数に制限されるため、全候補解除となり安全側に倒れる。
    let unregister_needed = if per_token > 0 {
        usize::try_from(shortage.div_ceil(per_token)).unwrap_or(usize::MAX)
    } else {
        0
    };

    // 候補を必要数まで切り詰め
    unregister_candidates.truncate(unregister_needed);

    Ok(Some(Plan {
        to_unregister: unregister_candidates,
        to_register,
        estimated_needed: NearToken::from_yoctonear(needed_u128),
    }))
}

#[cfg(test)]
mod tests;
