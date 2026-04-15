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

/// storage 管理計画の型。`plan()` の戻り値は必ずこの enum のいずれかの variant になる。
///
/// variant ごとに異なる処理経路と cap 検証契約を持つ:
/// - [`Plan::InitialRegister`]: deposits が空。`ensure_ref_storage_setup` は
///   cap 検証を通らずに `register_tokens` を直接呼ぶ。安全性は
///   `register_tokens` の attached_deposit = 1 yocto のみに依存し、cap の代数的
///   不変条件（`initial_deposit + actual_top_up ≤ max_top_up`）は `actual_top_up = 0`
///   で型レベルに閉じ込められている。
/// - [`Plan::Normal`]: deposits がある通常運用。unregister → cap 再評価 → top-up →
///   register の順で処理される。`Normal::to_register` を `register_tokens` に渡す
///   前に `storage.rs` ステップ 4-5 の cap 検証を必ず通す必要がある（順序不変条件）。
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Plan {
    /// 初回登録ケース: deposits が空 (`is_empty()`)。
    ///
    /// 呼び出し側は cap 検証と top-up 計算をスキップし、`register_tokens` を直接
    /// 発行する。このパスで storage 資金が動かないことは
    /// `register_tokens` の仕様（attached_deposit = 1 yocto）に依存する。
    InitialRegister {
        /// 登録すべきトークン。deposits が空なので requested の全量と等しい。
        to_register: Vec<TokenAccount>,
    },
    /// 通常運用ケース: deposits がある。
    Normal {
        /// ゼロ残高かつ keep に含まれない既存登録で、解除してよいトークン。
        to_unregister: Vec<TokenAccount>,
        /// まだ登録されていない requested トークン。
        ///
        /// **不変条件**: このフィールドを使って `register_tokens` を呼ぶ前に
        /// `storage.rs` ステップ 4-5 の cap 検証
        /// (`actual_top_up > remaining_cap` → `Err`) を必ず通す必要がある。
        /// 順序を破ると `max_top_up` cap 超過を許すことになる。
        to_register: Vec<TokenAccount>,
        /// 新規トークン登録に必要な storage 総量の見積もり（安全マージン 1.1 倍を適用済み）。
        ///
        /// 命名の意図: この値は `plan()` 実行時点の snapshot（すなわち unregister を実行する
        /// *前*）から算出された `per_token × to_register.len() × 1.1` の stale 見積もりである
        /// ことをフィールド名で示している。unregister で `deposits_len` が減少すると実際の
        /// per_token はわずかに上昇するが、それはここには反映されない。
        ///
        /// 実際の top-up 額は `ensure_ref_storage_setup` 側（`storage.rs` ステップ 4）で
        /// `balance_of` を再取得した `post_unregister_available` を用いて
        /// `pre_unregister_estimate.saturating_sub(NearToken::from_yoctonear(post_unregister_available))`
        /// として補正されるため、ここで stale なままでも最終的な整合性は保たれる。詳細は
        /// `storage.rs` のステップ 4 コメントを参照。
        pre_unregister_estimate: NearToken,
    },
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
/// 返り値は [`Plan`] の variant で表される。詳細な契約は各 variant の doc を参照。
/// - deposits が空 → [`Plan::InitialRegister`]
/// - deposits がある → [`Plan::Normal`]
pub(super) fn plan(
    snapshot: &StorageSnapshot,
    requested: &[TokenAccount],
    keep: &[TokenAccount],
) -> Result<Plan, PlanError> {
    let deposits = &snapshot.deposits;

    // deposits が空の場合は per_token を算出できない。
    // 呼び出し側は cap 検証をスキップして register_tokens 直接発行する。
    let Some(deposits_len) = NonZeroUsize::new(deposits.len()) else {
        // deposits 空 → 未登録 token と requested が 1:1 で一致するため、filter は no-op。
        // それでも MAX_REGISTER_PER_CYCLE ガードは通すことで planner 経路間の対称性を保つ。
        if requested.len() > MAX_REGISTER_PER_CYCLE {
            return Err(PlanError::TooManyTokens {
                requested: requested.len(),
                max: MAX_REGISTER_PER_CYCLE,
            });
        }
        return Ok(Plan::InitialRegister {
            to_register: requested.to_vec(),
        });
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
    //
    // **非対称性メモ**: ここでは `to_register.len()`（= requested − 既登録分）を見る。
    // 一方 `storage.rs` 先頭の同名チェックは `needed_tokens.len()`（= raw requested 数）
    // を見る。両者の違いは:
    //   - planner 側（本箇所、filtered count）は「実際に register_tokens で投入する token 数」
    //     の意味論的境界を表す。未登録分のみを見るため、再実行でも超過しない。
    //   - storage 側（raw count）は planner をスキップする `None` 経路（deposits 空）も
    //     含めて全呼び出しで効くサニティガード。filter 前でも明らかに過剰な requested
    //     を入口で弾くためのもの。
    // どちらが先に発火するかはパスに依存するが、ガード総量として冗長 (defence-in-depth)。
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
        return Ok(Plan::Normal {
            to_unregister: vec![],
            to_register,
            pre_unregister_estimate: NearToken::from_yoctonear(needed_u128),
        });
    }

    // needed_u128 > available は直前の L186 early-return で保証される local invariant。
    // 到達時点で shortage > 0 が確定しているため panic ブランチは意味論的に dead。
    let shortage = needed_u128
        .checked_sub(available)
        .expect("needed_u128 > available ensured by early-return branch above");

    // ゼロ残高かつ keep に含まれない既存登録 → 解除候補
    let mut unregister_candidates: Vec<TokenAccount> = deposits
        .iter()
        .filter(|(token, amount)| {
            amount.0 == 0 && !keep.contains(token) && !requested.contains(token)
        })
        .map(|(token, _)| token.clone())
        .collect();

    // Required unregister count = ceil(shortage / per_token).
    // `per_token > 0` is guaranteed by the L230 early-return: if per_token == 0 then
    // needed_raw = per_token * to_register.len() = 0 so needed_u128 = 0 ≤ available
    // and we would have returned already. Hence we can divide without the zero guard.
    // `usize::try_from` saturates to `usize::MAX` if the u128 quotient overflows;
    // the subsequent `truncate` then caps it at the candidate count (fail-safe).
    let unregister_needed = usize::try_from(shortage.div_ceil(per_token)).unwrap_or(usize::MAX);

    // 候補を必要数まで切り詰め
    unregister_candidates.truncate(unregister_needed);

    Ok(Plan::Normal {
        to_unregister: unregister_candidates,
        to_register,
        pre_unregister_estimate: NearToken::from_yoctonear(needed_u128),
    })
}

#[cfg(test)]
mod tests;
