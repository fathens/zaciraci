//! リバランスの直接スワップマッチングアルゴリズム
//!
//! 売却・購入操作を直接スワップにマッチングし、
//! 中間 wNEAR 変換を削減する最適化を提供する。

use common::types::*;
use num_bigint::ToBigInt;
use num_traits::ToPrimitive;

// --- リバランス直接スワップ最適化の型定義 ---

/// 売却操作（差分計算の出力）
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SellOperation {
    pub(crate) token: TokenAccount,
    pub(crate) near_value: NearValue,
    pub(crate) exchange_rate: ExchangeRate,
}

/// 購入操作（差分計算の出力）
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BuyOperation {
    pub(crate) token: TokenAccount,
    pub(crate) near_value: NearValue,
}

/// マッチングされた直接スワップ
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DirectSwap {
    pub(crate) sell_token: TokenAccount,
    pub(crate) buy_token: TokenAccount,
    pub(crate) near_value: NearValue,
    pub(crate) sell_exchange_rate: ExchangeRate,
}

/// マッチング結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MatchResult {
    pub(crate) direct_swaps: Vec<DirectSwap>,
    pub(crate) remaining_sells: Vec<SellOperation>,
    pub(crate) remaining_buys: Vec<BuyOperation>,
}

/// 売却・購入操作を直接スワップにマッチングする（純粋関数）
///
/// アルゴリズム:
/// 1. 売却・購入を wNEAR 価値の降順でソート
/// 2. 2ポインタで貪欲マッチング
/// 3. 未消化の操作を全て remaining に収集
///
/// 不変条件:
/// - sum(direct_swaps) + sum(remaining_sells) = sum(original_sells)
/// - sum(direct_swaps) + sum(remaining_buys) = sum(original_buys)
/// - 同一売却トークンの分割スワップの合計 ≤ 元の売却額
pub(crate) fn match_rebalance_operations(
    mut sell_operations: Vec<SellOperation>,
    mut buy_operations: Vec<BuyOperation>,
) -> MatchResult {
    if sell_operations.is_empty() || buy_operations.is_empty() {
        return MatchResult {
            direct_swaps: vec![],
            remaining_sells: sell_operations,
            remaining_buys: buy_operations,
        };
    }

    // 降順ソート（大きい金額を優先的にマッチ）
    sell_operations.sort_by(|a, b| b.near_value.cmp(&a.near_value));
    buy_operations.sort_by(|a, b| b.near_value.cmp(&a.near_value));

    let mut direct_swaps = Vec::new();

    let mut si = 0;
    let mut bi = 0;
    let mut sell_remaining = sell_operations[0].near_value.clone();
    let mut buy_remaining = buy_operations[0].near_value.clone();

    loop {
        let match_value = sell_remaining.clone().min(buy_remaining.clone());

        if match_value > NearValue::zero() {
            direct_swaps.push(DirectSwap {
                sell_token: sell_operations[si].token.clone(),
                buy_token: buy_operations[bi].token.clone(),
                near_value: match_value.clone(),
                sell_exchange_rate: sell_operations[si].exchange_rate.clone(),
            });
        }

        sell_remaining = &sell_remaining - &match_value;
        buy_remaining = &buy_remaining - &match_value;

        // 売却側が消化された場合、次の売却へ
        if sell_remaining == NearValue::zero() {
            si += 1;
            if si >= sell_operations.len() {
                // 売却を全て消化 — 購入側の残余を全て収集
                let mut remaining_buys = Vec::new();
                if buy_remaining > NearValue::zero() {
                    remaining_buys.push(BuyOperation {
                        token: buy_operations[bi].token.clone(),
                        near_value: buy_remaining,
                    });
                }
                // 現在の購入（部分消化済み or 完全消化）の次から未処理を追加
                for buy in &buy_operations[bi + 1..] {
                    remaining_buys.push(buy.clone());
                }
                return MatchResult {
                    direct_swaps,
                    remaining_sells: vec![],
                    remaining_buys,
                };
            }
            sell_remaining = sell_operations[si].near_value.clone();
        }

        // 購入側が消化された場合、次の購入へ
        if buy_remaining == NearValue::zero() {
            bi += 1;
            if bi >= buy_operations.len() {
                // 購入を全て消化 — 売却側の残余を全て収集
                let mut remaining_sells = Vec::new();
                if sell_remaining > NearValue::zero() {
                    remaining_sells.push(SellOperation {
                        token: sell_operations[si].token.clone(),
                        near_value: sell_remaining,
                        exchange_rate: sell_operations[si].exchange_rate.clone(),
                    });
                }
                // 現在の売却（部分消化済み or 完全消化）の次から未処理を追加
                for sell in &sell_operations[si + 1..] {
                    remaining_sells.push(sell.clone());
                }
                return MatchResult {
                    direct_swaps,
                    remaining_sells,
                    remaining_buys: vec![],
                };
            }
            buy_remaining = buy_operations[bi].near_value.clone();
        }
    }
}

/// TokenAmount を u128 に変換する。
///
/// 小数部がある場合は切り捨てられる（floor）。
///
/// # 前提条件
/// `amount` は非負であること。負の値を渡すと `u128` へのパースが失敗しエラーを返す。
/// 呼び出し元で `.abs()` 等により非負を保証すること。
pub(crate) fn token_amount_to_u128(amount: &TokenAmount) -> crate::Result<u128> {
    amount
        .smallest_units()
        .to_bigint()
        .ok_or_else(|| anyhow::anyhow!("Failed to convert TokenAmount to BigInt"))?
        .to_u128()
        .ok_or_else(|| anyhow::anyhow!("TokenAmount out of u128 range"))
}
