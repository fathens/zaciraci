//! 取引戦略オーケストレータモジュール
//!
//! ポートフォリオベースの取引戦略のエントリポイント。
//! 資金準備、トークン選定、ポートフォリオ最適化、取引実行の
//! ワークフロー全体を統括する。
//!
//! ## 単位の規約
//!
//! このモジュールでは以下の単位規約を使用しています：
//!
//! - **rate_yocto**: yocto tokens per 1 NEAR（DB に保存される形式）
//! - **predictions (BTreeMap<String, TokenPrice>)**: 予測価格（NEAR/token 単位、型安全）
//! - **volatility**: 比率（単位なし）
//!
//! ## yocto スケールの利点
//!
//! DBには `rate_yocto = tokens_yocto / NEAR` を保存。
//! これにより使用時のスケーリング（× 10^24）が不要になり効率的。
//!
//! ## 単位変換（型安全）
//!
//! - NEAR → yoctoNEAR: `NearValue::from_near(bd).to_yocto().as_bigdecimal()`
//! - yoctoNEAR → NEAR: `YoctoValue::from_yocto(bd).to_near().as_bigdecimal()`

use crate::Result;
use crate::candidate_telemetry;
use crate::predict::PredictionService;
use crate::swap;
use bigdecimal::{BigDecimal, ToPrimitive, Zero};
use blockchain::jsonrpc::{AccountInfo, GasInfo, SendTx, ViewContract};
use blockchain::wallet::Wallet;
use common::algorithm::{
    portfolio::{PortfolioData, execute_portfolio_optimization},
    types::{TokenData, TradingAction, WalletInfo},
};
use common::config::ConfigAccess;
use common::types::{
    ExchangeRate, NearAmount, NearValue, TokenAmount, TokenPrice, YoctoAmount, YoctoValue,
};
use common::types::{TokenAccount, TokenInAccount, TokenOutAccount};
use futures::stream::{self, StreamExt};
use logging::*;
use near_sdk::{AccountId, NearToken};
use persistence::evaluation_period::EvaluationPeriod;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Display;
use std::sync::Arc;

use super::execution::{
    execute_trading_actions, liquidate_all_positions, manage_evaluation_period,
};
use super::market_data::{
    calculate_enhanced_liquidity_score, calculate_volatility_from_history,
    estimate_market_cap_async,
};
use super::portfolio_cost::{CostAwareOutcome, collect_cost_inputs, run_cost_aware_optimization};

pub async fn start<C, W>(
    client: &C,
    wallet: &W,
    current_time: chrono::DateTime<chrono::Utc>,
    cfg: &impl ConfigAccess,
) -> Result<()>
where
    C: AccountInfo + SendTx + ViewContract + GasInfo,
    <C as SendTx>::Output: Display + blockchain::jsonrpc::SentTx,
    W: Wallet,
{
    let log = DEFAULT.new(o!("function" => "trade::start"));

    info!(log, "starting portfolio-based trading strategy");

    // TRADE_ENABLED のチェック
    let trade_enabled = cfg.trade_enabled();

    // Step 1: 評価期間のチェックと管理（清算が必要な場合は先に実行）
    // 初回起動時は available_funds=0 で呼び出し、後で prepare_funds() で資金準備
    let result =
        manage_evaluation_period(client, wallet, current_time, YoctoAmount::zero(), cfg).await?;
    info!(log, "evaluation period status";
        "period_id" => %result.period_id,
        "is_new_period" => result.is_new_period,
        "existing_tokens_count" => result.existing_tokens.len(),
        "liquidated_balance" => ?result.liquidated_balance,
        "trade_enabled" => trade_enabled
    );

    // period_id が空の場合は清算のみで終了（manage_evaluation_period で停止された）
    if result.period_id.is_empty() {
        info!(log, "trade stopped after liquidation (TRADE_ENABLED=false)");
        return Ok(());
    }

    // 取引が無効化されている場合
    if !trade_enabled {
        if result.is_new_period {
            info!(log, "trade disabled, skipping new period");
            return Ok(());
        } else {
            // 評価期間中: 清算して終了
            info!(log, "trade disabled, liquidating positions");
            let _ = liquidate_all_positions(client, wallet, cfg).await?;
            return Ok(());
        }
    }

    // Step 2: 資金準備（新規期間で清算がなかった場合のみ）
    let available_funds: YoctoAmount = if result.is_new_period {
        if let Some(balance) = result.liquidated_balance {
            // 清算が行われた場合: 清算後の残高をそのまま使用
            debug!(log, "Using liquidated balance for new period"; "available_funds" => %balance);
            if balance.is_zero() {
                info!(log, "no funds available after liquidation");
                return Ok(());
            }
            balance
        } else {
            // 初回起動: NEAR -> wrap.near 変換
            let funds = prepare_funds(client, wallet, cfg).await?;
            debug!(log, "Prepared funds for new period"; "available_funds" => %funds);

            if funds.is_zero() {
                info!(log, "no funds available for trading");
                return Ok(());
            }

            funds
        }
    } else {
        // 評価期間中: 既存トークンを継続使用、追加の資金準備は不要
        debug!(log, "continuing evaluation period, skipping prepare_funds");
        YoctoAmount::zero() // available_funds は使用されない
    };

    // Step 3: PredictionServiceの初期化
    let prediction_service = PredictionService::new(cfg)?;

    // 清算失敗トークンがあればログ出力
    if !result.failed_liquidations.is_empty() {
        warn!(log, "some tokens failed to liquidate, will be retried next period";
            "failed_count" => result.failed_liquidations.len(),
            "failed_tokens" => ?result.failed_liquidations
        );
    }

    // result を分解（existing_tokens は into_iter で消費するため先に取り出す）
    let period_id = result.period_id;
    let is_new_period = result.is_new_period;
    let existing_tokens = result.existing_tokens;

    // pool_info を 1 サイクルにつき 1 度だけ snapshot し、以降の処理は
    // すべてこの Arc を共有することで TOCTOU を排除する
    // （F008 / F025）。同じスナップショットを `select_top_volatility_tokens`
    // と `execute_portfolio_strategy` に渡すことで、ボラティリティ判定と
    // コスト見積りが同一プール状態を観測することを保証する。
    //
    // `current_time` を渡すことで simulate でも当日のプール状態を読み、
    // production も `Utc::now()` 同等の最新スナップショットを読む。
    // 旧コードの `None` は production では「最新」を意味して問題なかったが、
    // simulate では「最新」がテスト DB に流入した sim 期間外のデータを指して
    // しまい、コスト推定と swap 実行（mock_client.rs::handle_swap で
    // `Some(sim_day)` を使用）の時刻が乖離する原因になっていた。
    let pool_snapshot =
        persistence::pool_info::read_from_db(Some(current_time.naive_utc())).await?;

    // Step 4: トークン選定 (フラグでモード分岐)
    //
    // `held` は all-token モードの候補生成だけでなく、後段の storage `keep` リスト
    // でも使われる。`fetch_held_tokens` は両モードで安全に動作する
    // (`is_new_period=true` で empty、それ以外は DB snapshot → RPC fallback)。
    let all_predicted_enabled = cfg.trade_all_predicted_enabled();
    let held =
        fetch_held_tokens(client, wallet, &period_id, is_new_period, &existing_tokens).await?;

    let (selected_tokens, all_predicted_pre_filter_count) = if all_predicted_enabled {
        // All-token モード: 全予測トークン + 現在保有トークン union を毎サイクル算出
        let (candidates, pre_filter_count) = select_all_predicted_candidates(
            &prediction_service,
            current_time,
            cfg,
            &pool_snapshot,
            &held,
        )
        .await?;

        // 毎サイクル DB に書き込む。`manage_evaluation_period` の
        // 「selected_tokens empty + transactions exist → 破損疑い」検知ロジックが
        // 引き続き機能するよう、空でない候補集合を都度 selected_tokens 列に反映する。
        if !candidates.is_empty() {
            let token_strs: Vec<String> = candidates.iter().map(|t| t.to_string()).collect();
            match EvaluationPeriod::update_selected_tokens_async(period_id.clone(), token_strs)
                .await
            {
                Ok(_) => {
                    debug!(log, "updated selected tokens (all-token mode)";
                        "count" => candidates.len(),
                        "held_count" => held.len());
                }
                Err(e) => {
                    error!(log, "failed to update selected tokens (all-token mode)";
                        "error" => ?e);
                }
            }
        }

        (candidates, Some(pre_filter_count))
    } else if is_new_period {
        // Legacy モード (新規期間): 期間最初にトップ N ボラティリティトークンを固定
        let tokens =
            select_top_volatility_tokens(&prediction_service, current_time, cfg, &pool_snapshot)
                .await?;

        // 選定したトークンをデータベースに保存
        if !tokens.is_empty() {
            let token_strs: Vec<String> = tokens.iter().map(|t| t.to_string()).collect();
            match EvaluationPeriod::update_selected_tokens_async(period_id.clone(), token_strs)
                .await
            {
                Ok(_) => {
                    debug!(log, "updated selected tokens in database"; "count" => tokens.len());
                }
                Err(e) => {
                    error!(log, "failed to update selected tokens"; "error" => ?e);
                }
            }
        }

        (tokens, None)
    } else {
        // Legacy モード (評価期間中): 期間最初に固定したトークンを継続使用
        let tokens: Vec<AccountId> = existing_tokens
            .iter()
            .cloned()
            .map(AccountId::from)
            .collect();
        (tokens, None)
    };

    debug!(log, "Selected tokens"; "count" => selected_tokens.len(), "is_new_period" => is_new_period);

    if selected_tokens.is_empty() {
        info!(log, "no tokens selected for trading");
        return Ok(());
    }

    // Step 5: ポートフォリオ最適化を先に実行する。
    //
    // 旧フローでは storage setup → deposit → 最適化 の順だったが、all-token モードで
    // 候補が ~290 になると `MAX_REGISTER_PER_CYCLE = 100` を超えて storage setup が
    // 全 cycle で失敗していた。最適化はオンチェーン状態を変更しない (deposit 読みのみ)
    // ので、先に走らせて action に必要なトークンのみ register することで cap を回避する。
    debug!(log, "executing portfolio optimization";
        "is_new_period" => is_new_period,
        "token_count" => selected_tokens.len()
    );

    let params = PortfolioStrategyParams {
        prediction_service: &prediction_service,
        tokens: &selected_tokens,
        available_funds: available_funds.clone(),
        is_new_period,
        period_id: &period_id,
        end_date: current_time,
        cfg,
        pools: &pool_snapshot,
        all_predicted_pre_filter_count,
    };
    let (actions, expected_returns) =
        match execute_portfolio_strategy(&params, client, wallet).await {
            Ok(result) => result,
            Err(e) => {
                error!(log, "failed to execute portfolio strategy"; "error" => ?e);
                return Err(e);
            }
        };

    info!(log, "portfolio optimization completed";
        "action_count" => actions.len()
    );

    // Step 6: action から storage 登録に必要なトークン集合を導出する。
    //
    // - `needed_tokens`: action が触るトークン (register 対象)。WNEAR は
    //   `keep_with_portfolio` で自動付与されるため明示しなくてよい。
    // - `keep`: register 後に保持し続けたいトークン全体 (= needed ∪ held)。
    //   不要な銘柄は planner が unregister 対象に回す。
    let needed_tokens = extract_token_accounts_from_actions(&actions);
    let mut keep_input = needed_tokens.clone();
    for h in &held {
        let acc = h.inner().clone();
        if !keep_input.contains(&acc) {
            keep_input.push(acc);
        }
    }
    let keep = blockchain::ref_finance::storage::keep_with_portfolio(&keep_input);

    debug!(log, "ensuring REF Finance storage setup";
        "needed_count" => needed_tokens.len(),
        "keep_count" => keep.len(),
        "held_count" => held.len()
    );
    let max_top_up = blockchain::ref_finance::storage::max_top_up_from_config(cfg);
    blockchain::ref_finance::storage::ensure_ref_storage_setup(
        client,
        wallet,
        &needed_tokens,
        &keep,
        max_top_up,
    )
    .await?;
    debug!(log, "REF Finance storage setup completed");

    // Step 7: 投資額全額を REF Finance にデポジット (新規期間のみ)
    // WNEAR は `keep` に必ず含まれるため、ここまでで register 済み。
    if is_new_period {
        debug!(log, "depositing initial investment to REF Finance"; "amount" => %available_funds);
        blockchain::ref_finance::balances::deposit_wrap_near_to_ref(
            client,
            wallet,
            NearToken::from_yoctonear(available_funds.to_u128()),
            cfg,
        )
        .await?;
        debug!(log, "initial investment deposited to REF Finance");
    }

    // Step 8: 実際の取引実行
    let executed_actions = execute_trading_actions(
        client,
        wallet,
        &actions,
        period_id.clone(),
        cfg,
        &expected_returns,
    )
    .await?;
    info!(log, "trades executed"; "success" => executed_actions.success_count, "failed" => executed_actions.failed_count);

    // ポートフォリオ保有量を記録。keep には needed ∪ held ∪ WNEAR が含まれるため
    // 現サイクル末時点の保有可能集合を網羅する。
    if let Err(e) =
        super::snapshot::record_portfolio_holdings(client, wallet, &period_id, &keep, current_time)
            .await
    {
        warn!(log, "failed to record portfolio holdings"; "error" => ?e);
    }

    // 注: ハーベスト判定は manage_evaluation_period 内で評価期間終了時（清算後・新period作成前）に
    // 自動実行される。旧 period の initial_value と清算額で正しく比較するため。

    info!(log, "success");
    Ok(())
}

/// 資金準備 (NEAR -> wrap.near 変換)
async fn prepare_funds<C, W>(client: &C, wallet: &W, cfg: &impl ConfigAccess) -> Result<YoctoAmount>
where
    C: AccountInfo + SendTx + ViewContract + GasInfo,
    <C as SendTx>::Output: Display + blockchain::jsonrpc::SentTx,
    W: Wallet,
{
    let log = DEFAULT.new(o!("function" => "prepare_funds"));

    // 初期投資額の設定値を取得（NEAR単位で入力、yoctoNEARに変換）
    let target_investment: YoctoAmount = cfg
        .trade_initial_investment()
        .to_string()
        .parse::<NearAmount>()
        .unwrap()
        .to_yocto();

    // 必要な wrap.near 残高として投資額を設定（NEAR -> wrap.near変換）
    // アカウントには10 NEARを残し、それ以外を wrap.near に変換
    let required_balance = NearToken::from_yoctonear(target_investment.to_u128());
    let account_id = wallet.account_id();
    let balance = blockchain::ref_finance::balances::start(
        client,
        wallet,
        &blockchain::ref_finance::token_account::WNEAR_TOKEN,
        Some(required_balance),
        cfg,
    )
    .await?;

    // wrap.near の全額が投資可能
    // 設定された投資額と実際の残高の小さい方を使用
    let balance_amount = YoctoAmount::from_u128(balance.as_yoctonear());
    let available_funds = if balance_amount < target_investment {
        balance_amount
    } else {
        target_investment
    };

    if available_funds.is_zero() {
        return Err(anyhow::anyhow!(
            "Insufficient wrap.near balance for trading: {} yoctoNEAR",
            balance.as_yoctonear()
        ));
    }

    debug!(log, "prepared funds";
        "account" => %account_id,
        "wrap_near_balance" => balance.as_yoctonear(),
        "available_funds" => %available_funds
    );

    Ok(available_funds)
}

/// トップボラティリティトークンの選定 (PredictionServiceを使用)
///
/// `pools` は呼び出し側で取得した pool_info snapshot を共有する。
/// 同一サイクル内で `pool_info` を二重に読まない (TOCTOU 解消) ため。
///
/// `TRADE_ALL_PREDICTED_ENABLED=true` の場合は呼ばれない (代わりに
/// `select_all_predicted_candidates` を使用)。crate 外からの呼び出しは
/// 想定していないため `pub(crate)`。
pub(crate) async fn select_top_volatility_tokens(
    prediction_service: &PredictionService,
    end_date: chrono::DateTime<chrono::Utc>,
    cfg: &impl ConfigAccess,
    pools: &Arc<dex::PoolInfoList>,
) -> Result<Vec<AccountId>> {
    let limit = cfg.trade_top_tokens() as usize;
    select_volatility_tokens_inner(prediction_service, end_date, cfg, Some(limit), pools).await
}

/// 全予測トークン + 現在保有トークンを union した候補集合を返す (all-token モード用)。
///
/// `select_top_volatility_tokens` との違い:
/// - 上位 N トークンへの切り詰めなし (limit=None と同等)
/// - 流動性 / グラフ到達性フィルタを通らない保有トークンも候補に保持 (sell-only)
///
/// 戻り値の `Vec<AccountId>` は最適化器に渡される候補集合。
/// `execute_portfolio_optimization` 内で `current_weights > HELD_DUST_THRESHOLD`
/// な銘柄に sell-only 制約が適用される (C1)。
pub(crate) async fn select_all_predicted_candidates(
    prediction_service: &PredictionService,
    end_date: chrono::DateTime<chrono::Utc>,
    cfg: &impl ConfigAccess,
    pools: &Arc<dex::PoolInfoList>,
    held: &BTreeSet<TokenOutAccount>,
) -> Result<(Vec<AccountId>, usize)> {
    let log = DEFAULT.new(o!("function" => "select_all_predicted_candidates"));

    let price_history_days = i64::from(cfg.trade_price_history_days());
    let start_date = end_date - chrono::TimeDelta::days(price_history_days);
    let quote_token: TokenInAccount = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_in();

    // 1) ボラティリティが計算可能な全トークン (まだフィルタ前)
    let top_tokens = prediction_service
        .get_tokens_by_volatility(start_date, end_date, &quote_token)
        .await?;

    let mut tokens: Vec<AccountId> = top_tokens
        .into_iter()
        .map(|token| token.token.into())
        .collect();

    // 2) 保有トークンを候補に union (volatility に出てこない銘柄でも sell 経路を確保)
    for h in held {
        let acc: AccountId = h.as_account_id().clone();
        if !tokens.contains(&acc) {
            tokens.push(acc);
        }
    }

    if tokens.is_empty() {
        return Err(anyhow::anyhow!(
            "No candidate tokens (volatility data empty and no holdings)"
        ));
    }

    let pre_filter_count = tokens.len();
    debug!(log, "candidate tokens before hard filter";
        "count" => pre_filter_count, "held" => held.len());

    // 3) hard filter (流動性 + グラフ到達性)。保有は無条件保持。
    let min_liquidity = NearValue::from_near(BigDecimal::from(cfg.trade_min_pool_liquidity()));
    let wnear = blockchain::ref_finance::token_account::WNEAR_TOKEN.clone();
    let wnear_in: TokenInAccount = wnear.to_in();
    let latest_rates = persistence::token_rate::get_all_latest_rates(&wnear).await?;

    let filtered = hard_filter_tokens_keeping_held(
        tokens,
        pools,
        &latest_rates,
        &wnear,
        &wnear_in,
        &min_liquidity,
        held,
    )?;
    Ok((filtered, pre_filter_count))
}

/// `TradingAction` リストから storage 登録が必要な `TokenAccount` 集合を抽出する。
///
/// `start()` の post-opt フローで使用: 最適化が出力した action から実際に
/// 触るトークンだけを取り出して `ensure_ref_storage_setup` の
/// `needed_tokens` に渡すことで、`MAX_REGISTER_PER_CYCLE = 100` の cap を
/// 候補集合のサイズ (all-token モードで ~290) と切り離す。
///
/// WNEAR は `keep_with_portfolio` 側で自動付与されるため、本関数では含めない。
/// `Hold` variant は何も追加しない。`Rebalance::target_weights` のキー、
/// `Sell::{token,target}`、`Switch::{from,to}`、`AddPosition::token`、
/// `ReducePosition::token` をすべて union 化し、`BTreeSet` で dedup する。
fn extract_token_accounts_from_actions(actions: &[TradingAction]) -> Vec<TokenAccount> {
    let mut set: BTreeSet<TokenAccount> = BTreeSet::new();
    for action in actions {
        match action {
            TradingAction::Hold => {}
            TradingAction::Sell { token, target } => {
                set.insert(token.inner().clone());
                set.insert(target.inner().clone());
            }
            TradingAction::Switch { from, to } => {
                set.insert(from.inner().clone());
                set.insert(to.inner().clone());
            }
            TradingAction::Rebalance { target_weights } => {
                for token in target_weights.keys() {
                    set.insert(token.inner().clone());
                }
            }
            TradingAction::AddPosition { token, .. } => {
                set.insert(token.inner().clone());
            }
            TradingAction::ReducePosition { token, .. } => {
                set.insert(token.inner().clone());
            }
        }
    }
    set.into_iter().collect()
}

/// `start()` 用の保有トークン取得ヘルパー (all-token モードで使用)。
///
/// - 新規期間 (`is_new_period=true`): 直前に清算済みのため空集合を返す。
/// - 評価期間中:
///   1. `record_portfolio_holdings` が書き込んだ DB snapshot を優先 (最新の正)。
///   2. snapshot がなければ前サイクルの `existing_tokens` を補助情報として RPC fallback。
///   3. RPC でも取れなければ空集合 (ログ警告のみ)。
///
/// 戻り値の `BTreeSet<TokenOutAccount>` は wnear / ゼロ残高を除外済み。
async fn fetch_held_tokens<C, W>(
    client: &C,
    wallet: &W,
    period_id: &str,
    is_new_period: bool,
    fallback_tokens: &[TokenAccount],
) -> Result<BTreeSet<TokenOutAccount>>
where
    C: AccountInfo + SendTx + ViewContract + GasInfo,
    <C as SendTx>::Output: Display + blockchain::jsonrpc::SentTx,
    W: Wallet,
{
    let log = DEFAULT.new(o!("function" => "fetch_held_tokens"));

    if is_new_period {
        debug!(log, "new period, held is empty");
        return Ok(BTreeSet::new());
    }

    let wnear = &*blockchain::ref_finance::token_account::WNEAR_TOKEN;

    let balances = match super::snapshot::get_holdings_from_db(period_id).await? {
        Some(h) => {
            debug!(log, "held tokens from DB snapshot");
            h
        }
        None => {
            if fallback_tokens.is_empty() {
                warn!(log, "no DB snapshot and no fallback tokens, held treated as empty";
                    "period_id" => period_id);
                return Ok(BTreeSet::new());
            }
            let mut token_accounts: Vec<TokenAccount> = fallback_tokens.to_vec();
            super::snapshot::ensure_wnear_included(&mut token_accounts);
            match swap::get_current_portfolio_balances(client, wallet, &token_accounts).await {
                Ok(b) => {
                    debug!(log, "held tokens from RPC fallback");
                    b
                }
                Err(e) => {
                    warn!(log, "RPC fallback for held tokens failed, treating as empty";
                        "error" => ?e);
                    return Ok(BTreeSet::new());
                }
            }
        }
    };

    Ok(balances
        .into_iter()
        .filter(|(token, amount)| token != wnear && !amount.is_zero())
        .map(|(token, _)| TokenOutAccount::from(token))
        .collect())
}

/// 全対象トークンの予測用リストを生成（流動性フィルタ適用、上限なし）
///
/// `select_top_volatility_tokens()` と同じフィルタ（ボラティリティ＋流動性＋グラフ到達性）
/// を適用するが、上位N個への切り詰めを行わず全対象を返す。
/// 予測フェーズで全対象トークンの価格予測を実行するために使用。
///
/// `pools` は呼び出し側で取得した pool_info snapshot を共有する。
pub(crate) async fn select_prediction_target_tokens(
    prediction_service: &PredictionService,
    end_date: chrono::DateTime<chrono::Utc>,
    cfg: &impl ConfigAccess,
    pools: &Arc<dex::PoolInfoList>,
) -> Result<Vec<AccountId>> {
    select_volatility_tokens_inner(prediction_service, end_date, cfg, None, pools).await
}

/// ボラティリティトークン選定の共通ロジック
///
/// ボラティリティ順にトークンを取得し、流動性フィルタ＋グラフ到達性フィルタを適用。
/// `limit` が `Some(n)` なら上位N個に切り詰め、`None` なら全件返す。
///
/// `pools` は呼び出し側で 1 サイクル中に 1 度だけ取得した snapshot。
/// 内部で再度 `pool_info::read_from_db` を呼ばず、TOCTOU を排除する。
async fn select_volatility_tokens_inner(
    prediction_service: &PredictionService,
    end_date: chrono::DateTime<chrono::Utc>,
    cfg: &impl ConfigAccess,
    limit: Option<usize>,
    pools: &Arc<dex::PoolInfoList>,
) -> Result<Vec<AccountId>> {
    let log = DEFAULT.new(o!("function" => "select_volatility_tokens"));

    let price_history_days = i64::from(cfg.trade_price_history_days());
    let start_date = end_date - chrono::TimeDelta::days(price_history_days);

    let quote_token: TokenInAccount = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_in();

    let top_tokens = prediction_service
        .get_tokens_by_volatility(start_date, end_date, &quote_token)
        .await?;

    let tokens: Vec<AccountId> = top_tokens
        .into_iter()
        .map(|token| token.token.into())
        .collect();

    if tokens.is_empty() {
        return Err(anyhow::anyhow!(
            "No volatility tokens returned from prediction service"
        ));
    }

    debug!(log, "volatility tokens selected"; "count" => tokens.len(), "limit" => ?limit);

    let min_liquidity = NearValue::from_near(BigDecimal::from(cfg.trade_min_pool_liquidity()));
    let wnear = blockchain::ref_finance::token_account::WNEAR_TOKEN.clone();
    let wnear_in: TokenInAccount = wnear.to_in();
    let latest_rates = persistence::token_rate::get_all_latest_rates(&wnear).await?;

    apply_liquidity_filter_and_select(
        tokens,
        pools,
        &latest_rates,
        &wnear,
        &wnear_in,
        &min_liquidity,
        limit,
    )
}

/// ポートフォリオ戦略実行のパラメータ
pub(crate) struct PortfolioStrategyParams<'a, Cfg: ConfigAccess> {
    pub(crate) prediction_service: &'a PredictionService,
    pub(crate) tokens: &'a [AccountId],
    pub(crate) available_funds: YoctoAmount,
    pub(crate) is_new_period: bool,
    pub(crate) period_id: &'a str,
    pub(crate) end_date: chrono::DateTime<chrono::Utc>,
    pub(crate) cfg: &'a Cfg,
    /// 1 サイクル中に 1 度だけ取得した pool_info snapshot。
    /// `select_top_volatility_tokens` と `collect_cost_inputs` で同一 snapshot を
    /// 共有することで TOCTOU を排除する（F008 / F025）。
    pub(crate) pools: &'a Arc<dex::PoolInfoList>,
    /// all-token モードで `select_all_predicted_candidates` が hard filter
    /// 適用前に持っていた候補数 (predicted ∪ held)。`Some` のときだけ
    /// telemetry funnel をログ出力する (legacy mode は `None` で静音)。
    pub(crate) all_predicted_pre_filter_count: Option<usize>,
}

/// ポートフォリオ戦略の実行
///
/// # 内部の単位
/// * 価格: Price型（無次元比率）をスケーリング（× 10^24）してu128に格納
/// * 予測: 同じスケーリング済みf64値
pub(crate) async fn execute_portfolio_strategy<C, W, Cfg>(
    params: &PortfolioStrategyParams<'_, Cfg>,
    client: &C,
    wallet: &W,
) -> Result<(Vec<TradingAction>, BTreeMap<TokenOutAccount, f64>)>
where
    C: blockchain::jsonrpc::ViewContract
        + blockchain::jsonrpc::AccountInfo
        + blockchain::jsonrpc::SendTx
        + blockchain::jsonrpc::GasInfo,
    W: blockchain::wallet::Wallet,
    Cfg: ConfigAccess,
{
    let prediction_service = params.prediction_service;
    let tokens = params.tokens;
    let available_funds = &params.available_funds;
    let is_new_period = params.is_new_period;
    let period_id = params.period_id;
    let end_date = params.end_date;
    let cfg = params.cfg;
    let log = DEFAULT.new(o!("function" => "execute_portfolio_strategy"));

    // Telemetry: 全ステージの token 数を funnel 形式で記録する。各段階の
    // 値は計算可能になった時点で代入し、最後に all-token モードでのみ
    // まとめてログ出力する (legacy mode は funnel ログを出さない)。
    //
    // - `predicted`: hard filter 適用前の候補集合サイズ
    //   (`select_all_predicted_candidates` で計測 / legacy mode では tokens.len() と同値)
    // - `after_liquidity`: hard filter 通過後 (= tokens.len())
    let mut funnel = candidate_telemetry::CandidateFunnel {
        predicted: params
            .all_predicted_pre_filter_count
            .unwrap_or(tokens.len()),
        after_liquidity: tokens.len(),
        ..Default::default()
    };

    // ポートフォリオデータの準備
    let mut predictions: BTreeMap<common::types::TokenOutAccount, TokenPrice> = BTreeMap::new();

    // 型安全な quote_token をループ外で事前に準備（最適化）
    let quote_token_in: TokenInAccount =
        blockchain::ref_finance::token_account::WNEAR_TOKEN.to_in();

    // 設定を事前に取得
    let price_history_days = i64::from(cfg.trade_price_history_days());

    // 1. DB から最新の予測を読み取り（run_predictions() で事前に保存済み）
    let token_out_list: Vec<TokenOutAccount> = tokens.iter().map(|t| t.clone().into()).collect();
    let db_predictions =
        persistence::prediction_record::PredictionRecord::get_latest_fresh_predictions(
            &token_out_list,
            end_date.naive_utc(),
        )
        .await?;

    let mut batch_predictions: BTreeMap<TokenOutAccount, TokenPrice> = BTreeMap::new();
    let mut parse_failures = 0u32;
    for r in db_predictions {
        match r.token.parse::<TokenAccount>() {
            Ok(account) => {
                let token: TokenOutAccount = account.into();
                batch_predictions.insert(token, TokenPrice::from_near_per_token(r.predicted_price));
            }
            Err(e) => {
                warn!(log, "failed to parse token from prediction record"; "token" => &r.token, "error" => %e);
                parse_failures += 1;
            }
        }
    }
    if parse_failures > 0 {
        warn!(log, "skipped predictions with unparseable tokens"; "count" => parse_failures);
    }

    if batch_predictions.is_empty() {
        warn!(log, "no fresh predictions available for any token";
            "token_count" => token_out_list.len(),
        );
        return Err(anyhow::anyhow!("No fresh predictions available"));
    }

    debug!(log, "predictions loaded from DB"; "count" => batch_predictions.len());

    // 2. 並行実行数を設定から取得
    let concurrency = cfg.trade_prediction_concurrency() as usize;

    // 3. 価格履歴の期間を計算
    let start_date = end_date - chrono::TimeDelta::days(price_history_days);

    // 4. 各トークンの価格履歴・予測価格を並行取得
    let history_futures: Vec<_> = token_out_list
        .iter()
        .map(|token_out| {
            let log = log.clone();
            let quote_token_in = quote_token_in.clone();
            let token_out = token_out.clone();
            let predicted_price = batch_predictions.get(&token_out).cloned();
            async move {
                let token_str = token_out.to_string();

                // 予測がないトークンはスキップ
                let predicted_price = match predicted_price {
                    Some(p) => p,
                    None => {
                        debug!(log, "no prediction available, skipping"; "token" => %token_str);
                        return None;
                    }
                };

                // 価格履歴の取得
                let history_result = prediction_service
                    .get_price_history(&token_out, &quote_token_in, start_date, end_date)
                    .await;

                let history = match history_result {
                    Ok(hist) => hist,
                    Err(e) => {
                        error!(log, "failed to get price history for token"; "token" => %token_str, "error" => ?e);
                        return None;
                    }
                };

                // 現在価格を履歴から取得
                let current_price = history.prices.last()?.price.clone();

                // ボラティリティの計算
                let volatility = calculate_volatility_from_history(&history).ok()?;
                let volatility_f64 = volatility.to_f64()?;

                Some((
                    token_out,
                    history,
                    current_price,
                    predicted_price,
                    volatility_f64,
                    token_str,
                ))
            }
        })
        .collect();

    // 並行実行（concurrency で制限）
    let results: Vec<_> = stream::iter(history_futures)
        .buffer_unordered(concurrency)
        .collect()
        .await;

    // 6. 流動性スコア、decimals、市場規模を取得
    // client への参照を持つため、チャンク単位で並行処理
    let token_intermediate_data: Vec<_> = results.into_iter().flatten().collect();
    let mut final_results = Vec::with_capacity(token_intermediate_data.len());

    // チャンク単位で処理（concurrency 個ずつ）
    for chunk in token_intermediate_data.chunks(concurrency) {
        // このチャンク内の全トークンに対して並行実行
        let chunk_futures: Vec<_> = chunk
            .iter()
            .map(
                |(token_out, history, current_price, predicted_price, volatility_f64, token_str)| {
                    let token_out = token_out.clone();
                    let history = history.clone();
                    let current_price = current_price.clone();
                    let predicted_price = predicted_price.clone();
                    let volatility_f64 = *volatility_f64;
                    let token_str = token_str.clone();
                    let log = log.clone();
                    async move {
                        // 流動性スコアの計算（プール情報 + 取引量ベース）
                        let liquidity_score =
                            calculate_enhanced_liquidity_score(client, token_out.inner(), &history, cfg)
                                .await;

                        // トークンの decimals を取得（キャッシュ経由）
                        let decimals = match crate::token_cache::get_token_decimals_cached(
                            client,
                            token_out.inner(),
                        )
                        .await
                        {
                            Ok(d) => d,
                            Err(e) => {
                                warn!(log, "failed to get decimals"; "token" => %token_str, "error" => ?e);
                                return None;
                            }
                        };

                        // 市場規模の推定（実際の発行量データを取得）
                        let market_cap =
                            estimate_market_cap_async(client, token_out.inner(), &current_price, decimals)
                                .await;

                        Some((
                            token_out,
                            history,
                            current_price,
                            predicted_price,
                            volatility_f64,
                            liquidity_score,
                            market_cap,
                            decimals,
                        ))
                    }
                },
            )
            .collect();

        // チャンク内を並行実行
        let chunk_results = futures::future::join_all(chunk_futures).await;
        final_results.extend(chunk_results.into_iter().flatten());
    }

    // 7. 結果を集約
    let mut token_data = Vec::new();
    let mut historical_prices = BTreeMap::new();
    let mut expected_returns: BTreeMap<TokenOutAccount, f64> = BTreeMap::new();

    for (
        token_out,
        history,
        current_price,
        predicted_price,
        volatility_f64,
        liquidity_score,
        market_cap,
        decimals,
    ) in final_results
    {
        // 予測価格を保存
        predictions.insert(history.token.clone(), predicted_price.clone());

        // 相対リターンの計算（expected_return メソッドを使用）
        let expected_return_ratio = current_price.expected_return(&predicted_price);
        expected_returns.insert(token_out.clone(), expected_return_ratio);
        let expected_price_return_pct = expected_return_ratio * 100.0;

        trace!(log, "token prediction";
            "token" => %token_out,
            "current_price" => %current_price,
            "predicted_price" => %predicted_price,
            "expected_price_return_pct" => format!("{:.2}%", expected_price_return_pct)
        );

        // TokenData 用に symbol を先に取得
        let symbol_for_token_data = history.token.clone();

        historical_prices.insert(history.token.clone(), history);

        token_data.push(TokenData {
            symbol: symbol_for_token_data,
            current_rate: ExchangeRate::from_price(&current_price, decimals),
            historical_volatility: volatility_f64,
            liquidity_score: Some(liquidity_score),
            market_cap: Some(market_cap),
        });
    }

    // トークンが一つも処理できなかった場合はエラー
    if token_data.is_empty() {
        return Err(anyhow::anyhow!(
            "Failed to process any tokens for portfolio strategy"
        ));
    }

    // per-token confidence を計算（DB エラー時は Hold で安全に停止）
    let token_out_for_confidence: Vec<TokenOutAccount> =
        token_data.iter().map(|t| t.symbol.clone()).collect();
    let prediction_confidences = match super::prediction_accuracy::calculate_per_token_confidence(
        &token_out_for_confidence,
        cfg,
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            warn!(log, "confidence calculation failed, holding"; "error" => %e);
            return Ok((vec![TradingAction::Hold], BTreeMap::new()));
        }
    };

    // バイアス補正（フラグ on のとき適用、3 層 defense-in-depth）
    if cfg.trade_bias_correction_enabled() {
        let biases = match super::prediction_accuracy::calculate_per_token_bias(
            &token_out_for_confidence,
            cfg,
        )
        .await
        {
            Ok(b) => b,
            Err(e) => {
                warn!(log, "bias calculation failed, holding"; "error" => %e);
                return Ok((vec![TradingAction::Hold], BTreeMap::new()));
            }
        };

        // 補正に失敗した（factor<=0、ゼロ等）銘柄は最適化対象から除外し、
        // weight=0 経由の Sell trigger 発火を構造的に防ぐ
        let mut bias_excluded: Vec<TokenOutAccount> = Vec::new();
        for (token, bias) in &biases {
            let Some(predicted) = predictions.get(token) else {
                continue;
            };
            match super::prediction_accuracy::correct_prediction(predicted, *bias) {
                Some(corrected) => {
                    debug!(log, "bias correction applied";
                        "token" => %token,
                        "bias" => format!("{:.4}", bias),
                        "before" => %predicted,
                        "after" => %corrected);
                    predictions.insert(token.clone(), corrected);
                }
                None => {
                    debug!(log, "bias correction failed, excluding token";
                        "token" => %token, "bias" => format!("{:.4}", bias));
                    bias_excluded.push(token.clone());
                }
            }
        }
        if !bias_excluded.is_empty() {
            let bias_excluded_set: HashSet<&TokenOutAccount> = bias_excluded.iter().collect();
            predictions.retain(|k, _| !bias_excluded_set.contains(k));
            token_data.retain(|t| !bias_excluded_set.contains(&t.symbol));
            historical_prices.retain(|k, _| !bias_excluded_set.contains(k));
            debug!(log, "tokens excluded by bias correction failure";
                "count" => bias_excluded.len());
        }

        // 全銘柄が補正失敗した場合は Hold
        if token_data.is_empty() {
            warn!(log, "all tokens excluded by bias correction, holding");
            return Ok((vec![TradingAction::Hold], BTreeMap::new()));
        }
    }

    // 低 confidence トークンを除外（予測は既に実行済み → MAPE は更新される）
    let min_confidence = cfg.trade_min_token_confidence();
    let original_count = token_data.len();
    let excluded: Vec<(TokenOutAccount, f64)> = token_data
        .iter()
        .filter_map(|t| {
            prediction_confidences
                .get(&t.symbol)
                .filter(|&&c| c < min_confidence)
                .map(|&c| (t.symbol.clone(), c))
        })
        .collect();
    token_data.retain(|t| {
        prediction_confidences
            .get(&t.symbol)
            .is_none_or(|&c| c >= min_confidence)
    });
    let remaining_symbols: HashSet<&TokenOutAccount> =
        token_data.iter().map(|t| &t.symbol).collect();
    predictions.retain(|k, _| remaining_symbols.contains(k));
    historical_prices.retain(|k, _| remaining_symbols.contains(k));

    for (token, c) in &excluded {
        info!(log, "excluding token due to low confidence";
            "token" => %token, "confidence" => format!("{:.3}", c));
    }
    if !excluded.is_empty() {
        let excluded_str: Vec<String> = excluded
            .iter()
            .map(|(k, c)| format!("{}({:.3})", k, c))
            .collect();
        info!(log, "tokens filtered by prediction confidence";
            "original" => original_count, "remaining" => token_data.len(),
            "excluded" => excluded_str.join(", "));
    }

    // 全トークン除外のエッジケース: 安全に Hold を返す
    if token_data.is_empty() {
        let all_confidences: Vec<String> = prediction_confidences
            .iter()
            .map(|(k, c)| format!("{}({:.3})", k, c))
            .collect();
        warn!(log, "all tokens below confidence threshold, holding";
            "threshold" => format!("{:.3}", min_confidence),
            "tokens" => all_confidences.join(", "));
        return Ok((vec![TradingAction::Hold], BTreeMap::new()));
    }

    // フィルタ後のトークンのみの confidence を PortfolioData に渡す
    let filtered_confidences: BTreeMap<TokenOutAccount, f64> = prediction_confidences
        .into_iter()
        .filter(|(k, _)| remaining_symbols.contains(k))
        .collect();

    // Telemetry: confidence フィルタ通過後の token 数を記録。
    funnel.after_confidence = token_data.len();

    // 予測誤差分散ベース対角合成（フラグ on のとき）
    let pred_err_diagonal = if cfg.portfolio_pred_err_diagonal_enabled() {
        let token_out_for_var: Vec<TokenOutAccount> =
            token_data.iter().map(|t| t.symbol.clone()).collect();
        match super::prediction_accuracy::calculate_per_token_pred_err_variance(
            &token_out_for_var,
            cfg,
        )
        .await
        {
            Ok(variances) => {
                // typed config returns the enum directly — typo'd values
                // would have panicked at startup in `ConfigResolve`.
                let mode = cfg.portfolio_pred_err_diagonal_mode();
                Some(common::algorithm::portfolio::PredErrDiagonal {
                    k: cfg.portfolio_pred_err_diagonal_k(),
                    variances,
                    mode,
                })
            }
            Err(e) => {
                warn!(log, "pred_err_variance calculation failed, holding"; "error" => %e);
                return Ok((vec![TradingAction::Hold], BTreeMap::new()));
            }
        }
    } else {
        None
    };

    let portfolio_data = PortfolioData {
        tokens: token_data,
        predictions,
        historical_prices,
        prediction_confidences: filtered_confidences,
        pred_err_diagonal,
        cost_deductions: BTreeMap::new(),
    };

    funnel.optimizer_input = portfolio_data.tokens.len();

    // 既存ポジションの取得と WalletInfo の構築
    let wallet_info = if is_new_period {
        // 新規期間: ポジションなし、available_funds を総価値として使用
        debug!(log, "new evaluation period, starting with empty holdings");
        let total_value_near = available_funds.to_value().to_near();
        WalletInfo {
            holdings: BTreeMap::new(),
            total_value: total_value_near.clone(),
            cash_balance: total_value_near,
        }
    } else {
        // 評価期間中: 既存のポジションを取得し、実際のポートフォリオ価値を計算
        debug!(
            log,
            "continuing evaluation period, loading current holdings"
        );
        // wrap.near を含めて全残高を取得（DB に記録がある場合は DB から読み取り）
        let wnear_token = &*blockchain::ref_finance::token_account::WNEAR_TOKEN;
        let mut token_accounts: Vec<common::types::TokenAccount> = tokens
            .iter()
            .map(|t| common::types::TokenAccount::from(t.clone()))
            .collect();
        super::snapshot::ensure_wnear_included(&mut token_accounts);
        let current_balances = match super::snapshot::get_holdings_from_db(period_id).await? {
            Some(holdings) => {
                debug!(log, "loaded holdings from DB snapshot");
                holdings
            }
            None => {
                debug!(log, "no DB snapshot, falling back to RPC");
                swap::get_current_portfolio_balances(client, wallet, &token_accounts).await?
            }
        };

        // 実際のポートフォリオ総価値を計算
        let total_value_near = swap::calculate_total_portfolio_value(&current_balances).await?;

        // wrap.near の残高を cash_balance として使用
        let cash_balance_near = current_balances
            .get(wnear_token)
            .map(|amount| {
                let rate = ExchangeRate::wnear();
                amount / &rate
            })
            .unwrap_or_else(NearValue::zero);

        debug!(log, "portfolio value calculated";
            "total_value" => %total_value_near, "cash_balance" => %cash_balance_near);

        // holdings には投資対象トークンのみ（wrap.near は除外）
        let mut holdings_typed = BTreeMap::new();
        for (token, amount) in &current_balances {
            if token == wnear_token {
                continue;
            }
            if !amount.is_zero() {
                trace!(log, "loaded existing position"; "token" => %token, "amount" => %amount);
                let token_out: common::types::TokenOutAccount = token.clone().into();
                holdings_typed.insert(token_out, amount.clone());
            }
        }

        WalletInfo {
            holdings: holdings_typed,
            total_value: total_value_near,
            cash_balance: cash_balance_near,
        }
    };

    // ポートフォリオ最適化の実行
    let execution_report = if cfg.trade_cost_aware_return_enabled() {
        // 反復コスト考慮最適化: total_value=0 なら早期 Hold
        if wallet_info.total_value.as_bigdecimal() <= &BigDecimal::zero() {
            warn!(log, "total value is zero, holding");
            return Ok((vec![TradingAction::Hold], BTreeMap::new()));
        }

        let cost_inputs = match collect_cost_inputs(
            client,
            wallet.account_id(),
            &portfolio_data.tokens,
            params.pools,
            cfg,
        )
        .await
        {
            Ok(inputs) => inputs,
            Err(e) => {
                warn!(log, "cost inputs collection failed, holding"; "error" => %e);
                return Ok((vec![TradingAction::Hold], BTreeMap::new()));
            }
        };

        // path 不在 token を最適化対象から除外（A2: upstream filter）。
        // INFINITY 注入は `box_maximize_sharpe` の Cholesky 後段で NaN 連鎖を
        // 起こすため使えない（cost.rs:to_cost_deduction の docstring 参照）。
        let mut portfolio_data = portfolio_data;
        if !cost_inputs.failed_tokens.is_empty() {
            let unreachable: HashSet<TokenOutAccount> =
                cost_inputs.failed_tokens.iter().cloned().collect();
            warn!(log, "excluding tokens with no swap path";
                "count" => unreachable.len(),
                "remaining" => portfolio_data.tokens.len().saturating_sub(unreachable.len()));
            portfolio_data.retain_excluding(&unreachable);
        }
        if portfolio_data.tokens.is_empty() {
            warn!(log, "no tokens with valid swap paths, holding");
            return Ok((vec![TradingAction::Hold], BTreeMap::new()));
        }

        let total_value_yocto = wallet_info.total_value.to_yocto();
        let max_iter = cfg.portfolio_cost_iterations_max() as usize;
        // Defense-in-depth: `portfolio_cost_iteration_damping` is already clamped
        // to `[LOWER, UPPER]` (with `NaN → 0.5`) at the typed-config read
        // boundary, and `damp_and_diff` clamps internally as well. This third
        // clamp guards against future regressions where the typed-config layer
        // might be bypassed (e.g. a direct `cfg.read("...")` call). Idempotent —
        // no-op when both upstream guards are intact. The bounds are imported
        // from `common::config` so the three layers stay in sync if the values
        // are tightened (e.g. raising `LOWER` to block silent disable modes).
        let damping = cfg.portfolio_cost_iteration_damping().clamp(
            common::config::PORTFOLIO_COST_ITERATION_DAMPING_LOWER,
            common::config::PORTFOLIO_COST_ITERATION_DAMPING_UPPER,
        );

        match run_cost_aware_optimization(
            &wallet_info,
            portfolio_data,
            &cost_inputs,
            total_value_yocto.as_bigdecimal(),
            max_iter,
            damping,
            cfg.portfolio_rebalance_threshold(),
        )
        .await?
        {
            CostAwareOutcome::Optimized(report) => report,
            CostAwareOutcome::Hold => {
                return Ok((vec![TradingAction::Hold], BTreeMap::new()));
            }
        }
    } else {
        execute_portfolio_optimization(
            &wallet_info,
            portfolio_data,
            cfg.portfolio_rebalance_threshold(),
        )
        .await?
    };

    info!(log, "portfolio optimization completed";
        "actions" => execution_report.actions.len(),
        "rebalance_needed" => execution_report.rebalance_needed,
        "expected_return" => execution_report.optimal_weights.expected_return,
        "expected_volatility" => execution_report.optimal_weights.expected_volatility,
        "sharpe_ratio" => execution_report.optimal_weights.sharpe_ratio
    );

    info!(log, "portfolio backtest metrics";
        "sortino_ratio" => execution_report.expected_metrics.sortino_ratio,
        "max_drawdown" => execution_report.expected_metrics.max_drawdown,
        "calmar_ratio" => execution_report.expected_metrics.calmar_ratio,
        "turnover_rate" => execution_report.expected_metrics.turnover_rate
    );

    for (token, weight) in &execution_report.optimal_weights.weights {
        trace!(log, "optimal weight";
            "token" => %token,
            "weight" => %weight,
            "percentage" => format!("{:.2}%", weight.to_f64().unwrap_or(0.0) * 100.0)
        );
    }

    // Telemetry: ファネル要約と expected_return 分布をサイクル単位で集約ログする。
    // legacy mode (all_predicted_pre_filter_count=None) では出力しない。
    funnel.selected = execution_report
        .optimal_weights
        .weights
        .iter()
        .filter(|(_, w)| w.to_f64().unwrap_or(0.0) > 0.0)
        .count();
    if params.all_predicted_pre_filter_count.is_some() {
        let er_values: Vec<f64> = expected_returns.values().copied().collect();
        let er_stats = candidate_telemetry::summarize_distribution(&er_values);
        match er_stats {
            Some(s) => info!(log, "candidate funnel";
                "predicted" => funnel.predicted,
                "after_confidence" => funnel.after_confidence,
                "after_liquidity" => funnel.after_liquidity,
                "optimizer_input" => funnel.optimizer_input,
                "selected" => funnel.selected,
                "er_min" => format!("{:.4}", s.min),
                "er_p5" => format!("{:.4}", s.p5),
                "er_p25" => format!("{:.4}", s.p25),
                "er_p50" => format!("{:.4}", s.p50),
                "er_p75" => format!("{:.4}", s.p75),
                "er_max" => format!("{:.4}", s.max)),
            None => info!(log, "candidate funnel (no expected_return samples)";
                "predicted" => funnel.predicted,
                "after_confidence" => funnel.after_confidence,
                "after_liquidity" => funnel.after_liquidity,
                "optimizer_input" => funnel.optimizer_input,
                "selected" => funnel.selected),
        }
    }

    Ok((execution_report.actions, expected_returns))
}

/// 最小流動性を満たさないプールを除外する
///
/// 各プールの片側流動性（NEAR 換算の最小値）を算出し、閾値未満のプールを除外する。
fn filter_pools_by_liquidity(
    pools: &Arc<dex::PoolInfoList>,
    wnear: &TokenAccount,
    min_liquidity: &NearValue,
    rates: &HashMap<TokenAccount, ExchangeRate>,
) -> Arc<dex::PoolInfoList> {
    let filtered: Vec<Arc<dex::PoolInfo>> = pools
        .iter()
        .filter(
            |pool| match estimate_pool_liquidity_in_near(pool, wnear, rates) {
                Some(liquidity) => &liquidity >= min_liquidity,
                None => false,
            },
        )
        .cloned()
        .collect();

    Arc::new(dex::PoolInfoList::new(filtered))
}

/// プールの片側流動性を NEAR 建てで推定（最小値）
///
/// プール内の各トークンを NEAR 換算し、評価可能なトークンの中で最小の NEAR 換算額を返す。
///
/// # 設計上の注意: プール単位の全トークン最小値を使う理由
///
/// マルチトークンプール（stable pool 等）ではペア単位で判定するのが理想だが、
/// この関数は経路探索の**前段フィルタ**として使われる。経路探索前にはどのペアを
/// 使うか未確定のため、ペア単位の判定は不可能。全トークン最小値は保守的だが、
/// 流動性が極端に偏ったプールを安全に除外できる。
///
/// - wnear: 直接 NearValue に変換
/// - レートが存在するトークン: レートで NEAR 換算（レートがゼロの場合は NearValue::zero()）
/// - レートが存在しないトークン: スキップ（評価不能）
///
/// None を返すケース: トークンが無い空プール、または全トークンのレートが取得できないプール
fn estimate_pool_liquidity_in_near(
    pool: &dex::PoolInfo,
    wnear: &TokenAccount,
    rates: &HashMap<TokenAccount, ExchangeRate>,
) -> Option<NearValue> {
    let tokens = &pool.bare.token_account_ids;
    let amounts = &pool.bare.amounts;
    debug_assert_eq!(
        tokens.len(),
        amounts.len(),
        "PoolInfo tokens and amounts must have the same length"
    );
    let mut min_side: Option<NearValue> = None;

    for (i, token) in tokens.iter().enumerate() {
        let amount_raw = amounts.get(i).map(|a| a.0).unwrap_or(0);

        let side_value = if token == wnear {
            // wnear: 直接 NearValue に変換
            YoctoValue::from_yocto(BigDecimal::from(amount_raw)).to_near()
        } else if let Some(rate) = rates.get(token) {
            if rate.is_effectively_zero() {
                // 取引不能レート (raw_rate < 1) → 流動性ゼロとして min 計算に含める
                NearValue::zero()
            } else {
                // 他トークン: レートで NEAR 換算
                let token_amount =
                    TokenAmount::from_smallest_units(BigDecimal::from(amount_raw), rate.decimals());
                token_amount / rate
            }
        } else {
            // レートが無いトークン: スキップ（ゼロ扱いしない）
            // 理由: REF Finance のプールの大半は wnear + マイナートークンの構成で、
            // マイナートークンのレートは DB に無いことが多い。ゼロ扱いすると
            // wnear を含むプールが大量に除外され、取引機会を大幅に失う。
            // レート未取得トークンを含む経路のリスクは、後続のグラフ到達性フィルタ
            // (apply_liquidity_filter_and_select Step 2) で軽減される。
            continue;
        };

        min_side = Some(match min_side {
            Some(current_min) if side_value < current_min => side_value,
            Some(current_min) => current_min,
            None => side_value,
        });
    }

    min_side
}

/// 全予測トークン + 保有 union 用のハードフィルタ (all-token モード)。
///
/// `apply_liquidity_filter_and_select` との差分:
/// - `limit` パラメータなし (all-token モードは切り詰めない)
/// - `held` に含まれるトークンは流動性 / 到達性に関係なく**無条件保持**
///   (sell-only の出口経路を確保するため)
///
/// フィルタ後にトークンが残らなければエラー (呼び出し側で `Hold` フォールバック想定)。
fn hard_filter_tokens_keeping_held(
    tokens: Vec<AccountId>,
    pools: &Arc<dex::PoolInfoList>,
    latest_rates: &HashMap<TokenAccount, ExchangeRate>,
    wnear: &TokenAccount,
    wnear_in: &TokenInAccount,
    min_liquidity: &NearValue,
    held: &BTreeSet<TokenOutAccount>,
) -> Result<Vec<AccountId>> {
    let log = DEFAULT.new(o!("function" => "hard_filter_tokens_keeping_held"));

    let filtered_pools = filter_pools_by_liquidity(pools, wnear, min_liquidity, latest_rates);
    let graph = blockchain::ref_finance::path::graph::TokenGraph::new(filtered_pools);
    let buyable_tokens: HashSet<AccountId> = match graph.update_graph(wnear_in) {
        Ok(goals) => goals.iter().map(|t| t.as_account_id().clone()).collect(),
        Err(e) => return Err(anyhow::anyhow!("Failed to build token graph: {}", e)),
    };

    let held_ids: HashSet<AccountId> = held.iter().map(|t| t.as_account_id().clone()).collect();

    let original_count = tokens.len();
    let result: Vec<AccountId> = tokens
        .into_iter()
        .filter(|t| buyable_tokens.contains(t) || held_ids.contains(t))
        .collect();

    if result.is_empty() {
        return Err(anyhow::anyhow!(
            "No candidate tokens after hard filter (filtered {} tokens, no held)",
            original_count
        ));
    }

    debug!(log, "tokens after hard filter (keeping held)";
        "original" => original_count,
        "buyable" => buyable_tokens.len(),
        "held" => held.len(),
        "result" => result.len(),
    );

    Ok(result)
}

/// ボラティリティトークンに対して流動性フィルタとグラフ到達性フィルタを適用する
///
/// 制御フロー:
/// 1. 流動性でプールをフィルタ
/// 2. フィルタ済みプールからグラフを構築し、双方向到達可能なトークンのみ残す
/// 3. フィルタ後にトークンが残らなければエラー
fn apply_liquidity_filter_and_select(
    tokens: Vec<AccountId>,
    pools: &Arc<dex::PoolInfoList>,
    latest_rates: &HashMap<TokenAccount, ExchangeRate>,
    wnear: &TokenAccount,
    wnear_in: &TokenInAccount,
    min_liquidity: &NearValue,
    limit: Option<usize>,
) -> Result<Vec<AccountId>> {
    let log = DEFAULT.new(o!("function" => "apply_liquidity_filter_and_select"));

    // Step 1: 流動性でプールをフィルタ
    let filtered_pools = {
        let filtered = filter_pools_by_liquidity(pools, wnear, min_liquidity, latest_rates);
        debug!(log, "pools filtered by minimum liquidity";
            "original_pools" => pools.list().len(),
            "filtered_pools" => filtered.list().len(),
        );
        filtered
    };

    // Step 2: グラフ到達性フィルタ（双方向到達可能なトークンのみ）
    let graph = blockchain::ref_finance::path::graph::TokenGraph::new(filtered_pools);
    let buyable_tokens = match graph.update_graph(wnear_in) {
        Ok(goals) => {
            let token_ids: std::collections::HashSet<AccountId> =
                goals.iter().map(|t| t.as_account_id().clone()).collect();
            trace!(log, "buyable tokens"; "count" => token_ids.len());
            token_ids
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to build token graph: {}", e));
        }
    };

    // Step 3: フィルタ＋リミット適用
    let original_count = tokens.len();
    let filtered_iter = tokens
        .into_iter()
        .filter(|token| buyable_tokens.contains(token));
    let filtered_tokens: Vec<AccountId> = match limit {
        Some(n) => filtered_iter.take(n).collect(),
        None => filtered_iter.collect(),
    };

    if filtered_tokens.is_empty() {
        return Err(anyhow::anyhow!(
            "No tokens with sufficient liquidity after filtering {} volatility tokens",
            original_count
        ));
    }

    if let Some(n) = limit
        && filtered_tokens.len() < n
    {
        warn!(log, "insufficient tokens after filtering";
            "required" => n,
            "available" => filtered_tokens.len(),
            "fetched" => original_count,
        );
    }

    debug!(log, "tokens after liquidity filtering";
        "original_count" => original_count,
        "filtered_count" => filtered_tokens.len(),
        "limit" => ?limit,
    );

    Ok(filtered_tokens)
}

#[cfg(test)]
mod tests;
