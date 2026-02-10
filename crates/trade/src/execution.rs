//! 取引実行・清算・評価期間管理モジュール
//!
//! ポートフォリオ戦略で決定された取引アクションの実行、
//! ポジションの清算、評価期間のライフサイクル管理を担当する。

use crate::Result;
use crate::{recorder::TradeRecorder, swap};
use bigdecimal::BigDecimal;
use blockchain::wallet::Wallet;
use chrono::Utc;
use common::algorithm::types::TradingAction;
use common::config;
use common::types::*;
use logging::*;
use near_sdk::NearToken;
use persistence::evaluation_period::{EvaluationPeriod, NewEvaluationPeriod};
use std::collections::HashMap;

/// 実行サマリー
pub struct ExecutionSummary {
    pub success_count: usize,
    pub failed_count: usize,
}

/// 取引アクションを実際に実行
pub(crate) async fn execute_trading_actions(
    actions: &[TradingAction],
    _available_funds: u128,
    period_id: String,
) -> Result<ExecutionSummary> {
    let log = DEFAULT.new(o!("function" => "execute_trading_actions"));

    let mut summary = ExecutionSummary {
        success_count: 0,
        failed_count: 0,
    };

    // JSONRPCクライアントとウォレットを取得
    let client = blockchain::jsonrpc::new_client();
    let wallet = blockchain::wallet::new_wallet();

    // TradeRecorderを作成（バッチIDで関連取引をグループ化）
    let recorder = TradeRecorder::new(period_id.clone());
    trace!(log, "created trade recorder";
        "batch_id" => recorder.get_batch_id(),
        "period_id" => %period_id
    );

    for action in actions {
        match execute_single_action(&client, &wallet, action, &recorder).await {
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
) -> Result<()>
where
    C: blockchain::jsonrpc::AccountInfo
        + blockchain::jsonrpc::SendTx
        + blockchain::jsonrpc::ViewContract
        + blockchain::jsonrpc::GasInfo,
    <C as blockchain::jsonrpc::SendTx>::Output: std::fmt::Display,
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
            let wrap_near = &blockchain::ref_finance::token_account::WNEAR_TOKEN;
            let wrap_near_in: TokenInAccount = wrap_near.to_in();
            let wrap_near_out: TokenOutAccount = wrap_near.to_out();

            // Step 1: token → wrap.near
            // common と backend の TokenAccount は同一型なので直接使用可能
            if token.to_string() != wrap_near.to_string() {
                let from_token = token.as_in();
                swap::execute_direct_swap(
                    client,
                    wallet,
                    &from_token,
                    &wrap_near_out,
                    None,
                    recorder,
                )
                .await?;
            }

            // Step 2: wrap.near → target
            if target.to_string() != wrap_near.to_string() {
                swap::execute_direct_swap(client, wallet, &wrap_near_in, target, None, recorder)
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
            swap::execute_direct_swap(client, wallet, &from_token, to, None, recorder).await?;

            debug!(log, "switch completed"; "from" => %from, "to" => %to);
            Ok(())
        }
        TradingAction::Rebalance { target_weights } => {
            // ポートフォリオのリバランス
            debug!(log, "executing rebalance"; "weights" => ?target_weights);

            // 現在の保有量を取得（wrap.nearを明示的に追加）
            // TokenOutAccount → String に変換
            let mut tokens: Vec<String> = target_weights.keys().map(|t| t.to_string()).collect();
            let wrap_near = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_string();
            if !tokens.contains(&wrap_near) {
                tokens.push(wrap_near.clone());
                trace!(
                    log,
                    "added wrap.near to balance query for total value calculation"
                );
            }
            trace!(log, "tokens list for balance query"; "tokens" => ?tokens, "count" => tokens.len());

            let current_balances =
                crate::swap::get_current_portfolio_balances(client, wallet, &tokens).await?;

            // 総ポートフォリオ価値を計算
            let total_portfolio_value =
                crate::swap::calculate_total_portfolio_value(client, wallet, &current_balances)
                    .await?;

            // Phase 1と2に分けてリバランスを実行
            // まず各トークンの差分（wrap.near換算）を計算
            use num_bigint::ToBigInt;

            let mut sell_operations: Vec<(String, NearValue, ExchangeRate)> = Vec::new();
            let mut buy_operations: Vec<(String, NearValue)> = Vec::new();

            let wrap_near_str = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_string();

            for (token, target_weight) in target_weights.iter() {
                // TokenOutAccount → String for comparison and HashMap access
                let token_str = token.to_string();

                // weight の有効性確認
                if !target_weight.is_finite() || *target_weight < 0.0 {
                    warn!(log, "invalid weight, skipping"; "token" => &token_str, "weight" => *target_weight);
                    continue;
                }

                if token_str == wrap_near_str {
                    continue; // wrap.nearは除外
                }

                let current_amount = current_balances.get(&token_str);

                // 現在の価値（wrap.near換算）を計算
                let current_value_wrap_near: NearValue = match current_amount {
                    Some(amount) if !amount.is_zero() => {
                        let token_out: TokenOutAccount =
                            token_str.parse::<near_sdk::AccountId>()?.into();
                        let quote_in: TokenInAccount =
                            wrap_near_str.parse::<near_sdk::AccountId>()?.into();

                        let get_decimals = crate::make_get_decimals();
                        let rate = persistence::token_rate::TokenRate::get_latest(
                            &token_out,
                            &quote_in,
                            &get_decimals,
                        )
                        .await?
                        .ok_or_else(|| anyhow::anyhow!("No rate found for token: {}", token_str))?;

                        // TokenAmount / &ExchangeRate = NearValue トレイトを使用
                        amount / &rate.exchange_rate
                    }
                    _ => NearValue::zero(),
                };

                // 目標価値（wrap.near換算）を計算
                // target_weight は f64 (0.0~1.0)、例: 0.3 = ポートフォリオの30%
                let target_value_wrap_near: NearValue = &total_portfolio_value * *target_weight;

                // 差分を計算（wrap.near単位）
                let diff_wrap_near: NearValue = &target_value_wrap_near - &current_value_wrap_near;

                trace!(log, "rebalancing: token analysis";
                    "token" => &token_str,
                    "current_value_wrap_near" => %current_value_wrap_near,
                    "target_value_wrap_near" => %target_value_wrap_near,
                    "diff_wrap_near" => %diff_wrap_near
                );

                // 最小交換額チェック（1 NEAR以上）
                let min_trade_size = NearValue::one();
                let zero = NearValue::zero();

                if diff_wrap_near < zero && diff_wrap_near.abs() >= min_trade_size {
                    // 売却が必要
                    let token_out: TokenOutAccount =
                        token_str.parse::<near_sdk::AccountId>()?.into();
                    let quote_in: TokenInAccount =
                        wrap_near_str.parse::<near_sdk::AccountId>()?.into();

                    let get_decimals = crate::make_get_decimals();
                    let rate = persistence::token_rate::TokenRate::get_latest(
                        &token_out,
                        &quote_in,
                        &get_decimals,
                    )
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("No rate found for token: {}", token_str))?;

                    sell_operations.push((
                        token_str.clone(),
                        diff_wrap_near.abs(),
                        rate.exchange_rate.clone(),
                    ));
                } else if diff_wrap_near > zero && diff_wrap_near >= min_trade_size {
                    // 購入が必要
                    buy_operations.push((token_str.clone(), diff_wrap_near));
                }
            }

            // 型安全なwrap.nearを事前に準備
            let wrap_near_token = &blockchain::ref_finance::token_account::WNEAR_TOKEN;
            let wrap_near_in: TokenInAccount = wrap_near_token.to_in();
            let wrap_near_out: TokenOutAccount = wrap_near_token.to_out();

            // Phase 1: 全ての売却を実行（token → wrap.near）
            debug!(log, "Phase 1: executing sell operations"; "count" => sell_operations.len());
            for (token, wrap_near_value, exchange_rate) in sell_operations {
                // wrap.near価値をトークン数量に変換
                // NearValue * ExchangeRate = TokenAmount
                // NOTE: 理論上、decimals が非常に小さく高価なトークン（例: decimals=0, 価格 > 1 NEAR/token）の場合、
                // smallest_units < 1 となり to_bigint() でゼロに切り捨てられる可能性がある。
                // ただし、NEAR エコシステムの標準は decimals=18〜24 であり、
                // REF Finance で取引可能なトークンは実質的に全て decimals≥6 のため、
                // このケースは発生しない。
                // 参考: test_small_rate_scaling_issue (execution/tests.rs:413)
                let token_amount: TokenAmount = &wrap_near_value * &exchange_rate;
                let token_amount_u128 = token_amount
                    .smallest_units()
                    .to_bigint()
                    .ok_or_else(|| anyhow::anyhow!("Failed to convert to BigInt"))?
                    .to_string()
                    .parse::<u128>()
                    .map_err(|e| anyhow::anyhow!("Failed to parse as u128: {}", e))?;

                trace!(log, "selling token";
                    "token" => &token,
                    "wrap_near_value" => %wrap_near_value,
                    "token_amount" => token_amount_u128
                );

                let from_token: TokenInAccount = token
                    .parse()
                    .map_err(|e| anyhow::anyhow!("Invalid token: {}", e))?;
                swap::execute_direct_swap(
                    client,
                    wallet,
                    &from_token,
                    &wrap_near_out,
                    Some(token_amount_u128),
                    recorder,
                )
                .await?;
            }

            // Phase 1完了後、利用可能なwrap.nearを確認し、Phase 2の購入額を調整
            let available_wrap_near = {
                let account = wallet.account_id();
                let deposits =
                    blockchain::ref_finance::deposit::get_deposits(client, account).await?;
                let wrap_near_account: TokenAccount =
                    wrap_near_str.parse::<near_sdk::AccountId>()?.into();
                deposits
                    .get(&wrap_near_account)
                    .map(|u| u.0)
                    .unwrap_or_default()
            };

            debug!(log, "Phase 1 completed, checking available wrap.near";
                "available_wrap_near" => %available_wrap_near
            );

            // available_wrap_near (u128) を NearValue に変換
            let available_wrap_near_value =
                YoctoValue::from_yocto(BigDecimal::from(available_wrap_near)).to_near();

            // Phase 2の購入操作の総額を計算
            let total_buy_value: NearValue = buy_operations
                .iter()
                .map(|(_, value)| value.clone())
                .fold(NearValue::zero(), |acc, v| acc + v);

            debug!(log, "Phase 2 purchase amount analysis";
                "total_buy_value" => %total_buy_value,
                "available_wrap_near_value" => %available_wrap_near_value
            );

            // 利用可能残高に基づいて購入額を調整
            let adjusted_buy_operations: Vec<(String, NearValue)> =
                if total_buy_value > available_wrap_near_value {
                    // 比率を計算して調整（型安全な除算演算子を使用）
                    let ratio = &available_wrap_near_value / &total_buy_value;
                    debug!(log, "Adjusting purchase amounts to fit available balance";
                        "adjustment_factor" => %ratio
                    );

                    buy_operations
                        .into_iter()
                        .map(|(token, value)| (token, &value * &ratio))
                        .collect()
                } else {
                    buy_operations
                };

            // Phase 2: 全ての購入を実行（wrap.near → token）
            debug!(log, "Phase 2: executing buy operations"; "count" => adjusted_buy_operations.len());

            let mut phase2_success = 0;
            let mut phase2_failed = 0;

            for (token, wrap_near_value) in adjusted_buy_operations {
                // NearValue → YoctoValue → YoctoAmount → u128 に変換
                let wrap_near_amount_u128 = wrap_near_value.to_yocto().to_amount().to_u128();

                // NOTE: wrap_near_amount_u128 == 0 は、調整後の値が 10^-24 NEAR 未満の場合に発生。
                // これは available_wrap_near が total_buy_value の 10^-24 未満の場合であり、
                // 実質的に残高がゼロに等しい状況。そのような状況ではリバランス自体が意味をなさないため、
                // スキップして継続する現在の実装で問題ない。
                if wrap_near_amount_u128 == 0 {
                    error!(log, "Failed to convert purchase amount to u128"; "token" => &token);
                    phase2_failed += 1;
                    continue;
                }

                trace!(log, "buying token";
                    "token" => &token,
                    "wrap_near_amount" => wrap_near_amount_u128
                );

                let to_token: TokenOutAccount = match token.parse() {
                    Ok(t) => t,
                    Err(e) => {
                        error!(log, "Failed to parse token"; "token" => &token, "error" => %e);
                        phase2_failed += 1;
                        continue;
                    }
                };

                match swap::execute_direct_swap(
                    client,
                    wallet,
                    &wrap_near_in,
                    &to_token,
                    Some(wrap_near_amount_u128),
                    recorder,
                )
                .await
                {
                    Ok(_) => {
                        trace!(log, "purchase completed successfully"; "token" => &token);
                        phase2_success += 1;
                    }
                    Err(e) => {
                        error!(log, "purchase failed"; "token" => &token, "error" => %e);
                        phase2_failed += 1;
                    }
                }
            }

            info!(log, "rebalance completed";
                "phase2_success" => phase2_success,
                "phase2_failed" => phase2_failed
            );

            // Phase 2で全ての購入が失敗した場合のみエラーを返す
            // NOTE: 部分失敗（phase2_success > 0 && phase2_failed > 0）の場合も Ok を返す設計。
            // 理由:
            // 1. 次回のクーロン実行で、現在のポートフォリオ残高から再計算してリバランスが試行される
            // 2. failed_count はログに出力されるが、ExecutionSummary としてアクションには使われていない
            // 3. 部分的にリバランスされた状態でも、次回実行で自然に修正される
            // 将来的にアラートやメトリクス収集が必要になった場合は、この条件の見直しを検討する。
            if phase2_success == 0 && phase2_failed > 0 {
                return Err(anyhow::anyhow!(
                    "All Phase 2 purchases failed ({} failed)",
                    phase2_failed
                ));
            }

            Ok(())
        }
        TradingAction::AddPosition { token, weight } => {
            // ポジション追加
            debug!(log, "adding position"; "token" => %token, "weight" => weight);

            // wrap.near → token へのswap
            // common と backend の TokenAccount は同一型なので直接使用可能
            let wrap_near = &blockchain::ref_finance::token_account::WNEAR_TOKEN;
            if token.to_string() != wrap_near.to_string() {
                let wrap_near_in: TokenInAccount = wrap_near.to_in();
                swap::execute_direct_swap(client, wallet, &wrap_near_in, token, None, recorder)
                    .await?;
            }

            debug!(log, "position added"; "token" => %token, "weight" => weight);
            Ok(())
        }
        TradingAction::ReducePosition { token, weight } => {
            // ポジション削減
            // common と backend の TokenAccount は同一型なので直接使用可能
            debug!(log, "reducing position"; "token" => %token, "weight" => weight);

            // token → wrap.near へのswap
            let wrap_near = &blockchain::ref_finance::token_account::WNEAR_TOKEN;
            if token.to_string() != wrap_near.to_string() {
                let from_token = token.as_in();
                let wrap_near_out: TokenOutAccount = wrap_near.to_out();
                swap::execute_direct_swap(
                    client,
                    wallet,
                    &from_token,
                    &wrap_near_out,
                    None,
                    recorder,
                )
                .await?;
            }

            debug!(log, "position reduced"; "token" => %token, "weight" => weight);
            Ok(())
        }
    }
}

/// 評価期間のチェックと管理
///
/// 戻り値: (period_id, is_new_period, selected_tokens, liquidated_balance)
/// - liquidated_balance: 清算が行われた場合の最終残高
pub(crate) async fn manage_evaluation_period(
    available_funds: YoctoAmount,
) -> Result<(String, bool, Vec<String>, Option<YoctoAmount>)> {
    let log = DEFAULT.new(o!("function" => "manage_evaluation_period"));

    // 設定ファイルから評価期間を読み込む（デフォルト: 10日）
    let evaluation_period_days = config::get("TRADE_EVALUATION_DAYS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(10);

    info!(log, "evaluation period configuration"; "days" => evaluation_period_days);

    // 最新の評価期間を取得
    let latest_period = EvaluationPeriod::get_latest_async().await?;

    match latest_period {
        Some(period) => {
            let now = Utc::now().naive_utc();
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
                let final_balance = liquidate_all_positions().await?;
                info!(log, "liquidated all positions"; "final_balance" => %final_balance);

                // 評価期間のパフォーマンスを計算してログ出力
                let initial_value = YoctoValue::from_yocto(period_initial_value);
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

                // TRADE_ENABLED をチェック
                let trade_enabled = config::get("TRADE_ENABLED")
                    .map(|v| v.to_lowercase() == "true")
                    .unwrap_or(false);

                if !trade_enabled {
                    info!(log, "trade disabled, not starting new period";
                        "final_balance" => %final_balance
                    );

                    // TRADE_UNWRAP_ON_STOP が有効な場合、wrap.near を NEAR に戻して送金
                    let unwrap_on_stop = config::get("TRADE_UNWRAP_ON_STOP")
                        .map(|v| v.to_lowercase() == "true")
                        .unwrap_or(false);

                    if unwrap_on_stop {
                        info!(log, "unwrap_on_stop enabled, executing unwrap and transfer");
                        if let Err(e) = unwrap_and_transfer_wnear(&log).await {
                            error!(log, "failed to unwrap and transfer"; "error" => %e);
                        }
                    }

                    // 空の period_id を返して停止を通知
                    return Ok((String::new(), false, vec![], Some(final_balance)));
                }

                // 新規評価期間を作成
                let new_period =
                    NewEvaluationPeriod::new(final_value.as_bigdecimal().clone(), vec![]);
                let created_period = new_period.insert_async().await?;

                info!(log, "created new evaluation period";
                    "period_id" => %created_period.period_id,
                    "initial_value" => %created_period.initial_value
                );

                Ok((created_period.period_id, true, vec![], Some(final_balance)))
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

                let selected_tokens = period_selected_tokens
                    .unwrap_or_default()
                    .into_iter()
                    .flatten()
                    .collect();

                // トランザクションがゼロなら新規期間として扱う
                let is_new_period = transaction_count == 0;

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

            let initial_value = available_funds.as_bigdecimal().clone();
            let new_period = NewEvaluationPeriod::new(initial_value.clone(), vec![]);
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
) -> Vec<String> {
    deposits
        .iter()
        .filter_map(|(token, amount)| {
            // wrap.nearは除外し、残高があるトークンのみを対象とする
            if token != wrap_near_token && amount.0 > 0 {
                Some(token.to_string())
            } else {
                None
            }
        })
        .collect()
}

/// 全保有トークンをwrap.nearに売却
///
/// 戻り値: 売却後のwrap.near総額 (yoctoNEAR)
pub(crate) async fn liquidate_all_positions() -> Result<YoctoAmount> {
    let log = DEFAULT.new(o!("function" => "liquidate_all_positions"));

    // 最新の評価期間を取得
    let latest_period = EvaluationPeriod::get_latest_async().await?;
    let period_id = match latest_period {
        Some(period) => {
            // selected_tokensは履歴として記録（実際の清算には使用しない）
            let selected_tokens = period
                .selected_tokens
                .unwrap_or_default()
                .into_iter()
                .flatten()
                .collect::<Vec<String>>();
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
    let client = blockchain::jsonrpc::new_client();
    let wallet = blockchain::wallet::new_wallet();
    let account = wallet.account_id();
    let wrap_near_token = &blockchain::ref_finance::token_account::WNEAR_TOKEN;

    let deposits = blockchain::ref_finance::deposit::get_deposits(&client, account).await?;
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
        trace!(log, "liquidating token"; "token" => token);

        // トークンの残高を再確認（取得時点から変更がある可能性を考慮）
        let token_account: TokenAccount = token
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid token: {}", e))?;

        // トークンの REF Finance 上の残高を取得
        let account = wallet.account_id();
        let deposits = blockchain::ref_finance::deposit::get_deposits(&client, account).await?;
        let balance = deposits
            .get(&token_account)
            .map(|u| u.0)
            .unwrap_or_default();

        if balance == 0 {
            trace!(log, "token balance became zero, skipping"; "token" => token);
            continue;
        }

        // token → wrap.near にスワップ
        let from_token: TokenInAccount = token
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid token: {}", e))?;
        match swap::execute_direct_swap(
            &client,
            &wallet,
            &from_token,
            &wrap_near_out,
            None,
            &recorder,
        )
        .await
        {
            Ok(_) => {
                trace!(log, "successfully liquidated token"; "token" => token);
            }
            Err(e) => {
                error!(log, "failed to liquidate token"; "token" => token, "error" => ?e);
                // エラーが発生しても他のトークンの売却は継続
            }
        }
    }

    // 最終的なwrap.near残高を取得
    let account = wallet.account_id();
    let deposits = blockchain::ref_finance::deposit::get_deposits(&client, account).await?;
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
async fn unwrap_and_transfer_wnear(log: &slog::Logger) -> Result<()> {
    use blockchain::jsonrpc::{AccountInfo, SendTx, SentTx};
    use blockchain::ref_finance::{deposit, token_account::WNEAR_TOKEN};
    use common::types::{NearAmount, YoctoAmount};

    // HARVEST_ACCOUNT_ID を取得（未設定の場合はスキップ）
    let harvest_account_id = match config::get("HARVEST_ACCOUNT_ID") {
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
    let reserve_amount: YoctoAmount = config::get("HARVEST_RESERVE_AMOUNT")
        .ok()
        .and_then(|v| v.parse::<NearAmount>().ok())
        .unwrap_or_else(|| "1".parse().expect("valid NearAmount literal"))
        .to_yocto();
    let reserve_amount_u128: u128 = reserve_amount.to_u128();

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
