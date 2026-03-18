use crate::Result;
use crate::types::{NearValue, TokenOutAccount, TokenPrice};
use bigdecimal::{BigDecimal, FromPrimitive, RoundingMode, ToPrimitive};
use chrono::{DateTime, Utc};
use nalgebra::DMatrix;
use ndarray::{Array1, Array2};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

use super::types::*;

// ==================== ポートフォリオ固有の型定義 ====================

/// ポートフォリオデータ
#[derive(Debug, Clone)]
pub struct PortfolioData {
    pub tokens: Vec<TokenData>,
    /// 予測価格（TokenPrice: NEAR/token）
    pub predictions: BTreeMap<TokenOutAccount, TokenPrice>,
    pub historical_prices: BTreeMap<TokenOutAccount, PriceHistory>,
    /// トークンごとの予測精度に基づく信頼度 [0.0, 1.0]
    /// - エントリあり: confidence に応じた Sharpe/RP ブレンド
    /// - エントリなし: データ不足 → max(alpha_vol * 0.5, PREDICTION_ALPHA_FLOOR) にフォールバック
    pub prediction_confidences: BTreeMap<TokenOutAccount, f64>,
}

/// ポートフォリオ実行レポート
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioExecutionReport {
    pub actions: Vec<TradingAction>,
    pub optimal_weights: PortfolioWeights,
    pub rebalance_needed: bool,
    pub expected_metrics: PortfolioMetrics,
    pub timestamp: DateTime<Utc>,
}

// ==================== 定数 ====================

/// リスクフリーレート（年率2%相当の日次レート: 0.02 / 365）
const RISK_FREE_RATE: f64 = 5.479e-5;

/// 単一トークンの最大保有比率（積極的設定）
const MAX_POSITION_SIZE: f64 = 0.6;

/// 最小保有比率
const MIN_POSITION_SIZE: f64 = 0.05;

/// 最大保有トークン数（集中投資）
const MAX_HOLDINGS: usize = 6;

/// PSD 保証のための最小固有値閾値
const MIN_EIGENVALUE_THRESHOLD: f64 = 1e-6;

/// Ridge 正則化の対角加算量（Σ + εI）
const RIDGE_EPSILON: f64 = 1e-6;

/// 最小流動性スコア
/// スコア 0.5 = プール流動性 >= TRADE_MIN_POOL_LIQUIDITY 基準額
const MIN_LIQUIDITY_SCORE: f64 = 0.5;

/// 最小市場規模（10,000 NEAR）
fn min_market_cap() -> NearValue {
    NearValue::from_near(BigDecimal::from(10000))
}

/// 動的リスク調整の閾値（日次ボラティリティ）
const HIGH_VOLATILITY_THRESHOLD: f64 = 0.0157; // 年率30%相当の日次ボラティリティ (≈ 0.3/√365)
const LOW_VOLATILITY_THRESHOLD: f64 = 0.00523; // 年率10%相当の日次ボラティリティ (≈ 0.1/√365)

/// リスクパリティの最大反復回数
const MAX_RISK_PARITY_ITERATIONS: usize = 50;

/// リスクパリティの収束判定閾値
const RISK_PARITY_CONVERGENCE_TOLERANCE: f64 = 1e-6;

/// 予測精度が低い場合の alpha 下限値
/// confidence=0.0 のとき alpha はこの値まで下がる（Sharpe/RP 等配分に近づく）
pub const PREDICTION_ALPHA_FLOOR: f64 = 0.5;

/// 内部 f64 weight を外部公開用 BigDecimal に変換する。
/// 小数点以下10桁で丸める。
fn weight_from_f64(value: f64) -> BigDecimal {
    BigDecimal::from_f64(value)
        .unwrap_or_default()
        .with_scale_round(10, RoundingMode::HalfUp)
}

// ==================== コア計算関数 ====================

/// 期待リターンを計算
///
/// # 注意
/// rate = tokens/NEAR（1 NEAR あたりのトークン数）は価格の逆数。
/// rate が上がる = 価格が下がる なので、符号を反転させる。
///
/// - rate 上昇 → 価格下落 → 保有価値減少 → 負のリターン
/// - rate 下降 → 価格上昇 → 保有価値増加 → 正のリターン
///
/// `TokenPrice.expected_return()` を使用して符号の間違いを防ぐ。
pub fn calculate_expected_returns(
    tokens: &[TokenInfo],
    predictions: &BTreeMap<TokenOutAccount, TokenPrice>,
) -> Vec<f64> {
    tokens
        .iter()
        .map(|token| {
            if let Some(predicted_price) = predictions.get(&token.symbol) {
                let current_price = token.current_rate.to_price();
                if current_price.is_zero() || predicted_price.is_zero() {
                    return 0.0;
                }

                // TokenPrice.expected_return() で直接計算（型安全）
                current_price.expected_return(predicted_price)
            } else {
                0.0
            }
        })
        .collect()
}

/// 日次リターンを計算
/// 注意: 価格データは比率（Price型）として保存されている。リターン計算は相対値なので単位に依存しない。
///
/// 入力スライスの順序を保持して各トークンの日次リターンを返します。
/// 重複トークンは最初の出現のみを使用します（防御的チェック）。
pub fn calculate_daily_returns(historical_prices: &[PriceHistory]) -> Vec<Vec<f64>> {
    let mut seen = std::collections::HashSet::new();
    historical_prices
        .iter()
        .filter(|p| seen.insert(p.token.to_string()))
        .map(|p| calculate_returns_from_prices(&p.prices))
        .collect()
}

/// 共分散行列を計算（Ledoit-Wolf 縮小推定）
pub fn calculate_covariance_matrix(daily_returns: &[Vec<f64>]) -> Array2<f64> {
    let n = daily_returns.len();
    if n == 0 {
        return Array2::zeros((0, 0));
    }

    // Ledoit-Wolf 縮小推定（内部で T アラインされた sample_cov を計算）
    let mut covariance = ledoit_wolf_shrink(daily_returns);

    // PSD保証: 固有値分解→負の固有値をクランプ→再構成（防御的）
    ensure_positive_semi_definite(&mut covariance);

    covariance
}

/// Ledoit-Wolf 縮小推定（共分散行列の正則化）
///
/// サンプル共分散行列 S を縮小ターゲット F = (tr(S)/n)·I に向けて最適に縮小する。
/// Ledoit & Wolf (2004) の解析的縮小係数を使用。
/// n > T（トークン数 > データ点数）の場合に特に有効。
///
/// S, β̂, F, δ² を全て同一の T 点（= 全系列の共通最小長）データから計算し、
/// Ledoit-Wolf の i.i.d. 前提条件を満たす。
fn ledoit_wolf_shrink(daily_returns: &[Vec<f64>]) -> Array2<f64> {
    let n = daily_returns.len();
    if n <= 1 {
        let mut m = Array2::zeros((n, n));
        if n == 1 && !daily_returns[0].is_empty() {
            let r = &daily_returns[0];
            let mean = r.iter().sum::<f64>() / r.len() as f64;
            let var = r.iter().map(|&v| (v - mean).powi(2)).sum::<f64>()
                / (r.len() as f64 - 1.0).max(1.0);
            m[[0, 0]] = var;
        }
        return m;
    }

    // 全系列の共通最小長を取得（末尾アライン）
    let min_len = daily_returns.iter().map(|r| r.len()).min().unwrap_or(0);
    if min_len < 2 {
        return Array2::zeros((n, n));
    }

    let t = min_len;

    // 各トークンの平均リターン（T アライン済みデータ）
    let means: Vec<f64> = (0..n)
        .map(|i| {
            let r = &daily_returns[i];
            let start = r.len() - t;
            r[start..].iter().sum::<f64>() / t as f64
        })
        .collect();

    // T アラインされたデータからサンプル共分散行列を計算
    let mut sample_cov = Array2::zeros((n, n));
    for i in 0..n {
        for j in i..n {
            let ri = &daily_returns[i][daily_returns[i].len() - t..];
            let rj = &daily_returns[j][daily_returns[j].len() - t..];
            let cov = calculate_covariance(ri, rj);
            sample_cov[[i, j]] = cov;
            sample_cov[[j, i]] = cov;
        }
    }

    // ターゲット: F = (tr(S) / n) · I
    let mu = (0..n).map(|i| sample_cov[[i, i]]).sum::<f64>() / n as f64;

    // ||S - F||²_F — 対称性を利用: 対角 + 上三角×2
    let mut delta_sq = 0.0;
    for i in 0..n {
        delta_sq += (sample_cov[[i, i]] - mu).powi(2);
        for j in (i + 1)..n {
            delta_sq += 2.0 * sample_cov[[i, j]].powi(2);
        }
    }

    if delta_sq < 1e-30 {
        // S ≈ F: ターゲットそのものを返す
        let mut result = Array2::zeros((n, n));
        for i in 0..n {
            result[[i, i]] = mu;
        }
        return result;
    }

    // β̂ = (1/T²) · Σ_t ||x_t·x_t' - S||²_F
    // 対称性を利用: 上三角のみ計算して2倍（対角は1倍）
    let mut beta_hat = 0.0;
    let mut x_t = vec![0.0; n]; // ヒープ確保を1回に
    for day in 0..t {
        for i in 0..n {
            let r = &daily_returns[i];
            x_t[i] = r[r.len() - t + day] - means[i];
        }

        let mut norm_sq = 0.0;
        for i in 0..n {
            // 対角
            let diff = x_t[i] * x_t[i] - sample_cov[[i, i]];
            norm_sq += diff * diff;
            // 上三角（×2）
            for j in (i + 1)..n {
                let diff = x_t[i] * x_t[j] - sample_cov[[i, j]];
                norm_sq += 2.0 * diff * diff;
            }
        }
        beta_hat += norm_sq;
    }
    beta_hat /= (t as f64) * (t as f64);

    // 最適縮小係数
    let shrinkage = (beta_hat / delta_sq).clamp(0.0, 1.0);

    // Σ_LW = δ · F + (1 - δ) · S — 対称性を利用
    let mut result = Array2::zeros((n, n));
    let one_minus_s = 1.0 - shrinkage;
    for i in 0..n {
        result[[i, i]] = shrinkage * mu + one_minus_s * sample_cov[[i, i]];
        for j in (i + 1)..n {
            let val = one_minus_s * sample_cov[[i, j]];
            result[[i, j]] = val;
            result[[j, i]] = val;
        }
    }

    result
}

/// 共分散行列が正定値であることを保証する
///
/// 異なる長さの系列ペアから計算した共分散行列は正定値でない可能性がある。
/// 固有値分解 Σ = V·diag(λ)·V' を行い、負の固有値を ε にクランプして再構成する。
fn ensure_positive_semi_definite(covariance: &mut Array2<f64>) {
    let n = covariance.nrows();
    if n <= 1 {
        return;
    }

    // ndarray → nalgebra に変換
    let mat = nalgebra::DMatrix::from_fn(n, n, |i, j| covariance[[i, j]]);

    // 対称行列の固有値分解
    let eigen = mat.symmetric_eigen();

    // 負の固有値があるかチェック
    let min_eigenvalue = eigen
        .eigenvalues
        .iter()
        .cloned()
        .fold(f64::INFINITY, f64::min);

    if min_eigenvalue >= MIN_EIGENVALUE_THRESHOLD {
        return; // 既にPSD
    }

    // 負の固有値をクランプして再構成: Σ' = V · diag(max(λ, ε)) · V'
    let clamped_eigenvalues =
        nalgebra::DMatrix::from_diagonal(&nalgebra::DVector::from_fn(n, |i, _| {
            eigen.eigenvalues[i].max(MIN_EIGENVALUE_THRESHOLD)
        }));

    let reconstructed = &eigen.eigenvectors * clamped_eigenvalues * eigen.eigenvectors.transpose();

    // nalgebra → ndarray に変換
    for i in 0..n {
        for j in 0..n {
            covariance[[i, j]] = reconstructed[(i, j)];
        }
    }
}

/// 2つの系列間の共分散を計算
///
/// 長さが異なる場合は末尾（最新データ）を優先してトリミングする。
fn calculate_covariance(returns1: &[f64], returns2: &[f64]) -> f64 {
    let min_len = returns1.len().min(returns2.len());
    if min_len < 2 {
        return 0.0;
    }

    // 末尾（最新データ）を優先: 長い方の先頭を切り詰める
    let r1 = &returns1[returns1.len() - min_len..];
    let r2 = &returns2[returns2.len() - min_len..];

    let mean1: f64 = r1.iter().sum::<f64>() / min_len as f64;
    let mean2: f64 = r2.iter().sum::<f64>() / min_len as f64;

    r1.iter()
        .zip(r2.iter())
        .map(|(&v1, &v2)| (v1 - mean1) * (v2 - mean2))
        .sum::<f64>()
        / (min_len - 1) as f64
}

/// ポートフォリオリターンを計算
pub fn calculate_portfolio_return(weights: &[f64], expected_returns: &[f64]) -> f64 {
    weights
        .iter()
        .zip(expected_returns.iter())
        .map(|(&w, &r)| w * r)
        .sum()
}

/// ポートフォリオの標準偏差を計算
pub fn calculate_portfolio_std(weights: &[f64], covariance_matrix: &Array2<f64>) -> f64 {
    let w = Array1::from(weights.to_vec());
    let portfolio_variance = w.dot(&covariance_matrix.dot(&w));
    portfolio_variance.sqrt().max(1e-10) // ゼロ除算防止
}

// ==================== 最適化アルゴリズム ====================

/// シャープレシオを最大化する最適ポートフォリオを計算（解析解 + アクティブセット法）
///
/// ロングオンリー制約下での最大シャープ比ポートフォリオ:
/// 1. 超過リターン μ_excess = μ - rf を計算
/// 2. z = Σ⁻¹ · μ_excess を解く
/// 3. z_i < 0 の資産を除外し再計算（アクティブセット法）
/// 4. w = z / Σz_i で正規化
pub fn maximize_sharpe_ratio(
    expected_returns: &[f64],
    covariance_matrix: &Array2<f64>,
) -> Vec<f64> {
    let n = expected_returns.len();
    if n == 0 {
        return vec![];
    }

    let default_weights = vec![1.0 / n as f64; n];

    if n == 1 {
        return vec![1.0];
    }

    // 全トークンの期待リターンが同一の場合、等配分が最小分散解
    let min_return = expected_returns
        .iter()
        .cloned()
        .fold(f64::INFINITY, f64::min);
    let max_return = expected_returns
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);
    if (max_return - min_return).abs() < 1e-12 {
        return default_weights;
    }

    // 超過リターン: μ - rf
    let excess_returns: Vec<f64> = expected_returns
        .iter()
        .map(|&r| r - RISK_FREE_RATE)
        .collect();

    // アクティブセット法: ロングオンリー制約
    let mut active: Vec<usize> = (0..n).collect();

    loop {
        let m = active.len();
        if m == 0 {
            return default_weights;
        }

        // アクティブ資産のサブ共分散行列とサブ超過リターンベクトルを構築
        let cov_sub = DMatrix::from_fn(m, m, |i, j| covariance_matrix[[active[i], active[j]]]);
        let excess_sub = nalgebra::DVector::from_fn(m, |i, _| excess_returns[active[i]]);

        // Σ_sub · z = μ_excess_sub を解く（Cholesky優先、失敗時はLU分解）
        let z = cov_sub
            .clone()
            .cholesky()
            .map(|chol| chol.solve(&excess_sub))
            .or_else(|| cov_sub.clone().lu().solve(&excess_sub))
            .or_else(|| {
                // Ridge regularization retry: Σ_sub + εI
                let mut cov_reg = cov_sub.clone();
                for i in 0..m {
                    cov_reg[(i, i)] += RIDGE_EPSILON;
                }
                cov_reg.cholesky().map(|chol| chol.solve(&excess_sub))
            });

        let z = match z {
            Some(z) => z,
            None => return default_weights,
        };

        // 負の要素を見つけて除外
        let min_entry = z
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal));

        if let Some((idx, &val)) = min_entry
            && val < 0.0
        {
            active.remove(idx);
            continue;
        }

        // 全 z >= 0: 正規化して返す
        let sum_z: f64 = z.iter().sum();
        if sum_z <= 0.0 {
            return default_weights;
        }

        let mut weights = vec![0.0; n];
        for (i, &asset_idx) in active.iter().enumerate() {
            weights[asset_idx] = z[i] / sum_z;
        }

        return weights;
    }
}

/// Active Set 法における各資産の境界状態
#[derive(Clone, Copy, PartialEq, Eq)]
enum BoundState {
    Free,
    Lower,
    Upper,
}

/// ボックス制約付き Sharpe 最大化（3集合 Active Set 法）
///
/// 各資産の重みが [0, max_position] の範囲に収まるよう制約しつつ、
/// Sharpe 比を最大化する。Free / Lower(=0) / Upper(=max_position) の
/// 3 集合を管理し、KKT 条件に基づいて集合間を移動する。
///
/// max_position >= 1.0 のとき既存 `maximize_sharpe_ratio()` と同一の解を返す。
pub fn box_maximize_sharpe(
    expected_returns: &[f64],
    covariance_matrix: &Array2<f64>,
    max_position: f64,
) -> Vec<f64> {
    let n = expected_returns.len();
    if n == 0 {
        return vec![];
    }
    if n == 1 {
        return vec![1.0];
    }

    let default_weights = vec![1.0 / n as f64; n];

    // max_position が非実用的に小さい場合は等配分
    let effective_max = if n as f64 * max_position < 1.0 {
        1.0 / n as f64
    } else {
        max_position
    };

    // max_position >= 1.0 なら制約なしと同等
    if effective_max >= 1.0 {
        return maximize_sharpe_ratio(expected_returns, covariance_matrix);
    }

    // 全トークンの期待リターンが同一 → 等配分
    let min_ret = expected_returns
        .iter()
        .cloned()
        .fold(f64::INFINITY, f64::min);
    let max_ret = expected_returns
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);
    if (max_ret - min_ret).abs() < 1e-12 {
        return default_weights;
    }

    let excess_returns: Vec<f64> = expected_returns
        .iter()
        .map(|&r| r - RISK_FREE_RATE)
        .collect();

    // 3 集合: Free / Lower (w=0) / Upper (w=max_position)
    let mut state = vec![BoundState::Free; n];
    let max_iter = 3 * n + 10;

    enum Factored {
        Chol(nalgebra::Cholesky<f64, nalgebra::Dyn>),
        Lu(nalgebra::LU<f64, nalgebra::Dyn, nalgebra::Dyn>),
    }

    for _ in 0..max_iter {
        let free: Vec<usize> = (0..n).filter(|&i| state[i] == BoundState::Free).collect();
        let upper: Vec<usize> = (0..n).filter(|&i| state[i] == BoundState::Upper).collect();

        if free.is_empty() {
            // Free 集合が空: Upper に固定された資産のみで正規化
            if upper.is_empty() {
                return default_weights;
            }
            let mut weights = vec![0.0; n];
            for &i in &upper {
                weights[i] = effective_max;
            }
            normalize_weights(&mut weights);
            return weights;
        }

        let m = free.len();

        // Upper 集合の固定重みによる budget 消費
        let budget_upper: f64 = upper.iter().map(|_| effective_max).sum();
        let budget_free = 1.0 - budget_upper;

        if budget_free <= 0.0 {
            // Upper 集合だけで budget を超過
            let mut weights = vec![0.0; n];
            let total = upper.len() as f64 * effective_max;
            for &i in &upper {
                weights[i] = effective_max / total;
            }
            return weights;
        }

        // Free 集合のサブ問題を構築
        let cov_ff = DMatrix::from_fn(m, m, |i, j| covariance_matrix[[free[i], free[j]]]);
        let excess_f = nalgebra::DVector::from_fn(m, |i, _| excess_returns[free[i]]);

        // Σ_FF の分解（Cholesky 優先、LU フォールバック）を1回だけ行い再利用
        let factored = cov_ff
            .clone()
            .cholesky()
            .map(Factored::Chol)
            .or_else(|| Some(Factored::Lu(cov_ff.lu())));
        let factored = match factored {
            Some(f) => f,
            None => return default_weights,
        };
        let solve = |rhs: &nalgebra::DVector<f64>| -> Option<nalgebra::DVector<f64>> {
            let result = match &factored {
                Factored::Chol(chol) => Some(chol.solve(rhs)),
                Factored::Lu(lu) => lu.solve(rhs),
            };
            if result.is_some() {
                return result;
            }
            // Ridge regularization retry: Σ_FF + εI
            let mut cov_reg = DMatrix::from_fn(m, m, |i, j| covariance_matrix[[free[i], free[j]]]);
            for i in 0..m {
                cov_reg[(i, i)] += RIDGE_EPSILON;
            }
            cov_reg.cholesky().map(|chol| chol.solve(rhs))
        };

        // p = Σ_FF⁻¹ · μ_excess_F
        let p = match solve(&excess_f) {
            Some(p) => p,
            None => return default_weights,
        };

        // Σ_FU · w_U のベクトル → q = Σ_FF⁻¹ · (Σ_FU · w_U)
        let q = if upper.is_empty() {
            nalgebra::DVector::zeros(m)
        } else {
            let mut cov_fu_wu = nalgebra::DVector::zeros(m);
            for (fi, &f_idx) in free.iter().enumerate() {
                let mut sum = 0.0;
                for &u_idx in &upper {
                    sum += covariance_matrix[[f_idx, u_idx]] * effective_max;
                }
                cov_fu_wu[fi] = sum;
            }
            match solve(&cov_fu_wu) {
                Some(q) => q,
                None => return default_weights,
            }
        };

        // ラグランジュ乗数 γ = (budget_F + Σq) / Σp
        let sum_p: f64 = p.iter().sum();
        let sum_q: f64 = q.iter().sum();

        if sum_p.abs() < 1e-15 {
            return default_weights;
        }

        let gamma = (budget_free + sum_q) / sum_p;

        // Free 集合の重み: w_F = γ·p - q
        let w_free: Vec<f64> = (0..m).map(|i| gamma * p[i] - q[i]).collect();

        // 違反チェック
        let mut moved = false;

        // F→L: w < 0
        for (fi, &w) in w_free.iter().enumerate() {
            if w < -1e-10 {
                state[free[fi]] = BoundState::Lower;
                moved = true;
                break;
            }
        }
        if moved {
            continue;
        }

        // F→U: w > max_position
        for (fi, &w) in w_free.iter().enumerate() {
            if w > effective_max + 1e-10 {
                state[free[fi]] = BoundState::Upper;
                moved = true;
                break;
            }
        }
        if moved {
            continue;
        }

        // L→F / U→F: 勾配条件チェック
        // Lower (w=0): ∂L/∂w_i > 0 なら Free に移動すべき
        // Upper (w=max): ∂L/∂w_i < 0 なら Free に移動すべき
        // 勾配 = excess_returns[i] - γ * Σ_i· · w
        let mut weights = vec![0.0; n];
        for (fi, &f_idx) in free.iter().enumerate() {
            weights[f_idx] = w_free[fi];
        }
        for &u_idx in &upper {
            weights[u_idx] = effective_max;
        }

        for i in 0..n {
            if state[i] == BoundState::Lower {
                // Lower: 勾配 = μ_excess[i] - γ * Σ_row[i] · w
                let grad = excess_returns[i]
                    - gamma
                        * (0..n)
                            .map(|j| covariance_matrix[[i, j]] * weights[j])
                            .sum::<f64>();
                if grad > 1e-10 {
                    state[i] = BoundState::Free;
                    moved = true;
                    break;
                }
            } else if state[i] == BoundState::Upper {
                // Upper: 勾配の符号が逆 → 減らしたい
                let grad = excess_returns[i]
                    - gamma
                        * (0..n)
                            .map(|j| covariance_matrix[[i, j]] * weights[j])
                            .sum::<f64>();
                if grad < -1e-10 {
                    state[i] = BoundState::Free;
                    moved = true;
                    break;
                }
            }
        }
        if moved {
            continue;
        }

        // 収束: 全 KKT 条件を満たす
        let sum: f64 = weights.iter().sum();
        if sum <= 0.0 {
            return default_weights;
        }
        normalize_weights(&mut weights);
        clamp_and_normalize(&mut weights, effective_max);

        return weights;
    }

    // 収束しなかった場合: 等配分にフォールバック
    default_weights
}

/// リスクパリティ調整（反復収束版）
///
/// 各資産のリスク寄与度が均等になるよう重みを反復的に調整する。
/// 1回の調整で他の全資産のリスク寄与度が変化するため、収束まで反復が必要。
pub fn apply_risk_parity(weights: &mut [f64], covariance_matrix: &Array2<f64>) {
    let n = weights.len();
    if n == 0 {
        return;
    }

    for _ in 0..MAX_RISK_PARITY_ITERATIONS {
        let prev_weights = weights.to_vec();

        let w = Array1::from(weights.to_vec());
        let portfolio_variance = w.dot(&covariance_matrix.dot(&w));

        if portfolio_variance <= 0.0 {
            return;
        }

        let portfolio_vol = portfolio_variance.sqrt();
        let marginal_risk = covariance_matrix.dot(&w);

        // 目標リスク寄与度（均等）
        let target_risk_contribution = portfolio_vol / n as f64;

        // 重みを調整
        for i in 0..n {
            if marginal_risk[i] > 0.0 {
                let current_risk_contribution = weights[i] * marginal_risk[i] / portfolio_vol;
                if current_risk_contribution > 0.0 {
                    let adjustment = target_risk_contribution / current_risk_contribution;
                    weights[i] *= adjustment.clamp(0.5, 2.0);
                }
            }
        }

        // 正規化
        let sum: f64 = weights.iter().sum();
        if sum > 0.0 {
            for w in weights.iter_mut() {
                *w /= sum;
            }
        }

        // 収束判定（正規化後の重み変化で判定）
        let max_change = weights
            .iter()
            .zip(prev_weights.iter())
            .map(|(w, pw)| (w - pw).abs())
            .fold(0.0_f64, f64::max);
        if max_change < RISK_PARITY_CONVERGENCE_TOLERANCE {
            break;
        }
    }
}

/// ボックス制約付き Risk Parity（固定集合法）
///
/// 各資産の重みが [0, max_position] に収まるよう制約しつつ、
/// リスク寄与度の均等化を目指す。max_position に張り付いた資産を
/// Pinned 集合として固定し、残りの Free 集合で RP を反復する。
pub fn box_risk_parity(covariance_matrix: &Array2<f64>, max_position: f64) -> Vec<f64> {
    let n = covariance_matrix.nrows();
    if n == 0 {
        return vec![];
    }
    if n == 1 {
        return vec![1.0];
    }

    let effective_max = if n as f64 * max_position < 1.0 {
        1.0 / n as f64
    } else {
        max_position
    };

    // effective_max >= 1.0 なら制約なしのRP
    if effective_max >= 1.0 {
        let mut w = vec![1.0 / n as f64; n];
        apply_risk_parity(&mut w, covariance_matrix);
        return w;
    }

    // pinned[i] = true なら w[i] = effective_max に固定
    let mut pinned = vec![false; n];
    let mut weights = vec![1.0 / n as f64; n];
    let max_outer = 2 * n;

    for _ in 0..max_outer {
        let free: Vec<usize> = (0..n).filter(|&i| !pinned[i]).collect();
        let pinned_count = n - free.len();

        if free.is_empty() {
            // 全資産 pinned: 均等配分
            let s = pinned_count as f64 * effective_max;
            return vec![effective_max / s; n];
        }

        let budget_free = 1.0 - pinned_count as f64 * effective_max;
        if budget_free <= 0.0 {
            // pinned だけで budget 超過: pinned のみで正規化
            let s = pinned_count as f64 * effective_max;
            let mut w = vec![0.0; n];
            for i in 0..n {
                if pinned[i] {
                    w[i] = effective_max / s;
                }
            }
            return w;
        }

        // Free 集合で RP 反復
        let m = free.len();
        let mut w_free = vec![budget_free / m as f64; m];

        // サブ共分散行列（Free 集合のみ）
        let cov_sub = Array2::from_shape_fn((m, m), |(i, j)| covariance_matrix[[free[i], free[j]]]);

        for _ in 0..MAX_RISK_PARITY_ITERATIONS {
            let prev = w_free.clone();

            let wf = Array1::from(w_free.clone());
            let port_var = wf.dot(&cov_sub.dot(&wf));

            if port_var <= 0.0 {
                break;
            }

            let port_vol = port_var.sqrt();
            let marginal = cov_sub.dot(&wf);
            let target_rc = port_vol / m as f64;

            for i in 0..m {
                if marginal[i] > 0.0 {
                    let rc = w_free[i] * marginal[i] / port_vol;
                    if rc > 0.0 {
                        let adj = target_rc / rc;
                        w_free[i] *= adj.clamp(0.5, 2.0);
                    }
                }
            }

            // Free 集合内で budget_free に正規化
            let sum: f64 = w_free.iter().sum();
            if sum > 0.0 {
                for w in w_free.iter_mut() {
                    *w *= budget_free / sum;
                }
            }

            let max_change = w_free
                .iter()
                .zip(prev.iter())
                .map(|(a, b)| (a - b).abs())
                .fold(0.0_f64, f64::max);
            if max_change < RISK_PARITY_CONVERGENCE_TOLERANCE {
                break;
            }
        }

        // Free→Pinned: max_position 超過チェック
        let mut any_change = false;
        for (fi, &f_idx) in free.iter().enumerate() {
            if w_free[fi] > effective_max + 1e-10 {
                pinned[f_idx] = true;
                any_change = true;
            }
        }

        // Pinned→Free: pinned トークンの RC が目標を超過している場合 unpin
        // （重みを減らした方が RP に近づく → Free 集合で再最適化させる）
        if !any_change {
            // 現在の重みベクトルを構築して RC を計算
            let mut current_w = vec![0.0; n];
            for (fi, &f_idx) in free.iter().enumerate() {
                current_w[f_idx] = w_free[fi];
            }
            for i in 0..n {
                if pinned[i] {
                    current_w[i] = effective_max;
                }
            }
            let cw = Array1::from(current_w.clone());
            let pv = cw.dot(&covariance_matrix.dot(&cw));
            if pv > 0.0 {
                let pvol = pv.sqrt();
                let mg = covariance_matrix.dot(&cw);
                let target_rc = pvol / n as f64;
                for i in 0..n {
                    if pinned[i] {
                        let rc = current_w[i] * mg[i] / pvol;
                        if rc > target_rc * PINNED_RC_RELEASE_FACTOR {
                            pinned[i] = false;
                            any_change = true;
                            break; // 1つずつ解除
                        }
                    }
                }
            }
        }

        if !any_change {
            // 収束: 重みを組み立てて返す
            for (fi, &f_idx) in free.iter().enumerate() {
                weights[f_idx] = w_free[fi];
            }
            for i in 0..n {
                if pinned[i] {
                    weights[i] = effective_max;
                }
            }

            normalize_weights(&mut weights);
            clamp_and_normalize(&mut weights, effective_max);

            return weights;
        }
    }

    // 収束しなかった場合: 等配分
    vec![1.0 / n as f64; n]
}

// ==================== 案 I ユーティリティ関数群 ====================

/// 重みベクトルを正規化（合計 = 1.0）
fn normalize_weights(weights: &mut [f64]) {
    let sum: f64 = weights.iter().sum();
    if sum > 0.0 {
        for w in weights.iter_mut() {
            *w /= sum;
        }
    }
}

/// box clamp + 正規化（浮動小数点誤差対策）
fn clamp_and_normalize(weights: &mut [f64], max_position: f64) {
    for w in weights.iter_mut() {
        *w = w.clamp(0.0, max_position);
    }
    normalize_weights(weights);
}

/// box_sharpe + box_rp → alpha ブレンド → 正規化 → フルサイズ展開
///
/// サブセットの最適化結果を n_total サイズのベクトルに展開して返す。
fn blend_and_expand(
    sub_returns: &[f64],
    sub_cov: &Array2<f64>,
    max_position: f64,
    alphas: &[f64],
    subset_indices: &[usize],
    n_total: usize,
) -> Vec<f64> {
    debug_assert!(
        subset_indices.iter().all(|&idx| idx < alphas.len()),
        "subset_indices must be within alphas bounds"
    );
    let w_sharpe = box_maximize_sharpe(sub_returns, sub_cov, max_position);
    let w_rp = box_risk_parity(sub_cov, max_position);

    let mut blended: Vec<f64> = w_sharpe
        .iter()
        .zip(w_rp.iter())
        .enumerate()
        .map(|(i, (&ws, &wr))| {
            let a = alphas[subset_indices[i]];
            a * ws + (1.0 - a) * wr
        })
        .collect();
    normalize_weights(&mut blended);

    let mut weights = vec![0.0; n_total];
    for (i, &idx) in subset_indices.iter().enumerate() {
        weights[idx] = blended[i];
    }
    weights
}

/// サブセット最適化の不変パラメータ
struct SubsetOptParams<'a> {
    expected_returns: &'a [f64],
    covariance_matrix: &'a Array2<f64>,
    max_position: f64,
    alphas: &'a [f64],
}

/// キャッシュ付き blend_and_expand
///
/// subset_indices をキーとしてメモ化し、同一サブセットの重複計算を排除する。
fn cached_blend_and_expand(
    cache: &mut HashMap<Vec<usize>, Vec<f64>>,
    params: &SubsetOptParams<'_>,
    subset_indices: &[usize],
    n_total: usize,
) -> Vec<f64> {
    if let Some(cached) = cache.get(subset_indices) {
        return cached.clone();
    }
    let (sub_ret, sub_cov) = extract_sub_portfolio(
        params.expected_returns,
        params.covariance_matrix,
        subset_indices,
    );
    let weights = blend_and_expand(
        &sub_ret,
        &sub_cov,
        params.max_position,
        params.alphas,
        subset_indices,
        n_total,
    );
    cache.insert(subset_indices.to_vec(), weights.clone());
    weights
}

/// サブポートフォリオの μ と Σ を抽出
fn extract_sub_portfolio(
    expected_returns: &[f64],
    covariance_matrix: &Array2<f64>,
    indices: &[usize],
) -> (Vec<f64>, Array2<f64>) {
    let m = indices.len();
    let sub_returns: Vec<f64> = indices.iter().map(|&i| expected_returns[i]).collect();
    let sub_cov =
        Array2::from_shape_fn((m, m), |(i, j)| covariance_matrix[[indices[i], indices[j]]]);
    (sub_returns, sub_cov)
}

/// Risk Parity 乖離度（RC 均等度）を計算
///
/// 各資産のリスク寄与度 RC_i と目標 RC の差の二乗平均を返す。
/// 値が小さいほど RP に近い。
fn risk_parity_divergence(weights: &[f64], covariance_matrix: &Array2<f64>) -> f64 {
    let n = weights.len();
    if n == 0 {
        return 0.0;
    }

    let w = Array1::from(weights.to_vec());
    let port_var = w.dot(&covariance_matrix.dot(&w));
    if port_var <= 0.0 {
        return 0.0;
    }
    let port_vol = port_var.sqrt();

    let marginal = covariance_matrix.dot(&w);
    let target_rc = port_vol / n as f64;

    let mut sum_sq = 0.0;
    for i in 0..n {
        let rc = weights[i] * marginal[i] / port_vol;
        sum_sq += (rc - target_rc).powi(2);
    }
    sum_sq / n as f64
}

/// 流動性ペナルティ付きリターン調整
///
/// μ_adj[i] = μ[i] - LIQUIDITY_PENALTY_LAMBDA * (1.0 - liquidity[i])
/// liquidity が低いほどペナルティが大きい。
fn adjust_returns_for_liquidity(expected_returns: &[f64], liquidity_scores: &[f64]) -> Vec<f64> {
    debug_assert_eq!(
        expected_returns.len(),
        liquidity_scores.len(),
        "expected_returns and liquidity_scores must have the same length"
    );
    expected_returns
        .iter()
        .zip(liquidity_scores.iter())
        .map(|(&r, &liq)| r - LIQUIDITY_PENALTY_LAMBDA * (1.0 - liq.clamp(0.0, 1.0)))
        .collect()
}

/// ハードフィルタ: 流動性 + 時価総額の最低条件でトークンをフィルタ
///
/// `select_optimal_tokens()` のフィルタ部分を抽出。
/// スコアリングや相関フィルタは行わない。
pub fn hard_filter_tokens(tokens: &[TokenData]) -> Vec<TokenData> {
    let min_cap = min_market_cap();
    let filtered: Vec<&TokenData> = tokens
        .iter()
        .filter(|t| {
            let liquidity_ok = t.liquidity_score.unwrap_or(0.0) >= MIN_LIQUIDITY_SCORE;
            let market_cap_ok = match &t.market_cap {
                Some(cap) => cap >= &min_cap,
                None => true,
            };
            liquidity_ok && market_cap_ok
        })
        .collect();

    filtered.into_iter().cloned().collect()
}

/// C(n, k) の組み合わせイテレータ（辞書式順序、再帰なし）
struct Combinations {
    indices: Vec<usize>,
    n: usize,
    k: usize,
    first: bool,
}

impl Combinations {
    fn new(n: usize, k: usize) -> Self {
        let indices = (0..k).collect();
        Self {
            indices,
            n,
            k,
            first: true,
        }
    }
}

impl Iterator for Combinations {
    type Item = Vec<usize>;

    fn next(&mut self) -> Option<Vec<usize>> {
        if self.k == 0 || self.k > self.n {
            return None;
        }

        if self.first {
            self.first = false;
            return Some(self.indices.clone());
        }

        // 右端から増加可能な位置を探す
        let mut i = self.k;
        loop {
            if i == 0 {
                return None;
            }
            i -= 1;
            if self.indices[i] < self.n - self.k + i {
                break;
            }
        }

        // その位置をインクリメントし、以降を連番で埋める
        self.indices[i] += 1;
        for j in (i + 1)..self.k {
            self.indices[j] = self.indices[j - 1] + 1;
        }

        Some(self.indices.clone())
    }
}

// ==================== 案 I: Phase 3 全列挙 + 統合最適化 ====================

/// 枝刈り上位からの枝数
const PRUNE_KEEP_PER: usize = 12; // 2 × MAX_HOLDINGS

/// Phase 3 候補数上限: C(MAX_PHASE3_CANDIDATES, MAX_HOLDINGS) が実用的な計算量に収まる値
/// C(15, 6) = 5,005
const MAX_PHASE3_CANDIDATES: usize = 15;

/// 流動性ペナルティ係数
const LIQUIDITY_PENALTY_LAMBDA: f64 = 0.01;

/// Pinned→Free 解除閾値の倍率。
/// RC がターゲットのこの倍率を超えた場合に unpin する。
/// 1.0 では即座に unpin し振動を招き、2.0 では不均衡を許容しすぎる。
/// 1.5 はヒステリシスとして安定と均衡のバランスが取れた値。
const PINNED_RC_RELEASE_FACTOR: f64 = 1.5;

/// min_position_size 未満のトークンを除外して再最適化を繰り返す。
/// 毎回最低1トークンが脱落するため、最大 current_indices.len() 回で収束する。
fn filter_and_reoptimize(
    weights: &mut Vec<f64>,
    current_indices: &mut Vec<usize>,
    params: &SubsetOptParams<'_>,
    min_position_size: f64,
    cache: &mut HashMap<Vec<usize>, Vec<f64>>,
) {
    let n_total = params.expected_returns.len();
    let max_iters = current_indices.len();
    for _ in 0..max_iters {
        let survivors: Vec<usize> = current_indices
            .iter()
            .filter(|&&idx| weights[idx] >= min_position_size)
            .copied()
            .collect();
        if survivors.len() == current_indices.len() || survivors.is_empty() {
            break;
        }
        *weights = cached_blend_and_expand(cache, params, &survivors, n_total);
        *current_indices = survivors;
    }
}

/// Phase 3: 全列挙による最適サブセット選択
///
/// 候補トークン群から C(active, max_holdings) の全組み合わせを列挙し、
/// 各サブセットで box 制約付き Sharpe + RP ブレンドを計算。
/// 複合スコアで最良のサブセットを返す。
fn exhaustive_optimize(
    active_indices: &[usize],
    expected_returns: &[f64],
    covariance_matrix: &Array2<f64>,
    max_position: f64,
    max_holdings: usize,
    min_position_size: f64,
    alphas: &[f64],
) -> Vec<f64> {
    let n_total = expected_returns.len();
    let n_active = active_indices.len();

    if n_active == 0 {
        return vec![0.0; n_total];
    }

    let params = SubsetOptParams {
        expected_returns,
        covariance_matrix,
        max_position,
        alphas,
    };

    // active <= max_holdings: サブセット選択不要、直接最適化
    if n_active <= max_holdings {
        let mut cache = HashMap::new();
        let mut weights = cached_blend_and_expand(&mut cache, &params, active_indices, n_total);

        // MIN_POSITION_SIZE フィルタ: 違反トークンを除外して再最適化（反復）
        let mut current_indices = active_indices.to_vec();
        filter_and_reoptimize(
            &mut weights,
            &mut current_indices,
            &params,
            min_position_size,
            &mut cache,
        );

        return weights;
    }

    // C(active, max_holdings) の全列挙
    let mut best_score = f64::NEG_INFINITY;
    let mut best_weights = vec![0.0; n_total];
    let mut cache = HashMap::new();

    for combo in Combinations::new(n_active, max_holdings) {
        let subset_indices: Vec<usize> = combo.iter().map(|&ci| active_indices[ci]).collect();

        let mut effective_blended =
            cached_blend_and_expand(&mut cache, &params, &subset_indices, n_total);

        // MIN_POSITION_SIZE フィルタ: 違反トークンを除外して再最適化（反復）
        let mut current_indices = subset_indices.clone();
        filter_and_reoptimize(
            &mut effective_blended,
            &mut current_indices,
            &params,
            min_position_size,
            &mut cache,
        );

        // 複合スコア: alpha * sharpe - (1-alpha) * rp_div
        let active_w: Vec<f64> = effective_blended
            .iter()
            .filter(|&&w| w > 0.0)
            .cloned()
            .collect();
        let active_idx: Vec<usize> = effective_blended
            .iter()
            .enumerate()
            .filter(|&(_, w)| *w > 0.0)
            .map(|(i, _)| i)
            .collect();

        if active_w.is_empty() {
            continue;
        }

        let (ar, ac) = extract_sub_portfolio(expected_returns, covariance_matrix, &active_idx);
        let port_ret = calculate_portfolio_return(&active_w, &ar);
        let port_std = calculate_portfolio_std(&active_w, &ac);
        let sharpe = if port_std > 0.0 {
            (port_ret - RISK_FREE_RATE) / port_std
        } else {
            0.0
        };
        let rp_div = risk_parity_divergence(&active_w, &ac);

        // 複合スコア: Sharpe (O(1)) と rp_div (O(1e-5)) のスケール差を
        // ポートフォリオ分散で正規化して吸収
        let rp_div_normalized = if port_std > 0.0 {
            rp_div / (port_std * port_std)
        } else {
            0.0
        };
        // アクティブトークンの alpha 単純平均を使用
        // 加重平均はウエイト→alpha→ウエイトの循環依存になるため不可
        debug_assert!(
            !active_idx.is_empty(),
            "active_idx must be non-empty when active_w is non-empty"
        );
        let effective_alpha: f64 =
            active_idx.iter().map(|&idx| alphas[idx]).sum::<f64>() / active_idx.len() as f64;
        let score = effective_alpha * sharpe - (1.0 - effective_alpha) * rp_div_normalized;

        if score > best_score {
            best_score = score;
            best_weights = effective_blended;
        }
    }

    // フォールバック: best_weights が全ゼロなら等配分
    if best_weights.iter().all(|&w| w == 0.0) {
        let k = max_holdings.min(n_active);
        for i in 0..k {
            best_weights[active_indices[i]] = 1.0 / k as f64;
        }
    }

    best_weights
}

/// 3 フェーズ統合最適化（案 I）
///
/// Phase 1: 全 n トークンで box_maximize_sharpe + box_risk_parity
/// Phase 2: Sharpe 上位 ∪ RP 上位 の和集合で枝刈り
/// Phase 3: exhaustive_optimize で厳密解
fn unified_optimize(
    expected_returns: &[f64],
    covariance_matrix: &Array2<f64>,
    liquidity_scores: &[f64],
    max_position: f64,
    max_holdings: usize,
    min_position_size: f64,
    alphas: &[f64],
) -> Vec<f64> {
    let n = expected_returns.len();
    if n == 0 {
        return vec![];
    }
    if n == 1 {
        return vec![1.0];
    }

    // 流動性調整リターン
    let adj_returns = adjust_returns_for_liquidity(expected_returns, liquidity_scores);

    // Phase 1: 全 n トークンで独立に最適化
    let w_sharpe = box_maximize_sharpe(&adj_returns, covariance_matrix, max_position);
    let w_rp = box_risk_parity(covariance_matrix, max_position);

    // Phase 2: 枝刈り — Sharpe 上位 ∪ RP 上位 の和集合
    let keep = PRUNE_KEEP_PER.min(n);

    let mut sharpe_ranked: Vec<(usize, f64)> =
        w_sharpe.iter().enumerate().map(|(i, &w)| (i, w)).collect();
    sharpe_ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut rp_ranked: Vec<(usize, f64)> = w_rp.iter().enumerate().map(|(i, &w)| (i, w)).collect();
    rp_ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut active_set = std::collections::BTreeSet::new();
    for &(idx, _) in sharpe_ranked.iter().take(keep) {
        active_set.insert(idx);
    }
    for &(idx, _) in rp_ranked.iter().take(keep) {
        active_set.insert(idx);
    }

    // 和集合が空なら全トークンを使用
    let mut active_indices: Vec<usize> = if active_set.is_empty() {
        (0..n).collect()
    } else {
        active_set.into_iter().collect()
    };

    // Phase 3 候補数の安全制限: C(n, k) の組み合わせ爆発を防止
    // 候補が多すぎる場合、ブレンドスコア上位に絞り込む
    if active_indices.len() > MAX_PHASE3_CANDIDATES && active_indices.len() > max_holdings {
        let mut scored: Vec<(usize, f64)> = active_indices
            .iter()
            .map(|&i| {
                let blend = alphas[i] * w_sharpe[i] + (1.0 - alphas[i]) * w_rp[i];
                (i, blend)
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        active_indices = scored
            .iter()
            .take(MAX_PHASE3_CANDIDATES)
            .map(|&(i, _)| i)
            .collect();
        active_indices.sort(); // インデックス順にソート
    }

    // Phase 3: 全列挙による厳密解
    let mut weights = exhaustive_optimize(
        &active_indices,
        &adj_returns,
        covariance_matrix,
        max_position,
        max_holdings,
        min_position_size,
        alphas,
    );
    // 浮動小数点誤差による合計 != 1.0 を補正
    normalize_weights(&mut weights);
    weights
}

/// 市場の平均ボラティリティを計算（日次リターンの標準偏差の平均）
fn calculate_market_volatility(daily_returns: &[Vec<f64>]) -> f64 {
    if daily_returns.is_empty() {
        return 0.0;
    }

    daily_returns
        .iter()
        .map(|returns| {
            if returns.len() < 2 {
                return 0.0;
            }
            let mean = returns.iter().sum::<f64>() / returns.len() as f64;
            let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>()
                / (returns.len() - 1) as f64;
            variance.sqrt()
        })
        .sum::<f64>()
        / daily_returns.len() as f64
}

/// ボラティリティ → [0, 1] の正規化比率
/// LOW_VOLATILITY_THRESHOLD 以下 → 0.0、HIGH_VOLATILITY_THRESHOLD 以上 → 1.0
fn volatility_ratio(avg_volatility: f64) -> f64 {
    ((avg_volatility - LOW_VOLATILITY_THRESHOLD)
        / (HIGH_VOLATILITY_THRESHOLD - LOW_VOLATILITY_THRESHOLD))
        .clamp(0.0, 1.0)
}

/// ボラティリティに基づくブレンド alpha [0.7, 0.9]
/// 高ボラ → 0.7 (RP寄り)、低ボラ → 0.9 (Sharpe寄り)
fn volatility_blend_alpha(avg_volatility: f64) -> f64 {
    0.9 - volatility_ratio(avg_volatility) * (0.9 - 0.7)
}

/// ボラティリティに基づく動的最大ポジションサイズ
/// 高ボラ → MAX_POSITION_SIZE * 0.7 (分散強制)、低ボラ → MAX_POSITION_SIZE (集中許容)
fn dynamic_max_position(avg_volatility: f64) -> f64 {
    MAX_POSITION_SIZE * (1.0 - 0.3 * volatility_ratio(avg_volatility))
}

/// weight ベクトルのバリデーション
///
/// NaN/Inf/負値を 0.0 に置換する。非有限値が含まれていた場合は `true` を返す。
pub fn validate_weights(weights: &[f64]) -> (Vec<f64>, bool) {
    let mut had_invalid = false;
    let validated = weights
        .iter()
        .map(|&w| {
            if w.is_finite() && w >= 0.0 {
                w
            } else {
                had_invalid = true;
                0.0
            }
        })
        .collect();
    (validated, had_invalid)
}

/// リバランスが必要かチェック
pub fn needs_rebalancing(
    current_weights: &[f64],
    target_weights: &[f64],
    rebalance_threshold: f64,
) -> bool {
    if current_weights.len() != target_weights.len() {
        return true;
    }

    current_weights
        .iter()
        .zip(target_weights.iter())
        .any(|(&current, &target)| (current - target).abs() > rebalance_threshold)
}

// ==================== メトリクス計算 ====================

/// ターンオーバー率を計算
pub fn calculate_turnover_rate(old_weights: &[f64], new_weights: &[f64]) -> f64 {
    if old_weights.len() != new_weights.len() {
        return 1.0; // 完全な入れ替え
    }

    old_weights
        .iter()
        .zip(new_weights.iter())
        .map(|(&old, &new)| (old - new).abs())
        .sum::<f64>()
        / 2.0
}

/// 価格からリターンを計算
///
/// 入力をタイムスタンプ昇順にソートしてからリターンを計算する。
fn calculate_returns_from_prices(prices: &[PricePoint]) -> Vec<f64> {
    if prices.len() < 2 {
        return Vec::new();
    }

    let mut sorted: Vec<&PricePoint> = prices.iter().collect();
    sorted.sort_by_key(|p| p.timestamp);

    let mut returns = Vec::new();
    for i in 1..sorted.len() {
        let price_current = sorted[i].price.as_bigdecimal().to_f64().unwrap_or(0.0);
        let price_prev = sorted[i - 1].price.as_bigdecimal().to_f64().unwrap_or(0.0);
        if price_prev > 0.0 {
            returns.push((price_current - price_prev) / price_prev);
        }
    }
    returns
}

// ==================== ポートフォリオ実行 ====================

/// ポートフォリオ最適化戦略を実行
pub async fn execute_portfolio_optimization(
    wallet: &WalletInfo,
    portfolio_data: PortfolioData,
    rebalance_threshold: f64,
) -> Result<PortfolioExecutionReport> {
    // ハードフィルタ: 流動性 + 時価総額の最低条件
    let filtered_tokens = hard_filter_tokens(&portfolio_data.tokens);

    // フィルタを通過するトークンがない場合は Hold で早期リターン
    if filtered_tokens.is_empty() {
        return Ok(PortfolioExecutionReport {
            actions: vec![TradingAction::Hold],
            optimal_weights: PortfolioWeights {
                weights: BTreeMap::new(),
                timestamp: Utc::now(),
                expected_return: 0.0,
                expected_volatility: 0.0,
                sharpe_ratio: 0.0,
            },
            rebalance_needed: false,
            expected_metrics: PortfolioMetrics {
                sortino_ratio: 0.0,
                max_drawdown: 0.0,
                calmar_ratio: 0.0,
                turnover_rate: 0.0,
            },
            timestamp: Utc::now(),
        });
    }

    // historical_prices に存在するトークンのみに絞り込み
    let selected_tokens: Vec<TokenData> = filtered_tokens
        .into_iter()
        .filter(|t| portfolio_data.historical_prices.contains_key(&t.symbol))
        .collect();

    // 選択されたトークンのみでポートフォリオを構築
    let selected_predictions: BTreeMap<TokenOutAccount, TokenPrice> = portfolio_data
        .predictions
        .iter()
        .filter(|(symbol, _)| selected_tokens.iter().any(|t| &t.symbol == *symbol))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    // 期待リターンを計算
    let expected_returns = calculate_expected_returns(&selected_tokens, &selected_predictions);

    // 選択されたトークンの価格履歴を selected_tokens の順序に合わせて構築
    let selected_price_histories: Vec<PriceHistory> = selected_tokens
        .iter()
        .filter_map(|t| portfolio_data.historical_prices.get(&t.symbol).cloned())
        .collect();

    // 日次リターンと共分散行列を計算
    let daily_returns = calculate_daily_returns(&selected_price_histories);
    let covariance = calculate_covariance_matrix(&daily_returns);

    // 動的リスク調整: ボラティリティに基づくポジションサイズ制御
    let avg_volatility = calculate_market_volatility(&daily_returns);
    let max_position = dynamic_max_position(avg_volatility);

    // ボラティリティ連動 alpha_vol でブレンド（範囲 [0.7, 0.9]）
    let alpha_vol = volatility_blend_alpha(avg_volatility);

    // per-token alpha: トークンごとの confidence に基づく Sharpe/RP ブレンド比率
    let alphas: Vec<f64> = selected_tokens
        .iter()
        .map(|t| {
            let confidence = portfolio_data
                .prediction_confidences
                .get(&t.symbol)
                .copied();
            match confidence {
                Some(c) => {
                    let floor = PREDICTION_ALPHA_FLOOR;
                    (floor + (alpha_vol - floor) * c).clamp(floor, 0.9)
                }
                // データなし（コールドスタート）→ FLOOR を保証しつつ控えめなデフォルト
                None => (alpha_vol * 0.5).max(PREDICTION_ALPHA_FLOOR),
            }
        })
        .collect();

    // 流動性スコアを抽出
    let liquidity_scores: Vec<f64> = selected_tokens
        .iter()
        .map(|t| t.liquidity_score.unwrap_or(0.0))
        .collect();

    // 統合最適化（案 I: 3 フェーズ）
    let optimal_weights = unified_optimize(
        &expected_returns,
        &covariance,
        &liquidity_scores,
        max_position,
        MAX_HOLDINGS,
        MIN_POSITION_SIZE,
        &alphas,
    );

    // 現在のポートフォリオ重みを計算
    let current_weights = calculate_current_weights(&selected_tokens, wallet);

    // リバランスが必要かチェック
    let rebalance_needed =
        needs_rebalancing(&current_weights, &optimal_weights, rebalance_threshold);

    // アクションを生成
    let actions = if rebalance_needed {
        generate_rebalance_actions(
            &selected_tokens,
            &current_weights,
            &optimal_weights,
            rebalance_threshold,
        )
    } else {
        vec![TradingAction::Hold]
    };

    // フォワード指標を計算（PortfolioWeights用）
    let portfolio_return = calculate_portfolio_return(&optimal_weights, &expected_returns);
    let portfolio_vol = calculate_portfolio_std(&optimal_weights, &covariance);
    let sharpe_ratio = if portfolio_vol > 0.0 {
        (portfolio_return - RISK_FREE_RATE) / portfolio_vol
    } else {
        0.0
    };

    // 重みをBTreeMapに変換（キーによる確定的な順序のため）
    let weight_map: BTreeMap<TokenOutAccount, BigDecimal> = selected_tokens
        .iter()
        .zip(optimal_weights.iter())
        .filter(|&(_, weight)| *weight > 0.0)
        .map(|(token, weight)| (token.symbol.clone(), weight_from_f64(*weight)))
        .collect();

    let optimal_weights_struct = PortfolioWeights {
        weights: weight_map,
        timestamp: Utc::now(),
        expected_return: portfolio_return,
        expected_volatility: portfolio_vol,
        sharpe_ratio,
    };

    // ポートフォリオレベルの日次リターン系列を構築
    let min_return_len = daily_returns.iter().map(|r| r.len()).min().unwrap_or(0);
    let portfolio_daily_returns: Vec<f64> = if min_return_len > 0 && !optimal_weights.is_empty() {
        (0..min_return_len)
            .map(|day| {
                optimal_weights
                    .iter()
                    .zip(daily_returns.iter())
                    .map(|(w, returns)| w * returns[returns.len() - min_return_len + day])
                    .sum()
            })
            .collect()
    } else {
        vec![]
    };

    let daily_risk_free = RISK_FREE_RATE;
    let sortino_ratio = calculate_sortino_ratio(&portfolio_daily_returns, daily_risk_free);

    let cumulative_values: Vec<f64> = {
        let mut vals = Vec::with_capacity(portfolio_daily_returns.len() + 1);
        vals.push(1.0);
        for &r in &portfolio_daily_returns {
            vals.push(vals.last().unwrap() * (1.0 + r));
        }
        vals
    };
    let max_drawdown = calculate_max_drawdown(&cumulative_values);

    let backtest_mean_return = if portfolio_daily_returns.is_empty() {
        0.0
    } else {
        portfolio_daily_returns.iter().sum::<f64>() / portfolio_daily_returns.len() as f64
    };
    let calmar_ratio = if max_drawdown > 0.0 {
        backtest_mean_return / max_drawdown
    } else {
        0.0
    };

    let expected_metrics = PortfolioMetrics {
        sortino_ratio,
        max_drawdown,
        calmar_ratio,
        turnover_rate: calculate_turnover_rate(&current_weights, &optimal_weights),
    };

    Ok(PortfolioExecutionReport {
        actions,
        optimal_weights: optimal_weights_struct,
        rebalance_needed,
        expected_metrics,
        timestamp: Utc::now(),
    })
}

/// 現在の重みを計算
/// 型安全: holdingsはTokenAmount（smallest_units + decimals）、total_valueはNearValue（NEAR単位）
fn calculate_current_weights(tokens: &[TokenInfo], wallet: &WalletInfo) -> Vec<f64> {
    let mut weights = vec![0.0; tokens.len()];
    let total_value = &wallet.total_value;

    for (i, token) in tokens.iter().enumerate() {
        if let Some(holding) = wallet.holdings.get(&token.symbol) {
            // TokenAmount / &ExchangeRate = NearValue トレイトを使用
            // holding の decimals と current_rate の decimals は一致している必要がある
            let value_near = holding / &token.current_rate;

            // &NearValue / &NearValue = BigDecimal
            if !total_value.is_zero() {
                let weight = &value_near / total_value;
                weights[i] = weight.to_f64().unwrap_or(0.0);
            }

            // デバッグ用ログ (テスト時のみ)
            #[cfg(test)]
            {
                println!(
                    "Token {}: rate={}, holding={}, value_near={}, weight={:.6}%",
                    token.symbol,
                    token.current_rate,
                    holding,
                    value_near,
                    weights[i] * 100.0
                );

                let value_near_f64 = value_near.as_bigdecimal().to_f64().unwrap_or(0.0);
                if value_near_f64 > 100.0 {
                    // 100 NEAR以上の場合は警告
                    println!(
                        "WARNING: Token {} has unusually high value: {:.6} NEAR",
                        token.symbol, value_near_f64
                    );
                    println!("  Rate: {}", token.current_rate);
                    println!("  Holdings: {}", holding);
                    println!("  Value (NEAR): {}", value_near);
                    println!("  Weight: {:.6}%", weights[i] * 100.0);
                }
            }
        }
    }

    weights
}

/// リバランスアクションを生成
///
/// Rebalance アクションのみを生成する。個別の AddPosition/ReducePosition は
/// Rebalance ハンドラ内で差分計算されるため、ここでは不要。
fn generate_rebalance_actions(
    tokens: &[TokenInfo],
    _current_weights: &[f64],
    target_weights: &[f64],
    _rebalance_threshold: f64,
) -> Vec<TradingAction> {
    let target_map: BTreeMap<TokenOutAccount, BigDecimal> = tokens
        .iter()
        .enumerate()
        .filter(|(i, _)| target_weights[*i] > 0.0)
        .map(|(i, token)| (token.symbol.clone(), weight_from_f64(target_weights[i])))
        .collect();

    if target_map.is_empty() {
        return vec![];
    }

    vec![TradingAction::Rebalance {
        target_weights: target_map,
    }]
}

/// ソルティノレシオを計算
///
/// 下方偏差: sqrt( mean( min(r - rf, 0)^2 ) ) （全サンプル数で除算する標準的定義）
fn calculate_sortino_ratio(returns: &[f64], risk_free_rate: f64) -> f64 {
    if returns.is_empty() {
        return 0.0;
    }

    let mean_return: f64 = returns.iter().sum::<f64>() / returns.len() as f64;
    let excess_return = mean_return - risk_free_rate;

    let downside_variance: f64 = returns
        .iter()
        .map(|&r| (r - risk_free_rate).min(0.0).powi(2))
        .sum::<f64>()
        / returns.len() as f64;
    let downside_deviation = downside_variance.sqrt();

    // IEEE 754 guarantees: sqrt(0.0) is exactly 0.0, so exact comparison is correct.
    if downside_deviation == 0.0 {
        0.0
    } else {
        excess_return / downside_deviation
    }
}

/// 最大ドローダウンを計算
fn calculate_max_drawdown(cumulative_returns: &[f64]) -> f64 {
    if cumulative_returns.is_empty() {
        return 0.0;
    }

    let mut peak = cumulative_returns[0];
    let mut max_drawdown = 0.0;

    for &value in cumulative_returns.iter().skip(1) {
        if value > peak {
            peak = value;
        }

        if peak > 0.0 {
            let drawdown = (peak - value) / peak;
            if drawdown > max_drawdown {
                max_drawdown = drawdown;
            }
        }
    }

    max_drawdown
}

#[cfg(test)]
mod tests;
