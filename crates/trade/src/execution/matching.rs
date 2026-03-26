//! リバランスの直接スワップマッチングアルゴリズム
//!
//! 売却・購入操作を直接スワップにマッチングし、
//! 中間 wNEAR 変換を削減する最適化を提供する。

use common::types::*;

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
    sell_operations: Vec<SellOperation>,
    buy_operations: Vec<BuyOperation>,
) -> MatchResult {
    if sell_operations.is_empty() || buy_operations.is_empty() {
        return MatchResult {
            direct_swaps: vec![],
            remaining_sells: sell_operations,
            remaining_buys: buy_operations,
        };
    }

    // 降順ソート（大きい金額を優先的にマッチ）
    let mut sell_operations = sell_operations;
    let mut buy_operations = buy_operations;
    sell_operations.sort_unstable_by(|a, b| b.near_value.cmp(&a.near_value));
    buy_operations.sort_unstable_by(|a, b| b.near_value.cmp(&a.near_value));

    let mut direct_swaps = Vec::with_capacity(sell_operations.len().max(buy_operations.len()));
    let mut sell_iter = sell_operations.into_iter();
    let mut buy_iter = buy_operations.into_iter();

    // Invariant: is_empty() ガードにより到達時は必ず要素がある
    let Some(mut current_sell) = sell_iter.next() else {
        unreachable!("sell_operations was verified non-empty above");
    };
    let Some(mut current_buy) = buy_iter.next() else {
        unreachable!("buy_operations was verified non-empty above");
    };
    let mut sell_remaining = current_sell.near_value.clone();
    let mut buy_remaining = current_buy.near_value.clone();

    loop {
        let match_value = std::cmp::min(&sell_remaining, &buy_remaining).clone();

        if match_value > NearValue::zero() {
            direct_swaps.push(DirectSwap {
                sell_token: current_sell.token.clone(),
                buy_token: current_buy.token.clone(),
                near_value: match_value.clone(),
                sell_exchange_rate: current_sell.exchange_rate.clone(),
            });
        }

        sell_remaining = &sell_remaining - &match_value;
        buy_remaining = &buy_remaining - &match_value;
        debug_assert!(
            sell_remaining >= NearValue::zero(),
            "sell_remaining must be non-negative"
        );
        debug_assert!(
            buy_remaining >= NearValue::zero(),
            "buy_remaining must be non-negative"
        );

        // 売却側が消化された場合、次の売却へ
        if sell_remaining == NearValue::zero() {
            match sell_iter.next() {
                None => {
                    // 売却を全て消化 — 購入側の残余を全て収集
                    let mut remaining_buys = Vec::new();
                    if buy_remaining > NearValue::zero() {
                        remaining_buys.push(BuyOperation {
                            token: current_buy.token,
                            near_value: buy_remaining,
                        });
                    }
                    remaining_buys.extend(buy_iter);
                    return MatchResult {
                        direct_swaps,
                        remaining_sells: vec![],
                        remaining_buys,
                    };
                }
                Some(next) => {
                    current_sell = next;
                    sell_remaining = current_sell.near_value.clone();
                }
            }
        }

        // 購入側が消化された場合、次の購入へ
        if buy_remaining == NearValue::zero() {
            match buy_iter.next() {
                None => {
                    // 購入を全て消化 — 売却側の残余を全て収集
                    let mut remaining_sells = Vec::new();
                    if sell_remaining > NearValue::zero() {
                        remaining_sells.push(SellOperation {
                            token: current_sell.token,
                            near_value: sell_remaining,
                            exchange_rate: current_sell.exchange_rate,
                        });
                    }
                    remaining_sells.extend(sell_iter);
                    return MatchResult {
                        direct_swaps,
                        remaining_sells,
                        remaining_buys: vec![],
                    };
                }
                Some(next) => {
                    current_buy = next;
                    buy_remaining = current_buy.near_value.clone();
                }
            }
        }
    }
}
