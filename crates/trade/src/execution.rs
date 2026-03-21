//! 取引実行・清算・評価期間管理モジュール
//!
//! ポートフォリオ戦略で決定された取引アクションの実行、
//! ポジションの清算、評価期間のライフサイクル管理を担当する。

use crate::Result;
use crate::{recorder::TradeRecorder, swap};
use bigdecimal::{BigDecimal, ToPrimitive, Zero};
use blockchain::jsonrpc::{AccountInfo, GasInfo, SendTx, SentTx, ViewContract};
use blockchain::wallet::Wallet;
use chrono::{DateTime, Utc};
use common::algorithm::types::TradingAction;
use common::config::ConfigAccess;
use common::types::*;
use logging::*;
use near_sdk::NearToken;
pub(crate) mod matching;

use matching::{
    BuyOperation, PhaseCounters, SellOperation, match_rebalance_operations, token_amount_to_u128,
};
use persistence::evaluation_period::{EvaluationPeriod, NewEvaluationPeriod};
use std::collections::HashMap;
use std::fmt::Display;

/// 実行サマリー
pub struct ExecutionSummary {
    pub success_count: usize,
    pub failed_count: usize,
}

/// AddPosition の swap 金額を weight に基づいて按分計算する。
/// 最後の AddPosition が端数を含む残額を使い切る。
///
/// # 引数
/// - `add_positions`: (アクションインデックス, weight) のリスト
/// - `balance`: wrap.near 残高 (yocto)
///
/// # 戻り値
/// (アクションインデックス, swap金額) のリスト
fn allocate_add_position_amounts(
    add_positions: &[(usize, BigDecimal)],
    balance: u128,
) -> Vec<(usize, u128)> {
    if add_positions.is_empty() {
        return vec![];
    }

    let total_weight: BigDecimal = add_positions.iter().map(|(_, w)| w).sum();

    // weight を basis points (1/10000) に変換して整数演算
    let weights_bps: Vec<u128> = add_positions
        .iter()
        .map(|(_, w)| {
            if total_weight.is_zero() {
                0u128
            } else {
                (w / &total_weight * BigDecimal::from(10_000))
                    .to_u128()
                    .unwrap_or(0)
            }
        })
        .collect();
    let total_bps: u128 = weights_bps.iter().sum();

    let mut allocated_sum: u128 = 0;
    let mut result = Vec::with_capacity(add_positions.len());

    for (i, &(idx, _)) in add_positions.iter().enumerate() {
        let amount = if i == add_positions.len() - 1 {
            // 最後の AddPosition は残額全部を使い切る
            balance.saturating_sub(allocated_sum)
        } else if total_bps > 0 {
            balance / total_bps * weights_bps[i] + balance % total_bps * weights_bps[i] / total_bps
        } else {
            0
        };
        allocated_sum = allocated_sum.saturating_add(amount);
        result.push((idx, amount));
    }

    result
}

/// 全 AddPosition の swap 金額を事前に計算する。
/// 最後の AddPosition が端数を含む残額を使い切る。
///
/// # 戻り値
/// HashMap<アクションのインデックス, swap金額(yocto)>
async fn precompute_add_position_amounts<C, W>(
    client: &C,
    wallet: &W,
    actions: &[TradingAction],
) -> Result<HashMap<usize, u128>>
where
    C: ViewContract,
    W: Wallet,
{
    let add_positions: Vec<(usize, BigDecimal)> = actions
        .iter()
        .enumerate()
        .filter_map(|(idx, action)| match action {
            TradingAction::AddPosition { weight, .. } => Some((idx, weight.clone())),
            _ => None,
        })
        .collect();

    if add_positions.is_empty() {
        return Ok(HashMap::new());
    }

    let wrap_near = &blockchain::ref_finance::token_account::WNEAR_TOKEN;
    let account = wallet.account_id();
    let deposits = blockchain::ref_finance::deposit::get_deposits(client, account).await?;
    let balance = deposits.get(wrap_near).map(|u| u.0).unwrap_or_default();

    Ok(allocate_add_position_amounts(&add_positions, balance)
        .into_iter()
        .collect())
}

/// 取引アクションを実際に実行
pub(crate) async fn execute_trading_actions<C, W>(
    client: &C,
    wallet: &W,
    actions: &[TradingAction],
    period_id: String,
    cfg: &impl ConfigAccess,
) -> Result<ExecutionSummary>
where
    C: AccountInfo + SendTx + ViewContract + GasInfo,
    <C as SendTx>::Output: Display + SentTx,
    W: Wallet,
{
    let log = DEFAULT.new(o!("function" => "execute_trading_actions"));

    let mut summary = ExecutionSummary {
        success_count: 0,
        failed_count: 0,
    };

    // TradeRecorderを作成（バッチIDで関連取引をグループ化）
    let recorder = TradeRecorder::new(period_id.clone());
    trace!(log, "created trade recorder";
        "batch_id" => recorder.get_batch_id(),
        "period_id" => %period_id
    );

    // AddPosition の swap 金額を事前に一括計算
    let add_position_amounts = precompute_add_position_amounts(client, wallet, actions).await?;

    for (idx, action) in actions.iter().enumerate() {
        let swap_amount_override = add_position_amounts.get(&idx).copied();
        match execute_single_action(
            client,
            wallet,
            action,
            &recorder,
            swap_amount_override,
            &period_id,
            cfg,
        )
        .await
        {
            Ok(_) => {
                info!(log, "action executed successfully"; "action" => ?action);
                summary.success_count += 1;
            }
            Err(e) => {
                error!(log, "action execution failed"; "action" => ?action, "error" => ?e);
                summary.failed_count += 1;
            }
        }
    }

    Ok(summary)
}

/// 単一の取引アクションを実行
async fn execute_single_action<C, W>(
    client: &C,
    wallet: &W,
    action: &TradingAction,
    recorder: &TradeRecorder,
    swap_amount_override: Option<u128>,
    evaluation_period_id: &str,
    cfg: &impl ConfigAccess,
) -> Result<()>
where
    C: blockchain::jsonrpc::AccountInfo
        + blockchain::jsonrpc::SendTx
        + blockchain::jsonrpc::ViewContract
        + blockchain::jsonrpc::GasInfo,
    <C as blockchain::jsonrpc::SendTx>::Output: std::fmt::Display + blockchain::jsonrpc::SentTx,
    W: blockchain::wallet::Wallet,
{
    let log = DEFAULT.new(o!("function" => "execute_single_action"));

    match action {
        TradingAction::Hold => {
            // HODLなので何もしない
            trace!(log, "holding position");
            Ok(())
        }
        TradingAction::Sell { token, target } => {
            // token を売却して target を購入
            debug!(log, "executing sell"; "from" => %token, "to" => %target);

            // 2段階のswap: token → wrap.near → target
            let wrap_near = &*blockchain::ref_finance::token_account::WNEAR_TOKEN;
            let wrap_near_in: TokenInAccount = wrap_near.to_in();
            let wrap_near_out: TokenOutAccount = wrap_near.to_out();

            // Step 1: token → wrap.near
            // common と backend の TokenAccount は同一型なので直接使用可能
            if token.inner() != wrap_near {
                let from_token = token.as_in();
                swap::execute_direct_swap(
                    client,
                    wallet,
                    &from_token,
                    &wrap_near_out,
                    None,
                    recorder,
                    cfg,
                )
                .await?;
            }

            // Step 2: wrap.near → target
            if target.inner() != wrap_near {
                swap::execute_direct_swap(
                    client,
                    wallet,
                    &wrap_near_in,
                    target,
                    None,
                    recorder,
                    cfg,
                )
                .await?;
            }

            debug!(log, "sell completed"; "from" => %token, "to" => %target);
            Ok(())
        }
        TradingAction::Switch { from, to } => {
            // from から to へ切り替え（直接スワップ）
            // common と backend の TokenAccount は同一型なので直接使用可能
            debug!(log, "executing switch"; "from" => %from, "to" => %to);

            let from_token = from.as_in();
            swap::execute_direct_swap(client, wallet, &from_token, to, None, recorder, cfg).await?;

            debug!(log, "switch completed"; "from" => %from, "to" => %to);
            Ok(())
        }
        TradingAction::Rebalance { target_weights } => {
            // ポートフォリオのリバランス
            debug!(log, "executing rebalance"; "weights" => ?target_weights);

            // 現在の保有量を取得（wrap.nearを明示的に追加）
            let mut tokens: Vec<TokenAccount> =
                target_weights.keys().map(|t| t.inner().clone()).collect();
            let wnear = &*blockchain::ref_finance::token_account::WNEAR_TOKEN;
            if !tokens.contains(wnear) {
                tokens.push(wnear.clone());
                trace!(
                    log,
                    "added wrap.near to balance query for total value calculation"
                );
            }
            trace!(log, "tokens list for balance query"; "tokens" => ?tokens, "count" => tokens.len());

            let current_balances =
                match crate::snapshot::get_holdings_from_db(evaluation_period_id).await? {
                    Some(holdings) => holdings,
                    None => {
                        crate::swap::get_current_portfolio_balances(client, wallet, &tokens).await?
                    }
                };

            // 総ポートフォリオ価値を計算
            let total_portfolio_value =
                crate::swap::calculate_total_portfolio_value(client, wallet, &current_balances)
                    .await?;

            // 各トークンの差分（wrap.near換算）を計算
            let mut sell_operations: Vec<SellOperation> = Vec::new();
            let mut buy_operations: Vec<BuyOperation> = Vec::new();

            for (token, target_weight) in target_weights.iter() {
                let token_account: &TokenAccount = token.inner();

                // weight の有効性確認
                if *target_weight < BigDecimal::zero() {
                    warn!(log, "invalid weight, skipping"; "token" => %token_account, "weight" => %target_weight);
                    continue;
                }

                if token_account == wnear {
                    continue; // wrap.nearは除外
                }

                let current_amount = current_balances.get(token_account);

                // レートを取得してキャッシュ（売却時に再利用）
                let spot_rate = match current_amount {
                    Some(amount) if !amount.is_zero() => {
                        let token_out: TokenOutAccount = token_account.clone().into();
                        let quote_in = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_in();

                        let rate =
                            persistence::token_rate::TokenRate::get_latest(&token_out, &quote_in)
                                .await?
                                .ok_or_else(|| {
                                    anyhow::anyhow!("No rate found for token: {}", token_account)
                                })?;

                        let spot = rate.to_spot_rate();
                        if spot.is_effectively_zero() {
                            warn!(log, "rate too small for rebalance"; "token" => %token_account);
                            None
                        } else {
                            Some(spot)
                        }
                    }
                    _ => None,
                };

                // 現在の価値（wrap.near換算）を計算
                let current_value_wrap_near: NearValue = match (current_amount, &spot_rate) {
                    (Some(amount), Some(rate)) if !amount.is_zero() => amount / rate,
                    _ => NearValue::zero(),
                };

                // 目標価値（wrap.near換算）を計算
                let target_value_wrap_near: NearValue = &total_portfolio_value * target_weight;

                // 差分を計算（wrap.near単位）
                let diff_wrap_near: NearValue = &target_value_wrap_near - &current_value_wrap_near;

                trace!(log, "rebalancing: token analysis";
                    "token" => %token_account,
                    "current_value_wrap_near" => %current_value_wrap_near,
                    "target_value_wrap_near" => %target_value_wrap_near,
                    "diff_wrap_near" => %diff_wrap_near
                );

                // 最小交換額チェック（1 NEAR以上）
                let min_trade_size = NearValue::one();
                let zero = NearValue::zero();

                if diff_wrap_near < zero && diff_wrap_near.abs() >= min_trade_size {
                    // 売却が必要 — キャッシュ済みレートを再利用
                    let rate = spot_rate.ok_or_else(|| {
                        anyhow::anyhow!("No cached rate for sell operation: {}", token_account)
                    })?;

                    sell_operations.push(SellOperation {
                        token: token_account.clone(),
                        near_value: diff_wrap_near.abs(),
                        exchange_rate: rate,
                    });
                } else if diff_wrap_near > zero && diff_wrap_near >= min_trade_size {
                    // 購入が必要
                    buy_operations.push(BuyOperation {
                        token: token_account.clone(),
                        near_value: diff_wrap_near,
                    });
                }
            }

            // 型安全なwrap.nearを事前に準備
            let wrap_near_token = &blockchain::ref_finance::token_account::WNEAR_TOKEN;
            let wrap_near_in: TokenInAccount = wrap_near_token.to_in();
            let wrap_near_out: TokenOutAccount = wrap_near_token.to_out();

            info!(log, "rebalance operations";
                "sell_count" => sell_operations.len(),
                "buy_count" => buy_operations.len()
            );

            let mut direct_swap = PhaseCounters::new();
            let mut remainder_sell = PhaseCounters::new();
            let mut remainder_buy = PhaseCounters::new();

            // 常に match_rebalance_operations を通す（売却のみ・購入のみも統一処理）
            let mut result = match_rebalance_operations(sell_operations, buy_operations);

            info!(log, "direct swap matching result";
                "direct_swap_count" => result.direct_swaps.len(),
                "remaining_sells" => result.remaining_sells.len(),
                "remaining_buys" => result.remaining_buys.len()
            );

            // 1. 直接スワップ実行（near_value 降順 — match_rebalance_operations がソート済み）
            // 失敗した直接スワップは remaining に fallback し、wNEAR 経由で再試行される
            let fallback_to_remaining =
                |result: &mut matching::MatchResult, ds: &matching::DirectSwap| {
                    result.remaining_sells.push(SellOperation {
                        token: ds.sell_token.clone(),
                        near_value: ds.near_value.clone(),
                        exchange_rate: ds.sell_exchange_rate.clone(),
                    });
                    result.remaining_buys.push(BuyOperation {
                        token: ds.buy_token.clone(),
                        near_value: ds.near_value.clone(),
                    });
                };

            for ds in &result.direct_swaps.clone() {
                let token_amount: TokenAmount = &ds.near_value * &ds.sell_exchange_rate;
                let token_amount_u128 = match token_amount_to_u128(&token_amount) {
                    Ok(v) => v,
                    Err(e) => {
                        error!(log, "token amount conversion failed"; "error" => %e);
                        direct_swap.failed += 1;
                        fallback_to_remaining(&mut result, ds);
                        continue;
                    }
                };

                if token_amount_u128 == 0 {
                    warn!(log, "token amount truncated to zero, skipping direct swap";
                        "sell_token" => %ds.sell_token, "buy_token" => %ds.buy_token);
                    direct_swap.failed += 1;
                    fallback_to_remaining(&mut result, ds);
                    continue;
                }

                trace!(log, "executing direct swap";
                    "sell_token" => %ds.sell_token,
                    "buy_token" => %ds.buy_token,
                    "near_value" => %ds.near_value,
                    "token_amount" => token_amount_u128
                );

                let from_token: TokenInAccount = ds.sell_token.clone().into();
                let to_token: TokenOutAccount = ds.buy_token.clone().into();
                match swap::execute_direct_swap(
                    client,
                    wallet,
                    &from_token,
                    &to_token,
                    Some(token_amount_u128),
                    recorder,
                    cfg,
                )
                .await
                {
                    Ok(_) => {
                        info!(log, "direct swap completed";
                            "sell_token" => %ds.sell_token, "buy_token" => %ds.buy_token);
                        direct_swap.success += 1;
                    }
                    Err(e) => {
                        error!(log, "direct swap failed";
                            "sell_token" => %ds.sell_token, "buy_token" => %ds.buy_token,
                            "error" => %e);
                        direct_swap.failed += 1;
                        fallback_to_remaining(&mut result, ds);
                    }
                }
            }

            // 2. 残余売却実行（token → wNEAR）
            for sell in &result.remaining_sells {
                let token_amount: TokenAmount = &sell.near_value * &sell.exchange_rate;
                let token_amount_u128 = match token_amount_to_u128(&token_amount) {
                    Ok(v) => v,
                    Err(e) => {
                        error!(log, "token amount conversion failed"; "error" => %e);
                        remainder_sell.failed += 1;
                        continue;
                    }
                };

                if token_amount_u128 == 0 {
                    warn!(log, "token amount truncated to zero, skipping sell";
                        "token" => %sell.token);
                    remainder_sell.failed += 1;
                    continue;
                }

                trace!(log, "executing remainder sell";
                    "token" => %sell.token, "near_value" => %sell.near_value);
                let from_token: TokenInAccount = sell.token.clone().into();
                match swap::execute_direct_swap(
                    client,
                    wallet,
                    &from_token,
                    &wrap_near_out,
                    Some(token_amount_u128),
                    recorder,
                    cfg,
                )
                .await
                {
                    Ok(_) => {
                        info!(log, "remainder sell completed"; "token" => %sell.token);
                        remainder_sell.success += 1;
                    }
                    Err(e) => {
                        error!(log, "remainder sell failed"; "token" => %sell.token, "error" => %e);
                        remainder_sell.failed += 1;
                    }
                }
            }

            // 3. 残余購入実行（wNEAR → token、比率調整あり）
            if !result.remaining_buys.is_empty() {
                let available_wrap_near = {
                    let account = wallet.account_id();
                    let deposits =
                        blockchain::ref_finance::deposit::get_deposits(client, account).await?;
                    deposits
                        .get(wrap_near_token)
                        .map(|u| u.0)
                        .unwrap_or_default()
                };
                let available_wrap_near_value =
                    YoctoValue::from_yocto(BigDecimal::from(available_wrap_near)).to_near();

                let total_buy_value: NearValue =
                    result.remaining_buys.iter().map(|op| &op.near_value).sum();

                let ratio = if total_buy_value > available_wrap_near_value {
                    Some(&available_wrap_near_value / &total_buy_value)
                } else {
                    None
                };

                // 最後の購入で端数を回収する（allocate_add_position_amounts と同パターン）
                let mut allocated_sum: u128 = 0;
                let buy_count = result.remaining_buys.len();
                for (i, buy) in result.remaining_buys.iter().enumerate() {
                    let is_last = i == buy_count - 1;
                    let wrap_near_amount_u128 = if is_last && ratio.is_some() {
                        // 最後の購入は残額を使い切り、ratio 按分の切り捨て端数を回収
                        available_wrap_near.saturating_sub(allocated_sum)
                    } else {
                        let adjusted_value = match &ratio {
                            Some(r) => &buy.near_value * r,
                            None => buy.near_value.clone(),
                        };
                        adjusted_value.to_yocto().to_amount().to_u128()
                    };
                    allocated_sum = allocated_sum.saturating_add(wrap_near_amount_u128);

                    if wrap_near_amount_u128 == 0 {
                        error!(log, "purchase amount is zero after conversion";
                            "token" => %buy.token,
                            "original_near_value" => %buy.near_value);
                        remainder_buy.failed += 1;
                        continue;
                    }

                    trace!(log, "executing remainder buy";
                        "token" => %buy.token,
                        "original_value" => %buy.near_value,
                        "wrap_near_amount" => wrap_near_amount_u128);
                    let to_token: TokenOutAccount = buy.token.clone().into();
                    match swap::execute_direct_swap(
                        client,
                        wallet,
                        &wrap_near_in,
                        &to_token,
                        Some(wrap_near_amount_u128),
                        recorder,
                        cfg,
                    )
                    .await
                    {
                        Ok(_) => {
                            info!(log, "remainder buy completed"; "token" => %buy.token);
                            remainder_buy.success += 1;
                        }
                        Err(e) => {
                            error!(log, "remainder buy failed"; "token" => %buy.token, "error" => %e);
                            remainder_buy.failed += 1;
                        }
                    }
                }
            }

            let total_success =
                direct_swap.success + remainder_sell.success + remainder_buy.success;
            let total_failed = direct_swap.failed + remainder_sell.failed + remainder_buy.failed;

            if direct_swap.failed > direct_swap.success && direct_swap.failed > 0 {
                warn!(log, "high direct swap failure rate";
                    "success" => direct_swap.success, "failed" => direct_swap.failed);
            }
            if !result.remaining_sells.is_empty()
                && remainder_sell.success == 0
                && remainder_sell.failed > 0
            {
                warn!(log, "all remainder sells failed";
                    "failed" => remainder_sell.failed);
            }
            if !result.remaining_buys.is_empty()
                && remainder_buy.success == 0
                && remainder_buy.failed > 0
            {
                warn!(log, "all remainder buys failed";
                    "failed" => remainder_buy.failed);
            }

            info!(log, "rebalance completed";
                "direct_swap_success" => direct_swap.success,
                "direct_swap_failed" => direct_swap.failed,
                "remainder_sell_success" => remainder_sell.success,
                "remainder_sell_failed" => remainder_sell.failed,
                "remainder_buy_success" => remainder_buy.success,
                "remainder_buy_failed" => remainder_buy.failed
            );

            // 全操作失敗時のみエラーを返す。部分失敗は次回リバランスサイクルで自然修正。
            if total_success == 0 && total_failed > 0 {
                return Err(anyhow::anyhow!(
                    "All rebalance operations failed ({} failed)",
                    total_failed
                ));
            }

            Ok(())
        }
        TradingAction::AddPosition { token, weight } => {
            // ポジション追加
            debug!(log, "adding position"; "token" => %token, "weight" => %weight);

            // wrap.near → token へのswap
            let wrap_near = &*blockchain::ref_finance::token_account::WNEAR_TOKEN;
            if token.inner() != wrap_near {
                let swap_amount = swap_amount_override.ok_or_else(|| {
                    anyhow::anyhow!("No pre-computed swap amount for AddPosition: {}", token)
                })?;

                debug!(log, "using pre-computed swap amount";
                    "swap_amount" => swap_amount, "weight" => %weight
                );

                if swap_amount == 0 {
                    return Err(anyhow::anyhow!(
                        "Pre-computed swap amount is 0 for token: {} (weight: {})",
                        token,
                        weight
                    ));
                }

                let wrap_near_in: TokenInAccount = wrap_near.to_in();
                swap::execute_direct_swap(
                    client,
                    wallet,
                    &wrap_near_in,
                    token,
                    Some(swap_amount),
                    recorder,
                    cfg,
                )
                .await?;
            }

            debug!(log, "position added"; "token" => %token, "weight" => %weight);
            Ok(())
        }
        TradingAction::ReducePosition { token, weight } => {
            // ポジション削減
            // common と backend の TokenAccount は同一型なので直接使用可能
            debug!(log, "reducing position"; "token" => %token, "weight" => %weight);

            // token → wrap.near へのswap
            let wrap_near = &*blockchain::ref_finance::token_account::WNEAR_TOKEN;
            if token.inner() != wrap_near {
                let from_token = token.as_in();
                let wrap_near_out: TokenOutAccount = wrap_near.to_out();
                swap::execute_direct_swap(
                    client,
                    wallet,
                    &from_token,
                    &wrap_near_out,
                    None,
                    recorder,
                    cfg,
                )
                .await?;
            }

            debug!(log, "position reduced"; "token" => %token, "weight" => %weight);
            Ok(())
        }
    }
}

/// 評価期間のチェックと管理
///
/// 戻り値: (period_id, is_new_period, selected_tokens, liquidated_balance)
/// - liquidated_balance: 清算が行われた場合の最終残高
pub(crate) async fn manage_evaluation_period<C, W>(
    client: &C,
    wallet: &W,
    current_time: DateTime<Utc>,
    available_funds: YoctoAmount,
    cfg: &impl ConfigAccess,
) -> Result<(String, bool, Vec<TokenAccount>, Option<YoctoAmount>)>
where
    C: AccountInfo + SendTx + ViewContract + GasInfo,
    <C as SendTx>::Output: Display + SentTx,
    W: Wallet,
{
    let log = DEFAULT.new(o!("function" => "manage_evaluation_period"));

    // 設定ファイルから評価期間を読み込む（デフォルト: 10日）
    let evaluation_period_days = i64::from(cfg.trade_evaluation_days());

    info!(log, "evaluation period configuration"; "days" => evaluation_period_days);

    // 最新の評価期間を取得
    let latest_period = EvaluationPeriod::get_latest_async().await?;

    match latest_period {
        Some(period) => {
            let now = current_time.naive_utc();
            let period_duration = now.signed_duration_since(period.start_time);
            // period のフィールドを事前に取り出す（clone 不要で move）
            let period_id = period.period_id;
            let period_initial_value = period.initial_value;
            let period_selected_tokens = period.selected_tokens;

            if period_duration.num_days() >= evaluation_period_days {
                // 評価期間終了: 全トークンを売却して新規期間を開始
                info!(log, "evaluation period ended, starting new period";
                    "previous_period_id" => %period_id,
                    "days_elapsed" => period_duration.num_days()
                );

                // 全トークンをwrap.nearに売却
                let final_balance = liquidate_all_positions(client, wallet, cfg).await?;
                info!(log, "liquidated all positions"; "final_balance" => %final_balance);

                // 評価期間のパフォーマンスを計算してログ出力
                let initial_value = period_initial_value.to_value();
                let final_value = final_balance.to_value();

                // 参照同士の演算（clone 不要）
                let change_amount = &final_value - &initial_value;
                let change_percentage = if !initial_value.is_zero() {
                    (&final_value / &initial_value - BigDecimal::from(1)) * BigDecimal::from(100)
                } else {
                    BigDecimal::from(0)
                };

                info!(log, "evaluation period performance";
                    "period_id" => %period_id,
                    "initial_value" => %initial_value,
                    "final_value" => %final_value,
                    "change_amount" => %change_amount,
                    "change_percentage" => %format!("{:.2}%", change_percentage)
                );

                // ハーベスト判定: 旧 period の initial_value と清算後の final_value で比較
                // 新 period 作成前に実行することで、正しい initial_value で判定できる
                let harvested_amount = crate::harvest::check_and_execute_harvest(
                    &initial_value,
                    &final_value,
                    &period_id,
                    cfg,
                )
                .await
                .unwrap_or_else(|e| {
                    error!(log, "harvest failed, continuing with new period"; "error" => %e);
                    YoctoAmount::zero()
                });

                // ハーベスト後の残高を取得（ハーベスト実行時は REF Finance 残高が変動）
                // ハーベスト未実行の場合（閾値未達・時間条件・最低額条件等）は清算時の残高をそのまま使用
                let post_harvest_balance = if !harvested_amount.is_zero() {
                    info!(log, "harvest completed, refreshing balance"; "harvested" => %harvested_amount);
                    let account = wallet.account_id();
                    let wrap_near = &blockchain::ref_finance::token_account::WNEAR_TOKEN;
                    let deposits =
                        blockchain::ref_finance::deposit::get_deposits(client, account).await?;
                    let balance = deposits.get(wrap_near).map(|u| u.0).unwrap_or_default();
                    YoctoAmount::from_u128(balance)
                } else {
                    final_balance
                };
                let post_harvest_value = post_harvest_balance.to_value();

                // TRADE_ENABLED をチェック
                let trade_enabled = cfg.trade_enabled();

                if !trade_enabled {
                    info!(log, "trade disabled, not starting new period";
                        "final_balance" => %post_harvest_balance
                    );

                    // TRADE_UNWRAP_ON_STOP が有効な場合、wrap.near を NEAR に戻して送金
                    let unwrap_on_stop = cfg.trade_unwrap_on_stop();

                    if unwrap_on_stop {
                        info!(log, "unwrap_on_stop enabled, executing unwrap and transfer");
                        if let Err(e) = unwrap_and_transfer_wnear(&log, cfg).await {
                            error!(log, "failed to unwrap and transfer"; "error" => %e);
                        }
                    }

                    // 空の period_id を返して停止を通知
                    return Ok((String::new(), false, vec![], Some(post_harvest_balance)));
                }

                // 新規評価期間を作成（ハーベスト後の残高を initial_value とする）
                let new_period = NewEvaluationPeriod::new(post_harvest_value.to_amount(), vec![]);
                let created_period = new_period.insert_async().await?;

                info!(log, "created new evaluation period";
                    "period_id" => %created_period.period_id,
                    "initial_value" => %created_period.initial_value
                );

                Ok((
                    created_period.period_id,
                    true,
                    vec![],
                    Some(post_harvest_balance),
                ))
            } else {
                // 評価期間中: トランザクション記録で判定
                debug!(log, "checking evaluation period status";
                    "period_id" => %period_id,
                    "days_remaining" => evaluation_period_days - period_duration.num_days()
                );

                // トランザクション記録をチェック
                use persistence::trade_transaction::TradeTransaction;
                let transaction_count =
                    TradeTransaction::count_by_evaluation_period_async(period_id.clone()).await?;

                debug!(log, "transaction count for period";
                    "count" => transaction_count,
                    "period_id" => %period_id
                );

                let selected_tokens: Vec<TokenAccount> = period_selected_tokens
                    .unwrap_or_default()
                    .into_iter()
                    .flatten()
                    .filter_map(|s| s.parse::<TokenAccount>().ok())
                    .collect();

                // selected_tokens が空かつトランザクションがゼロなら新規期間として扱う
                // selected_tokens.is_empty() だけだとパース全失敗（データ破損）時に誤判定するため、
                // transaction_count == 0 も併用して安全性を確保
                let is_new_period = selected_tokens.is_empty() && transaction_count == 0;

                if selected_tokens.is_empty() && transaction_count > 0 {
                    error!(log, "selected_tokens empty but transactions exist, possible data corruption";
                        "transaction_count" => transaction_count);
                }

                if is_new_period {
                    debug!(
                        log,
                        "no transactions found in period, treating as new period"
                    );
                } else {
                    debug!(log, "continuing evaluation period with existing positions";
                        "transaction_count" => transaction_count
                    );
                }

                Ok((period_id, is_new_period, selected_tokens, None))
            }
        }
        None => {
            // 初回起動: 新規評価期間を作成
            info!(log, "no evaluation period found, creating first period");

            let new_period = NewEvaluationPeriod::new(available_funds.clone(), vec![]);
            let created_period = new_period.insert_async().await?;

            info!(log, "created first evaluation period";
                "period_id" => %created_period.period_id,
                "initial_value" => %created_period.initial_value
            );

            Ok((created_period.period_id, true, vec![], None))
        }
    }
}

/// REF Financeの残高から清算対象トークンをフィルタリング
///
/// wrap.nearとゼロ残高のトークンを除外し、清算すべきトークンのリストを返す
pub(crate) fn filter_tokens_to_liquidate(
    deposits: &HashMap<TokenAccount, near_sdk::json_types::U128>,
    wrap_near_token: &TokenAccount,
) -> Vec<TokenAccount> {
    deposits
        .iter()
        .filter_map(|(token, amount)| {
            // wrap.nearは除外し、残高があるトークンのみを対象とする
            if token != wrap_near_token && amount.0 > 0 {
                Some(token.clone())
            } else {
                None
            }
        })
        .collect()
}

/// 全保有トークンをwrap.nearに売却
///
/// 戻り値: 売却後のwrap.near総額 (yoctoNEAR)
pub(crate) async fn liquidate_all_positions<C, W>(
    client: &C,
    wallet: &W,
    cfg: &impl ConfigAccess,
) -> Result<YoctoAmount>
where
    C: AccountInfo + SendTx + ViewContract + GasInfo,
    <C as SendTx>::Output: Display + SentTx,
    W: Wallet,
{
    let log = DEFAULT.new(o!("function" => "liquidate_all_positions"));

    // 最新の評価期間を取得
    let latest_period = EvaluationPeriod::get_latest_async().await?;
    let period_id = match latest_period {
        Some(period) => {
            // selected_tokensは履歴として記録（実際の清算には使用しない）
            let selected_tokens: Vec<String> = period
                .selected_tokens
                .unwrap_or_default()
                .into_iter()
                .flatten()
                .collect();
            trace!(log, "evaluation period selected tokens";
                  "period_id" => &period.period_id,
                  "selected_tokens" => ?selected_tokens);
            period.period_id
        }
        None => {
            debug!(log, "no evaluation period found, nothing to liquidate");
            return Ok(YoctoAmount::zero());
        }
    };

    // 実際のREF Finance残高を取得して清算対象を決定
    let account = wallet.account_id();
    let wrap_near_token = &blockchain::ref_finance::token_account::WNEAR_TOKEN;

    let deposits = blockchain::ref_finance::deposit::get_deposits(client, account).await?;
    let tokens_to_liquidate = filter_tokens_to_liquidate(&deposits, wrap_near_token);

    if tokens_to_liquidate.is_empty() {
        debug!(log, "no tokens to liquidate");
        // wrap.nearの残高を返す
        let wrap_near = &blockchain::ref_finance::token_account::WNEAR_TOKEN;
        let balance = deposits.get(wrap_near).map(|u| u.0).unwrap_or_default();
        return Ok(YoctoAmount::from_u128(balance));
    }

    info!(log, "liquidating positions"; "token_count" => tokens_to_liquidate.len());

    // トレードレコーダーを作成
    let recorder = TradeRecorder::new(period_id);

    // 型安全なwrap.nearを事前に準備
    let wrap_near_out: TokenOutAccount = wrap_near_token.to_out();

    // 各トークンをwrap.nearに変換
    for token in &tokens_to_liquidate {
        trace!(log, "liquidating token"; "token" => %token);

        // トークンの REF Finance 上の残高を取得
        let account = wallet.account_id();
        let deposits = blockchain::ref_finance::deposit::get_deposits(client, account).await?;
        let balance = deposits.get(token).map(|u| u.0).unwrap_or_default();

        if balance == 0 {
            trace!(log, "token balance became zero, skipping"; "token" => %token);
            continue;
        }

        // token → wrap.near にスワップ
        let from_token: TokenInAccount = token.clone().into();
        match swap::execute_direct_swap(
            client,
            wallet,
            &from_token,
            &wrap_near_out,
            None,
            &recorder,
            cfg,
        )
        .await
        {
            Ok(_) => {
                trace!(log, "successfully liquidated token"; "token" => %token);
            }
            Err(e) => {
                error!(log, "failed to liquidate token"; "token" => %token, "error" => ?e);
                // エラーが発生しても他のトークンの売却は継続
            }
        }
    }

    // 最終的なwrap.near残高を取得
    let account = wallet.account_id();
    let deposits = blockchain::ref_finance::deposit::get_deposits(client, account).await?;
    let wrap_near = &blockchain::ref_finance::token_account::WNEAR_TOKEN;
    let final_balance =
        YoctoAmount::from_u128(deposits.get(wrap_near).map(|u| u.0).unwrap_or_default());

    info!(log, "liquidation complete"; "final_wrap_near_balance" => %final_balance);
    Ok(final_balance)
}

/// 評価期間終了時に wrap.near を NEAR に戻して HARVEST_ACCOUNT_ID に送金
///
/// 処理フロー:
/// 1. REF Finance から wrap.near を withdraw
/// 2. wrap.near を NEAR に unwrap
/// 3. NEAR を HARVEST_ACCOUNT_ID に送金（HARVEST_RESERVE_AMOUNT を残す）
async fn unwrap_and_transfer_wnear(log: &slog::Logger, cfg: &impl ConfigAccess) -> Result<()> {
    use blockchain::jsonrpc::{AccountInfo, SendTx, SentTx};
    use blockchain::ref_finance::{deposit, token_account::WNEAR_TOKEN};
    use common::types::{NearAmount, YoctoAmount};

    // HARVEST_ACCOUNT_ID を取得（未設定の場合はスキップ）
    let harvest_account_id = match cfg.harvest_account_id() {
        Ok(id) if !id.is_empty() => id,
        _ => {
            info!(
                log,
                "HARVEST_ACCOUNT_ID not set, skipping unwrap and transfer"
            );
            return Ok(());
        }
    };

    let harvest_account: near_sdk::AccountId = harvest_account_id
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid HARVEST_ACCOUNT_ID: {}", e))?;

    // HARVEST_RESERVE_AMOUNT を取得
    let reserve_amount: YoctoAmount = cfg
        .harvest_reserve_amount()
        .to_string()
        .parse::<NearAmount>()
        .map_err(|e| anyhow::anyhow!("Failed to parse harvest reserve amount: {}", e))?
        .to_yocto();
    let mut reserve_amount_u128: u128 = reserve_amount.to_u128();
    if reserve_amount_u128 == 0 {
        warn!(log, "harvest reserve amount converted to zero, using default 1 NEAR";
            "configured_value" => cfg.harvest_reserve_amount());
        reserve_amount_u128 = 1_000_000_000_000_000_000_000_000; // 1 NEAR
    }

    let client = blockchain::jsonrpc::new_client();
    let wallet = blockchain::wallet::new_wallet();
    let account = wallet.account_id();

    // REF Finance 内の wrap.near 残高を取得
    let deposits = deposit::get_deposits(&client, account).await?;
    let wnear_balance = deposits.get(&WNEAR_TOKEN).map(|u| u.0).unwrap_or_default();

    if wnear_balance == 0 {
        info!(log, "no wrap.near balance in REF Finance, skipping");
        return Ok(());
    }

    info!(log, "starting unwrap and transfer";
        "wnear_balance" => wnear_balance,
        "target_account" => %harvest_account,
        "reserve_amount" => reserve_amount_u128
    );

    // 1. REF Finance から wrap.near を withdraw
    let wnear_token = NearToken::from_yoctonear(wnear_balance);
    trace!(log, "withdrawing wrap.near from REF Finance"; "amount" => wnear_balance);
    let withdraw_tx = deposit::withdraw(&client, &wallet, &WNEAR_TOKEN, wnear_token).await?;
    if let Err(e) = withdraw_tx.wait_for_success().await {
        error!(log, "failed to withdraw from REF Finance"; "error" => %e);
        return Err(anyhow::anyhow!("Withdraw failed: {}", e));
    }

    // 2. wrap.near を NEAR に unwrap
    trace!(log, "unwrapping wrap.near to NEAR"; "amount" => wnear_balance);
    let unwrap_tx = deposit::wnear::unwrap(&client, &wallet, wnear_token).await?;
    if let Err(e) = unwrap_tx.wait_for_success().await {
        error!(log, "failed to unwrap NEAR"; "error" => %e);
        return Err(anyhow::anyhow!("Unwrap failed: {}", e));
    }

    // 3. NEAR を HARVEST_ACCOUNT_ID に送金（HARVEST_RESERVE_AMOUNT を残す）
    let current_native_balance = client.get_native_amount(account).await?;
    let reserve_amount_token = NearToken::from_yoctonear(reserve_amount_u128);

    let available_for_transfer = if current_native_balance > reserve_amount_token {
        current_native_balance.saturating_sub(reserve_amount_token)
    } else {
        info!(log, "insufficient balance for transfer after reserve";
            "current_balance" => current_native_balance.as_yoctonear(),
            "reserve_amount" => reserve_amount_u128
        );
        return Ok(());
    };

    if available_for_transfer.as_yoctonear() == 0 {
        info!(log, "no NEAR available for transfer after reserve");
        return Ok(());
    }

    trace!(log, "transferring NEAR to harvest account";
        "amount" => available_for_transfer.as_yoctonear(),
        "target" => %harvest_account
    );

    let signer = wallet.signer();
    let sent_tx = client
        .transfer_native_token(signer, &harvest_account, available_for_transfer)
        .await?;

    let tx_outcome = sent_tx.wait_for_executed().await?;
    let tx_hash = match tx_outcome {
        near_primitives::views::FinalExecutionOutcomeViewEnum::FinalExecutionOutcome(view) => {
            view.transaction_outcome.id.to_string()
        }
        near_primitives::views::FinalExecutionOutcomeViewEnum::FinalExecutionOutcomeWithReceipt(
            view,
        ) => view.final_outcome.transaction_outcome.id.to_string(),
    };

    info!(log, "unwrap and transfer completed";
        "transferred_amount" => available_for_transfer.as_yoctonear(),
        "target_account" => %harvest_account,
        "tx_hash" => %tx_hash
    );

    Ok(())
}

#[cfg(test)]
mod tests;
