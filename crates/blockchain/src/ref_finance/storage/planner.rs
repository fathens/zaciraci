use std::collections::HashMap;
use std::num::NonZeroUsize;

use common::types::TokenAccount;
use near_sdk::NearToken;
use near_sdk::json_types::U128;
use thiserror::Error;

use super::{StorageBalance, StorageBalanceBounds};

/// storage 見積もりの安全マージン。per_token × 必要枠に掛ける係数 (1.1 = 10% 余裕)。
const SAFETY_MARGIN_NUMERATOR: u128 = 11;
const SAFETY_MARGIN_DENOMINATOR: u128 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StorageSnapshot {
    pub balance: StorageBalance,
    pub deposits: HashMap<TokenAccount, U128>,
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
            deposits: HashMap::new(),
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
    pub top_up: NearToken,
    /// 新規トークン登録に必要な storage 総量（安全マージン適用済み）。
    /// unregister 後に balance_of を再取得して `needed.saturating_sub(new_available)` で
    /// 実際の top-up 額を再計算する際に使用する。
    pub needed: u128,
}

#[derive(Error, Debug)]
pub(super) enum PlanError {
    #[error("no deposits registered")]
    EmptyDeposits,
    #[error("per_token arithmetic overflow")]
    ArithmeticOverflow,
}

/// storage 管理計画を立てる純関数。I/O を一切行わない。
///
/// - `snapshot`: 現在の storage 状態（balance, deposits, bounds）
/// - `requested`: 今回必要なトークン（register したい）
/// - `keep`: 解除してはいけないトークン（wnear + 次期候補等）
///
/// 返り値の `Plan`:
/// - `to_unregister`: ゼロ残高かつ keep に含まれない既存登録を解除して枠を空ける
/// - `to_register`: まだ登録されていない requested トークン
/// - `top_up`: unregister だけでは足りない場合の追加 storage_deposit 額
pub(super) fn plan(
    snapshot: &StorageSnapshot,
    requested: &[TokenAccount],
    keep: &[TokenAccount],
) -> Result<Plan, PlanError> {
    let deposits = &snapshot.deposits;

    // deposits が空の場合は per_token を算出できない
    // (初回登録時は ensure 側で initial deposit → register_tokens の別パスを通る)
    let deposits_len = NonZeroUsize::new(deposits.len()).ok_or(PlanError::EmptyDeposits)?;

    let total = snapshot.balance.total.0;
    let available = snapshot.balance.available.0;
    let min_bound = snapshot.bounds.min.0;

    // used = total - available (型安全: total >= available は NEP-145 の不変条件)
    let used = total
        .checked_sub(available)
        .ok_or(PlanError::ArithmeticOverflow)?;

    // usable = used - min_bound (min_bound はアカウント登録自体のコスト)
    // used < min_bound の場合は per_token = 0 として扱う（全枠が min_bound 以内に収まっている）
    let usable = used.saturating_sub(min_bound);

    // per_token = usable / deposits_len (切り上げ除算で過小評価を防ぐ)
    let per_token = usable.div_ceil(deposits_len.get() as u128);

    // 新規登録が必要なトークン
    let to_register: Vec<TokenAccount> = requested
        .iter()
        .filter(|token| !deposits.contains_key(*token))
        .cloned()
        .collect();

    // 新規登録に必要な storage
    let needed_raw = per_token
        .checked_mul(to_register.len() as u128)
        .ok_or(PlanError::ArithmeticOverflow)?;

    // 安全マージン適用 (1.1x)
    let needed = needed_raw
        .checked_mul(SAFETY_MARGIN_NUMERATOR)
        .ok_or(PlanError::ArithmeticOverflow)?
        / SAFETY_MARGIN_DENOMINATOR;

    if needed <= available {
        // 余裕あり: unregister も top-up も不要
        return Ok(Plan {
            to_unregister: vec![],
            to_register,
            top_up: NearToken::from_yoctonear(0),
            needed,
        });
    }

    let shortage = needed
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
    let unregister_needed = if per_token > 0 {
        usize::try_from(shortage.div_ceil(per_token)).unwrap_or(usize::MAX)
    } else {
        0
    };

    // 候補を必要数まで切り詰め
    unregister_candidates.truncate(unregister_needed);

    // unregister で回収できる storage
    let recovered = per_token
        .checked_mul(unregister_candidates.len() as u128)
        .ok_or(PlanError::ArithmeticOverflow)?;

    // まだ足りない分を top-up
    let remaining_shortage = shortage.saturating_sub(recovered);
    let top_up = NearToken::from_yoctonear(remaining_shortage);

    Ok(Plan {
        to_unregister: unregister_candidates,
        to_register,
        top_up,
        needed,
    })
}

#[cfg(test)]
mod tests;
