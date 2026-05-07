//! コスト考慮型ポートフォリオ最適化（反復ループ）
//!
//! `strategy.rs::execute_portfolio_strategy` から切り出した、
//! cost-aware return mode 専用の反復最適化ループ。`expected_return -
//! cost_deduction` を Markowitz `box_maximize_sharpe` に渡しつつ、
//! 反復ごとに weight が安定するまでダンピングする。
//!
//! ## 型レベルの安全性
//!
//! - 「first iteration をループ外で実行する」type-state パターンで実装し、
//!   `last_report.expect("at least one iteration ran")` が型レベルで不要に
//!   なる（F001 の panic 経路をクローズ）。
//! - `compute_cost_deductions` は `cost::CostDeduction` Newtype 経由で
//!   `f64` を作るため、`is_finite() && >= 0.0` 不変条件が型で保証される
//!   （F002 の NaN cascade をクローズ）。
//! - `PortfolioData::retain_excluding` で「除外集合」を直接渡せるため、
//!   反転 HashSet を作るヘルパは不要（fin-1 提案、F012）。

use crate::Result;
use crate::cost::{CostDeduction, estimate_trade_cost};
use bigdecimal::{BigDecimal, FromPrimitive, ToPrimitive};
use blockchain::jsonrpc::{GasInfo, ViewContract};
use blockchain::types::gas_price::GasPrice;
use common::algorithm::portfolio::{
    PortfolioData, PortfolioExecutionReport, damp_and_diff, execute_portfolio_optimization,
};
use common::algorithm::types::{TokenData, WalletInfo};
use common::types::{ExchangeRate, TokenAccount, TokenInAccount, TokenOutAccount, YoctoValue};
use dex::{PoolInfoList, TokenPath};
use logging::*;
use near_sdk::AccountId;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

/// 収束判定の重み変化量しきい値（max |Δw| < 1e-3 で収束扱い）
const CONVERGENCE_TOLERANCE: f64 = 1e-3;

/// 取引コスト見積もりに必要な静的入力
///
/// 反復最適化の各反復で path / spot_rate は変わらないため、ループ前に 1 回だけ
/// 収集する。wnear が pools に存在しない等の致命的失敗は `Err` で全体を停止し、
/// 個別 token の path 不在は `failed_tokens` に蓄積して呼び出し側で除外する
/// （`crates/trade/src/swap.rs:execute_direct_swap` と同じ
/// `update_graph` + per-token `match` パターン）。
pub(crate) struct PortfolioCostInputs {
    pub(crate) gas_price: GasPrice,
    pub(crate) storage_min: YoctoValue,
    pub(crate) existing_deposits: HashSet<TokenAccount>,
    pub(crate) paths: BTreeMap<TokenOutAccount, TokenPath>,
    pub(crate) rates: BTreeMap<TokenOutAccount, ExchangeRate>,
    /// `swap_path` が失敗した token（呼び出し側で `retain_excluding` 経由で除外）
    pub(crate) failed_tokens: Vec<TokenOutAccount>,
}

/// `compute_cost_deductions` の結果
///
/// path 不在 token は `collect_cost_inputs` 段階で除外済みなので、
/// ここに現れるのは `estimate_trade_cost` または `to_cost_deduction` が
/// `Err` を返した token のみ。呼び出し側は `estimation_failures` の token を
/// `retain_excluding` で portfolio から除外し、全 token 失敗なら Hold する。
///
/// `f64::INFINITY` 注入は `box_maximize_sharpe` の Cholesky 後段で NaN 連鎖を
/// 起こすため使用しない（`cost.rs:to_cost_deduction` の docstring 参照）。
struct CostDeductionResult {
    deductions: BTreeMap<TokenOutAccount, f64>,
    estimation_failures: Vec<TokenOutAccount>,
}

/// cost-aware 反復後の最終 outcome
///
/// `Hold` バリアントが返るのは「token が一つも残らなかった」病理パスのみ。
/// 呼び出し側は Hold + 空の expected_returns で早期リターンする。
pub(crate) enum CostAwareOutcome {
    Optimized(PortfolioExecutionReport),
    Hold,
}

/// `PortfolioCostInputs::collect`：path / rate / gas_price / 既存 deposit を収集
///
/// `pools` は呼び出し側 (`execute_portfolio_strategy`) で 1 サイクル中に
/// 1 度だけ取得した snapshot を共有する。同一サイクル内で `pool_info` を
/// 二重に読まない (TOCTOU 解消) ためにこの引数で注入する。
pub(crate) async fn collect_cost_inputs<C>(
    client: &C,
    account: &AccountId,
    tokens: &[TokenData],
    pools: &Arc<PoolInfoList>,
) -> Result<PortfolioCostInputs>
where
    C: ViewContract + GasInfo,
{
    let log = DEFAULT.new(o!("function" => "collect_cost_inputs"));

    let gas_price = client.get_gas_price(None).await?;
    let bounds = blockchain::ref_finance::storage::check_bounds(client).await?;
    let storage_min = YoctoValue::from_yocto_u128(bounds.min.0);
    let deposits = blockchain::ref_finance::deposit::get_deposits(client, account).await?;
    let existing_deposits: HashSet<TokenAccount> = deposits.into_keys().collect();

    let graph = blockchain::ref_finance::path::graph::TokenGraph::new(Arc::clone(pools));
    let wnear_in: TokenInAccount = blockchain::ref_finance::token_account::WNEAR_TOKEN
        .clone()
        .to_in();

    // wnear から到達可能な goal をキャッシュに展開（必須）。
    // ここで失敗するのは pools に wnear が存在しない致命的状況のみで、
    // その場合は呼び出し側で Hold に倒す。
    graph.update_graph(&wnear_in)?;

    let mut paths = BTreeMap::new();
    let mut rates = BTreeMap::new();
    let mut failed_tokens = Vec::new();
    for t in tokens {
        match blockchain::ref_finance::path::swap_path(&graph, &wnear_in, &t.symbol).await {
            Ok(path) => {
                paths.insert(t.symbol.clone(), path);
                rates.insert(t.symbol.clone(), t.current_rate.clone());
            }
            Err(e) => {
                debug!(log, "swap path unavailable for token";
                    "token" => %t.symbol, "error" => %e);
                failed_tokens.push(t.symbol.clone());
            }
        }
    }
    if !failed_tokens.is_empty() {
        warn!(log, "tokens excluded from cost estimation: no swap path";
            "count" => failed_tokens.len(),
            "total" => tokens.len());
    }

    Ok(PortfolioCostInputs {
        gas_price,
        storage_min,
        existing_deposits,
        paths,
        rates,
        failed_tokens,
    })
}

/// 重みから銘柄ごとの cost_deduction 比率を計算する。
///
/// `total_value_yocto` は wallet 全体の価値（yoctoNEAR 単位）、
/// `assumed_in[i] = total_value_yocto × weights[i]` で銘柄ごとの取引額を概算。
///
/// `CostDeduction::new` の不変条件 (`is_finite() && >= 0.0`) を満たさない値は
/// `estimation_failures` 経路に合流し、Markowitz には渡らない（NaN cascade 防止）。
/// NaN な weight も入口で 0.0 にクランプして混入を排除する。
fn compute_cost_deductions(
    weights: &[f64],
    tokens: &[TokenData],
    inputs: &PortfolioCostInputs,
    total_value_yocto: &BigDecimal,
) -> CostDeductionResult {
    let mut deductions = BTreeMap::new();
    let mut estimation_failures = Vec::new();
    // `zip` で対応付けることで `weights[i]` のインデックスアクセスを排除し、
    // 長さ不一致時の panic 経路を型レベルで除去する。
    // ただし zip は silent truncation する性質があるため、長さ不一致は
    // `damp_and_diff` の `assert_eq!` と方針を揃えて debug ビルドで検出する
    // （現 caller は常に同じ n で再構築するので release で panic させる必要なし）。
    debug_assert_eq!(
        tokens.len(),
        weights.len(),
        "tokens and weights must have the same length"
    );
    for (t, &raw_w) in tokens.iter().zip(weights.iter()) {
        // NaN weight は入口で 0.0 にクランプ（CostDeduction::new の
        // is_finite 不変条件と整合）。.max(0.0) は f64::NaN.max(0.0) = 0.0
        // なので兼ねるが、明示的に is_finite チェックして意図を表す。
        let w = if raw_w.is_finite() {
            raw_w.max(0.0)
        } else {
            0.0
        };
        let w_bd = BigDecimal::from_f64(w).unwrap_or_default();
        let assumed_in_bd = total_value_yocto * w_bd;
        let assumed_in = YoctoValue::from_yocto(assumed_in_bd);

        let token_account: TokenAccount = t.symbol.clone().into();
        let new_token_count = if inputs.existing_deposits.contains(&token_account) {
            0
        } else {
            1
        };
        // path / rate は collect_cost_inputs 段階で同じキーで insert されているため、
        // ここで揃って欠けるのは「retain_excluding 後に残った token」のみ
        // = 想定外。揃わない場合は当該 token をスキップ（防御）。
        let Some(path) = inputs.paths.get(&t.symbol) else {
            continue;
        };
        let Some(rate) = inputs.rates.get(&t.symbol) else {
            continue;
        };
        match estimate_trade_cost(
            path,
            &assumed_in,
            rate,
            inputs.gas_price,
            &inputs.storage_min,
            new_token_count,
        ) {
            Ok(b) => match b.to_cost_deduction(&assumed_in) {
                Ok(deduction) => {
                    let cd: CostDeduction = deduction;
                    deductions.insert(t.symbol.clone(), cd.as_f64());
                }
                Err(_) => {
                    estimation_failures.push(t.symbol.clone());
                }
            },
            Err(_) => {
                estimation_failures.push(t.symbol.clone());
            }
        }
    }
    CostDeductionResult {
        deductions,
        estimation_failures,
    }
}

/// `portfolio_data` / `weights` / 直近の `report` を保持する反復状態
///
/// `report` が常に有効な値であることを型で保証することで、`last_report.expect`
/// （`Option<PortfolioExecutionReport>` のアンラップ）を不要にする。
struct IterationState {
    portfolio_data: PortfolioData,
    weights: Vec<f64>,
    report: PortfolioExecutionReport,
}

/// 1 反復を実行し、`estimation_failures` があれば token を除外して再試行する。
///
/// 戻り値:
/// - `Some(state)`: 真の最適化が完了した（`report` が `box_maximize_sharpe` 由来）
/// - `None`: 反復のたびに全 token が脱落し、最適化対象が空になった（呼び出し側で Hold）
async fn run_one_iteration(
    wallet_info: &WalletInfo,
    mut portfolio_data: PortfolioData,
    mut weights: Vec<f64>,
    cost_inputs: &PortfolioCostInputs,
    total_value_yocto: &BigDecimal,
    rebalance_threshold: f64,
    iter_label: usize,
) -> Result<Option<IterationState>> {
    let log = DEFAULT.new(o!("function" => "run_one_iteration"));
    loop {
        if portfolio_data.tokens.is_empty() {
            return Ok(None);
        }
        let cost_result = compute_cost_deductions(
            &weights,
            &portfolio_data.tokens,
            cost_inputs,
            total_value_yocto,
        );
        // estimate_trade_cost が Err を返した token を除外して再開。
        // INFINITY や 0.0 を埋めるとそれぞれ NaN 連鎖 / 過小コスト推定を
        // 招くため、不確実な token は最適化対象から外す（`retain_excluding`
        // で全 token-indexed フィールドを同期 filter）。
        if !cost_result.estimation_failures.is_empty() {
            let failed: HashSet<TokenOutAccount> =
                cost_result.estimation_failures.iter().cloned().collect();
            warn!(log, "excluding tokens with cost estimation failure";
                "count" => failed.len(),
                "remaining" => portfolio_data.tokens.len().saturating_sub(failed.len()),
                "iter" => iter_label);
            portfolio_data.retain_excluding(&failed);
            if portfolio_data.tokens.is_empty() {
                return Ok(None);
            }
            let n = portfolio_data.tokens.len();
            weights = vec![1.0 / n as f64; n];
            continue;
        }

        let mut pd_iter = portfolio_data.clone();
        pd_iter.cost_deductions = cost_result.deductions;
        let report =
            execute_portfolio_optimization(wallet_info, pd_iter, rebalance_threshold).await?;
        return Ok(Some(IterationState {
            portfolio_data,
            weights,
            report,
        }));
    }
}

/// cost-aware 反復最適化を実行する。
///
/// # 設計
///
/// 1. **First iteration をループ外で実行**：`run_one_iteration` で「真の最適化が
///    完了した状態」(`IterationState`) を必ず取得してから後続反復に入る。これに
///    より「全反復で `estimation_failures != ∅` で continue → `last_report` が
///    `None` のまま終了」(F001 の panic 経路) を型レベルで排除する。
/// 2. **後続反復**：`damp_and_diff` で重みをダンプして次の反復へ。`max_diff <
///    CONVERGENCE_TOLERANCE` で収束したら終了。
/// 3. **病理パス**：first iteration から `None` が返れば（残 token がゼロ）
///    `CostAwareOutcome::Hold` を返す。後続反復で `None` が返った場合は最後の
///    成功 report をそのまま採用（保守的）。
pub(crate) async fn run_cost_aware_optimization(
    wallet_info: &WalletInfo,
    portfolio_data: PortfolioData,
    cost_inputs: &PortfolioCostInputs,
    total_value_yocto: &BigDecimal,
    max_iter: usize,
    damping: f64,
    rebalance_threshold: f64,
) -> Result<CostAwareOutcome> {
    let log = DEFAULT.new(o!("function" => "run_cost_aware_optimization"));

    let n = portfolio_data.tokens.len();
    if n == 0 {
        return Ok(CostAwareOutcome::Hold);
    }
    let initial_weights = vec![1.0 / n as f64; n];

    // First iteration: type-state により以降は state.report が常に有効。
    let Some(mut state) = run_one_iteration(
        wallet_info,
        portfolio_data,
        initial_weights,
        cost_inputs,
        total_value_yocto,
        rebalance_threshold,
        0,
    )
    .await?
    else {
        warn!(log, "no tokens with cost estimation, holding");
        return Ok(CostAwareOutcome::Hold);
    };

    // 後続反復: ダンピング + 収束判定。
    let total_iters = max_iter.max(1);
    for iter in 1..total_iters {
        let candidate: Vec<f64> = state
            .portfolio_data
            .tokens
            .iter()
            .map(|t| {
                state
                    .report
                    .optimal_weights
                    .weights
                    .get(&t.symbol)
                    .and_then(|bd| bd.to_f64())
                    .unwrap_or(0.0)
            })
            .collect();
        let (new_weights, max_diff) = damp_and_diff(&state.weights, &candidate, damping)?;
        debug!(log, "cost-aware iteration";
            "iter" => iter, "max_diff" => format!("{:.6}", max_diff));
        state.weights = new_weights;
        if max_diff < CONVERGENCE_TOLERANCE {
            break;
        }
        match run_one_iteration(
            wallet_info,
            state.portfolio_data.clone(),
            state.weights.clone(),
            cost_inputs,
            total_value_yocto,
            rebalance_threshold,
            iter,
        )
        .await?
        {
            Some(next) => state = next,
            None => {
                // 後続反復で全 token 脱落 → 直前の成功 report をそのまま採用。
                warn!(log, "all tokens excluded mid-iteration, returning last report";
                    "iter" => iter);
                break;
            }
        }
    }

    Ok(CostAwareOutcome::Optimized(state.report))
}
