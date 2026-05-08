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
//!
//! ## コストモデル: Δw ベース (Phase 2)
//!
//! `compute_cost_deductions` は target weight と current weight (wallet 由来)
//! の差分 `|Δw| × total_value` を実 trade size、`target_w × total_value` を
//! held size として分離して扱う。これにより partial exit / entry の両方向で
//! 正しい cost ratio を与え、Phase 1 の Entry-from-cash モデルが持っていた
//! 「target_w=0 で SELL コスト消失」「partial entry で過大評価」の非対称な
//! 誤差を解消する。詳細は `compute_cost_deductions` の docstring 参照。

use crate::Result;
use crate::cost::estimate_trade_cost;
use bigdecimal::{BigDecimal, FromPrimitive, ToPrimitive};
use blockchain::jsonrpc::{GasInfo, ViewContract};
use blockchain::types::gas_price::GasPrice;
use common::algorithm::portfolio::{
    PortfolioData, PortfolioExecutionReport, calculate_current_weights, damp_and_diff,
    execute_portfolio_optimization,
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

/// damping に応じた反復上限スケーリングの hard cap。
///
/// `damping` の防御下限 (`PORTFOLIO_COST_ITERATION_DAMPING_LOWER = 0.1`) に
/// 対応する `⌈1/0.1⌉ = 10` を hard-cap として固定し、typed config の clamp
/// が bypass された経路（cfg(test) 直接構築・将来の API 変更）でも
/// `f64 as usize` saturating cast による DoS surface
/// （例: `damping = 1e-300` で `1.0/damping = 1e300 as usize → usize::MAX`、
/// `usize::MAX × 10` 反復 ≈ 無限ループ等価で cron tick 完全停止）を
/// 構造的に塞ぐ。
const MAX_DAMPING_SCALE: u32 = 10;

/// `max_iter * scale` の合計上限。production typed config (max_iter ≤ 10、
/// damping ≥ 0.1) では `10 × 10 = 100` で頭打ち、bypass 経路でも本上限で
/// 反復回数を構造的に制限する。
const MAX_TOTAL_ITERATIONS: usize = 100;

/// `max_iter` を `damping` に応じてスケールし、反復上限を有効収束範囲に揃える。
///
/// `damp_and_diff` は `next = (1 - α) × prev + α × candidate` 型の指数収束で、
/// target=0 elimination のような worst-case で `(1 - α)^N < CONVERGENCE_TOLERANCE`
/// になるまでに必要な反復数は α が小さいほど大きい。例えば α=0.5 では N≈10、
/// α=0.1 では N≈69 が必要。生の `max_iter = 10` だけで打ち切ると α=0.1 では
/// 残留 ~35% で収束未到達のまま停止する。`⌈1/α⌉` 倍に拡張することで、
/// 防御下限の damping (`PORTFOLIO_COST_ITERATION_DAMPING_LOWER = 0.1`) でも
/// CONVERGENCE_TOLERANCE まで届く headroom を確保する。
///
/// damping は production typed config で `[0.1, 1.0]` に clamp 済みのため
/// 倍率は最大 10×、`max_iter` 上限 10 と合わせても合計 ≤ 100 反復。clamp
/// が bypass された経路でも `MAX_DAMPING_SCALE` / `MAX_TOTAL_ITERATIONS`
/// で hard-cap し、`f64 as usize` saturating cast の platform-dependent DoS
/// surface を構造的に塞ぐ（fail-loud cap）。
fn scale_max_iter_by_damping(max_iter: usize, damping: f64) -> usize {
    let scale = if damping > 0.0 {
        // f64 を MAX_DAMPING_SCALE (10) に clamp してから usize cast すること
        // で、`damping = 1e-300` 等の clamp 漏れ経路でも cast 結果が確定上限
        // 以下に収まる。`.min()` は片側 NaN なら NaN を返すが damping > 0.0
        // ガードで NaN は排除済み、Infinity も `.min(10.0) = 10.0` で吸収。
        let raw_scale = (1.0 / damping).ceil().min(MAX_DAMPING_SCALE as f64);
        if raw_scale.is_finite() && raw_scale >= 1.0 {
            raw_scale as usize
        } else {
            1
        }
    } else {
        1
    };
    max_iter
        .max(1)
        .saturating_mul(scale.max(1))
        .min(MAX_TOTAL_ITERATIONS)
}

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

/// 重みから銘柄ごとの cost_deduction 比率を計算する（Δw ベース）。
///
/// `total_value_yocto` は wallet 全体の価値（yoctoNEAR 単位）。
/// `target_weights[i]` と `current_weights[i]` から取引差分
/// `Δw[i] = target_w - current_w` を求め、以下を別々に扱う:
///
/// - **trade_size** = `|Δw[i]| × total_value_yocto`
///   `estimate_trade_cost` の price impact 計算に渡す「実際のスワップ量」。
/// - **held_size** = `target_w[i] × total_value_yocto`
///   結果コストを正規化する基準（`r - deduction` の単位を揃えるため
///   target weight 下の保有量で割る）。
///
/// `CostDeduction::new` の不変条件 (`is_finite() && >= 0.0`) を満たさない値は
/// `estimation_failures` 経路に合流し、Markowitz には渡らない（NaN cascade 防止）。
/// NaN な weight も入口で 0.0 にクランプして混入を排除する。
///
/// # 退化ケースの扱い
///
/// - `|Δw| ≈ 0`（取引なし） → `deductions[i] = 0`（コスト無し）。
///   typed config の `PORTFOLIO_COST_DELTA_W_THRESHOLD` 相当の閾値は
///   現状ハードコードで `1e-9`。`f64` 量子化誤差を吸収する目的で、
///   実運用での Δw はほぼ常に 1e-9 を上回る。
/// - `target_w ≈ 0`（全 exit） → `deductions[i] = 0`。Markowitz は
///   `weight × (r - deduction) = 0` で当該銘柄を選好しない構造のため、
///   exit cost は portfolio 比較の観点で見えない（Markowitz の構造的限界）。
///   実コストは `estimate_trade_cost(|Δw|, ...)` 自体は計算済みで、運用の
///   debug ログに記録される。Phase 3 で transition cost を直接 objective に
///   足す regularized Markowitz が必要な場合の follow-up。
///
/// # Phase 1 との対比（Entry-from-cash）
///
/// 旧形式は `assumed_in = target_w × total_value` を **trade と held の両方** に
/// 使う Entry-from-cash モデルだった。partial entry では target_w に対して
/// 過大評価（trade size が実 Δw より大きい）、全 exit では SELL コスト消失
/// （trade size = held size = 0）という非対称な誤差があった。本実装は
/// trade と held を分離して両方向で正しく扱う。
fn compute_cost_deductions(
    target_weights: &[f64],
    current_weights: &[f64],
    tokens: &[TokenData],
    inputs: &PortfolioCostInputs,
    total_value_yocto: &BigDecimal,
) -> CostDeductionResult {
    /// `|Δw|` がこの値より小さい場合は「取引なし」として deduction = 0。
    /// f64 量子化誤差の吸収用で、典型的な Δw はこれより 6 桁以上大きい。
    const DELTA_W_NOOP_THRESHOLD: f64 = 1e-9;

    let mut deductions = BTreeMap::new();
    let mut estimation_failures = Vec::new();
    // `zip` で対応付けることで `weights[i]` のインデックスアクセスを排除し、
    // 長さ不一致時の panic 経路を型レベルで除去する。
    // ただし zip は silent truncation する性質があるため、長さ不一致は
    // debug ビルドで検出する（現 caller は常に同じ n で再構築するので
    // release で panic させる必要なし。`damp_and_diff` 側は外部から長さ
    // 不一致を渡される可能性に備え bail! で fail-soft する設計）。
    debug_assert_eq!(
        tokens.len(),
        target_weights.len(),
        "tokens and target_weights must have the same length"
    );
    debug_assert_eq!(
        tokens.len(),
        current_weights.len(),
        "tokens and current_weights must have the same length"
    );
    for ((t, &raw_target), &raw_current) in tokens
        .iter()
        .zip(target_weights.iter())
        .zip(current_weights.iter())
    {
        // NaN/Inf/負値 target_w は upstream のロジック異常シグナル。silent に
        // 0 へクランプすると optimizer が当該銘柄を「コストなし」で扱って
        // 誤った選好を返す経路になるため、estimation_failures に倒して
        // retain_excluding で portfolio から除外する（Phase 1 と同じ安全性）。
        if !raw_target.is_finite() || raw_target < 0.0 {
            estimation_failures.push(t.symbol.clone());
            continue;
        }
        // current_w は wallet 由来で計算上 finite-non-negative になるはずだが、
        // calculate_current_weights が `unwrap_or(0.0)` で fallback する経路を
        // 持つため、ここでも防御的に 0 へクランプする（fail-soft）。
        let target_w = raw_target;
        let current_w = if raw_current.is_finite() {
            raw_current.max(0.0)
        } else {
            0.0
        };
        let delta_w = (target_w - current_w).abs();

        // target_w = 0 (full exit): Markowitz の `weight × (r - deduction)` は
        // 構造的に 0 なので、ここで deduction を 0 として返しても optimizer の
        // 当該銘柄選好には影響しない。SELL コスト自体は estimate_trade_cost が
        // 計算しており debug ログにも残せるが、portfolio 比較に流す経路が
        // ないため deduction = 0 で素通しする。
        if target_w == 0.0 {
            deductions.insert(t.symbol.clone(), 0.0);
            if delta_w > DELTA_W_NOOP_THRESHOLD {
                let log = DEFAULT.new(o!("function" => "compute_cost_deductions"));
                debug!(
                    log,
                    "delta-w cost: full-exit cost not propagated to Markowitz (target_w=0)";
                    "token" => %t.symbol,
                    "delta_w" => format!("{delta_w:.6}"),
                );
            }
            continue;
        }

        // |Δw| ≈ 0 → 取引なし、deduction = 0。typed config bypass で current_w
        // が NaN/負値だった場合も上の正規化で 0 になっており、ここに来る Δw は
        // 純粋な丸め誤差レベル。
        if delta_w < DELTA_W_NOOP_THRESHOLD {
            deductions.insert(t.symbol.clone(), 0.0);
            continue;
        }

        // 直前の `is_finite() && >= 0.0` ガードにより `BigDecimal::from_f64`
        // は仕様上 None を返さないが、bigdecimal の future minor version で
        // 挙動が変わる可能性に備えて fail-soft skip する（cron tick crash 防止）。
        let Some(target_w_bd) = BigDecimal::from_f64(target_w) else {
            estimation_failures.push(t.symbol.clone());
            continue;
        };
        let Some(delta_w_bd) = BigDecimal::from_f64(delta_w) else {
            estimation_failures.push(t.symbol.clone());
            continue;
        };

        let trade_size = YoctoValue::from_yocto(total_value_yocto * delta_w_bd);
        let held_size = YoctoValue::from_yocto(total_value_yocto * target_w_bd);

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
        // CostError は `std::error::Error` 実装済みなので Into 経由で
        // anyhow::Error に橋渡しし、二重 nested match を平坦化する。
        let result = estimate_trade_cost(
            path,
            &trade_size,
            rate,
            inputs.gas_price,
            &inputs.storage_min,
            new_token_count,
        )
        .and_then(|b| {
            b.to_cost_deduction_with_basis(&trade_size, &held_size)
                .map_err(Into::into)
        });
        match result {
            Ok(deduction) => {
                deductions.insert(t.symbol.clone(), deduction.as_f64());
            }
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
        // 現在の保有重みを wallet から計算する（Δw ベースの cost 推定で必要）。
        // 反復ごとに portfolio_data.tokens が retain_excluding で減るため、
        // tokens に揃えて再計算する（順序も同期）。
        let current_weights = calculate_current_weights(&portfolio_data.tokens, wallet_info);
        let cost_result = compute_cost_deductions(
            &weights,
            &current_weights,
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

    // 後続反復: ダンピング + 収束判定。`scale_max_iter_by_damping` が damping
    // に応じて反復上限を拡張し、防御下限 `damping=0.1` でも収束 headroom を確保する。
    let total_iters = scale_max_iter_by_damping(max_iter, damping);
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

#[cfg(test)]
mod tests;
