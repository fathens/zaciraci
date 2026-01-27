use crate::Result;
use crate::types::{NearValue, TokenOutAccount, TokenPrice};
use bigdecimal::{BigDecimal, ToPrimitive};
use chrono::{DateTime, Utc};
use ndarray::{Array1, Array2};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::types::*;

// ==================== ポートフォリオ固有の型定義 ====================

/// ポートフォリオデータ
#[derive(Debug, Clone)]
pub struct PortfolioData {
    pub tokens: Vec<TokenData>,
    /// 予測価格（TokenPrice: NEAR/token）
    pub predictions: BTreeMap<TokenOutAccount, TokenPrice>,
    pub historical_prices: Vec<PriceHistory>,
    pub correlation_matrix: Option<Array2<f64>>,
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

/// トークンスコア
#[derive(Debug, Clone)]
pub struct TokenScore {
    pub token: String,
    pub sharpe_ratio: f64,
    pub liquidity_score: f64,
    pub prediction_confidence: f64,
    pub volatility_rank: f64,
    pub composite_score: f64,
}

// ==================== 定数 ====================

/// リスクフリーレート（年率2%）
const RISK_FREE_RATE: f64 = 0.02;

/// 単一トークンの最大保有比率（積極的設定）
const MAX_POSITION_SIZE: f64 = 0.6;

/// 最小保有比率
const MIN_POSITION_SIZE: f64 = 0.05;

/// 最大保有トークン数（集中投資）
const MAX_HOLDINGS: usize = 6;

/// 最適化の最大反復回数
const MAX_OPTIMIZATION_ITERATIONS: usize = 100;

/// 数値安定性のための正則化パラメータ
const REGULARIZATION_FACTOR: f64 = 1e-6;

/// 最小流動性スコア
const MIN_LIQUIDITY_SCORE: f64 = 0.1;

/// 最小市場規模（10,000 NEAR）
fn min_market_cap() -> NearValue {
    NearValue::from_near(BigDecimal::from(10000))
}

/// 動的リスク調整の閾値
const HIGH_VOLATILITY_THRESHOLD: f64 = 0.3; // 30%
const LOW_VOLATILITY_THRESHOLD: f64 = 0.1; // 10%

/// 最大相関閾値
const MAX_CORRELATION_THRESHOLD: f64 = 0.7;

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
/// **重要**: この関数は入力されたhistorical_pricesの順序を保持します。
/// BTreeMapを使用して決定的な処理を行いますが、結果の順序は入力順序に従います。
pub fn calculate_daily_returns(historical_prices: &[PriceHistory]) -> Vec<Vec<f64>> {
    let mut token_prices: BTreeMap<String, Vec<(DateTime<Utc>, f64)>> = BTreeMap::new();

    // トークン別に価格データをグループ化
    for price_data in historical_prices {
        for price_point in &price_data.prices {
            // Price型（比率）をf64に変換
            let price_f64 = price_point.price.to_string().parse::<f64>().unwrap_or(0.0);
            token_prices
                .entry(price_data.token.to_string())
                .or_default()
                .push((price_point.timestamp, price_f64));
        }
    }

    // 入力順序を保持: 元の配列から重複を除いたトークン順序を取得
    let unique_tokens: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        historical_prices
            .iter()
            .filter_map(|p| {
                let token_str = p.token.to_string();
                if seen.insert(token_str.clone()) {
                    Some(token_str)
                } else {
                    None
                }
            })
            .collect()
    };

    // 入力順序でリターンを計算
    let mut returns = Vec::new();
    for token in unique_tokens {
        if let Some(mut prices) = token_prices.get(&token).cloned() {
            prices.sort_by_key(|&(timestamp, _)| timestamp);

            let mut token_returns = Vec::new();
            for i in 1..prices.len() {
                if prices[i - 1].1 > 0.0 {
                    let return_rate = (prices[i].1 - prices[i - 1].1) / prices[i - 1].1;
                    token_returns.push(return_rate);
                }
            }
            returns.push(token_returns);
        }
    }

    returns
}

/// 共分散行列を計算
pub fn calculate_covariance_matrix(daily_returns: &[Vec<f64>]) -> Array2<f64> {
    let n = daily_returns.len();
    if n == 0 {
        return Array2::zeros((0, 0));
    }

    let mut covariance = Array2::zeros((n, n));

    for i in 0..n {
        for j in 0..n {
            let cov = calculate_covariance(&daily_returns[i], &daily_returns[j]);
            covariance[[i, j]] = cov;
        }
    }

    // 正則化（数値安定性のため）
    for i in 0..n {
        covariance[[i, i]] += REGULARIZATION_FACTOR;
    }

    covariance
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

/// シャープレシオを最大化する最適ポートフォリオを計算
pub fn maximize_sharpe_ratio(
    expected_returns: &[f64],
    covariance_matrix: &Array2<f64>,
) -> Vec<f64> {
    let n = expected_returns.len();
    if n == 0 {
        return vec![];
    }

    let mut best_weights = vec![1.0 / n as f64; n];
    let mut best_sharpe = f64::NEG_INFINITY;

    // 目標リターンの範囲を設定
    let min_return = expected_returns
        .iter()
        .cloned()
        .fold(f64::INFINITY, f64::min);
    let max_return = expected_returns
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);

    // 全トークンの期待リターンが同一の場合、グリッドサーチは無意味（全反復が同じ target_return）
    // 等配分が最小分散解の良い近似なので early return
    if (max_return - min_return).abs() < 1e-12 {
        return best_weights;
    }

    // グリッドサーチで最適化
    for i in 0..50 {
        let target_return = min_return + (max_return - min_return) * i as f64 / 49.0;

        if let Ok(weights) =
            calculate_efficient_frontier(expected_returns, covariance_matrix, target_return)
        {
            let portfolio_return = calculate_portfolio_return(&weights, expected_returns);
            let portfolio_std = calculate_portfolio_std(&weights, covariance_matrix);

            let sharpe = (portfolio_return - RISK_FREE_RATE / 365.0) / portfolio_std;

            if sharpe > best_sharpe && portfolio_std > 0.0 {
                best_sharpe = sharpe;
                best_weights = weights;
            }
        }
    }

    best_weights
}

/// 効率的フロンティア上の最適ポートフォリオを計算
pub fn calculate_efficient_frontier(
    expected_returns: &[f64],
    covariance_matrix: &Array2<f64>,
    target_return: f64,
) -> Result<Vec<f64>> {
    let n = expected_returns.len();
    if n == 0 {
        return Ok(vec![]);
    }

    // 初期解: 等配分
    let mut weights = vec![1.0 / n as f64; n];

    // 制約付き最適化（収束判定付き）
    for _ in 0..MAX_OPTIMIZATION_ITERATIONS {
        let prev_weights = weights.clone();

        weights =
            optimize_weights_step(&weights, expected_returns, covariance_matrix, target_return);

        // 制約を適用
        apply_individual_constraints(&mut weights);

        // 正規化
        let sum: f64 = weights.iter().sum();
        if sum > 0.0 {
            for w in weights.iter_mut() {
                *w /= sum;
            }
        }

        // 収束判定: weight変化量が十分小さければ早期終了
        let max_change = weights
            .iter()
            .zip(prev_weights.iter())
            .map(|(w, pw)| (w - pw).abs())
            .fold(0.0_f64, f64::max);
        if max_change < 1e-6 {
            break;
        }
    }

    Ok(weights)
}

/// 最適化の1ステップ
fn optimize_weights_step(
    current_weights: &[f64],
    expected_returns: &[f64],
    covariance_matrix: &Array2<f64>,
    target_return: f64,
) -> Vec<f64> {
    let n = current_weights.len();
    let mut new_weights = current_weights.to_vec();

    let current_return = calculate_portfolio_return(current_weights, expected_returns);
    let return_diff = target_return - current_return;

    // リターン調整
    for i in 0..n {
        let adjustment = return_diff * expected_returns[i] * 0.1; // 学習率0.1
        new_weights[i] = (current_weights[i] + adjustment).max(0.0);
    }

    // リスク調整（分散最小化方向）
    let w = Array1::from(current_weights.to_vec());
    let risk_gradient = 2.0 * covariance_matrix.dot(&w);

    for i in 0..n {
        let risk_adjustment = -risk_gradient[i] * 0.01; // 小さな学習率
        new_weights[i] = (new_weights[i] + risk_adjustment).max(0.0);
    }

    new_weights
}

/// リスクパリティ調整
pub fn apply_risk_parity(weights: &mut [f64], covariance_matrix: &Array2<f64>) {
    let n = weights.len();
    if n == 0 {
        return;
    }

    // 各資産のリスク寄与度を計算
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
            let adjustment = target_risk_contribution / current_risk_contribution;
            weights[i] *= adjustment.clamp(0.5, 2.0); // 極端な調整を制限
        }
    }

    // 正規化
    let sum: f64 = weights.iter().sum();
    if sum > 0.0 {
        for w in weights.iter_mut() {
            *w /= sum;
        }
    }
}

// ==================== 制約の適用 ====================

/// 個別制約を適用
fn apply_individual_constraints(weights: &mut [f64]) {
    for w in weights.iter_mut() {
        *w = w.clamp(0.0, MAX_POSITION_SIZE);
    }
}

/// 全体制約を適用
pub fn apply_constraints(weights: &mut [f64]) {
    // 反復的に制約を適用（収束まで）
    for _ in 0..10 {
        // 最大10回の反復
        let mut changed = false;

        // 個別制約
        for w in weights.iter_mut() {
            let old_w = *w;
            *w = w.clamp(0.0, MAX_POSITION_SIZE);
            if (*w - old_w).abs() > 1e-10 {
                changed = true;
            }
        }

        // 上位N個のみ保有
        if weights.len() > MAX_HOLDINGS {
            let mut indexed_weights: Vec<(usize, f64)> =
                weights.iter().enumerate().map(|(i, &w)| (i, w)).collect();
            indexed_weights
                .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            let indices_to_zero: Vec<usize> = indexed_weights[MAX_HOLDINGS..]
                .iter()
                .map(|(i, _)| *i)
                .collect();
            for idx in indices_to_zero {
                if weights[idx] > 0.0 {
                    weights[idx] = 0.0;
                    changed = true;
                }
            }
        }

        // 最小ポジションサイズフィルタ
        for w in weights.iter_mut() {
            if *w > 0.0 && *w < MIN_POSITION_SIZE {
                *w = 0.0;
                changed = true;
            }
        }

        // 正規化
        let sum: f64 = weights.iter().sum();
        if sum > 0.0 {
            for w in weights.iter_mut() {
                *w /= sum;
            }
        }

        // 変化がなくなったら終了
        if !changed {
            break;
        }
    }

    // 最終的な制約チェックと正規化（収束ループ）
    // clamp と normalize を繰り返し、両方の制約を同時に満たす
    for _ in 0..10 {
        for w in weights.iter_mut() {
            *w = w.clamp(0.0, MAX_POSITION_SIZE);
        }
        let sum: f64 = weights.iter().sum();
        if sum > 0.0 {
            for w in weights.iter_mut() {
                *w /= sum;
            }
        }
        // 全要素が MAX_POSITION_SIZE 以内なら収束
        if weights.iter().all(|&w| w <= MAX_POSITION_SIZE + 1e-10) {
            break;
        }
    }
}

/// 市場ボラティリティに基づく動的リスク調整
fn calculate_dynamic_risk_adjustment(historical_prices: &[PriceHistory]) -> f64 {
    let daily_returns = calculate_daily_returns(historical_prices);

    if daily_returns.is_empty() {
        return 1.0; // デフォルト（調整なし）
    }

    // 全トークンの平均ボラティリティを計算
    let avg_volatility = daily_returns
        .iter()
        .map(|returns| {
            if returns.len() < 2 {
                return 0.0;
            }
            let mean = returns.iter().sum::<f64>() / returns.len() as f64;
            let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>()
                / (returns.len() - 1) as f64;
            variance.sqrt() * (365.25_f64).sqrt() // 年率換算
        })
        .sum::<f64>()
        / daily_returns.len() as f64;

    // 動的調整係数を計算
    if avg_volatility > HIGH_VOLATILITY_THRESHOLD {
        // 高ボラティリティ：リスクを抑制
        0.7
    } else if avg_volatility < LOW_VOLATILITY_THRESHOLD {
        // 低ボラティリティ：より積極的に
        1.4
    } else {
        // 中程度：線形補間
        let ratio = (avg_volatility - LOW_VOLATILITY_THRESHOLD)
            / (HIGH_VOLATILITY_THRESHOLD - LOW_VOLATILITY_THRESHOLD);
        1.4 - (1.4 - 0.7) * ratio
    }
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

// ==================== トークン選択 ====================

/// 個別トークンのシャープレシオを計算
fn calculate_individual_sharpe(
    token: &TokenData,
    historical_prices: &[PriceHistory],
    expected_return: f64,
) -> f64 {
    // トークンの価格履歴を取得
    let token_prices = historical_prices
        .iter()
        .find(|p| p.token == token.symbol)
        .map(|p| &p.prices);

    if let Some(prices) = token_prices
        && prices.len() > 1
    {
        let returns = calculate_returns_from_prices(prices);

        if !returns.is_empty() {
            let volatility = calculate_std_dev(&returns);
            if volatility > 0.0 {
                return (expected_return - RISK_FREE_RATE / 365.0) / volatility;
            }
        }
    }

    0.0
}

/// 標準偏差を計算
fn calculate_std_dev(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }

    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance =
        values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (values.len() - 1) as f64;

    variance.sqrt()
}

/// トークンのスコアを計算
fn calculate_token_score(
    token: &TokenData,
    prediction: Option<&TokenPrice>,
    historical_prices: &[PriceHistory],
    all_volatilities: &[f64],
) -> TokenScore {
    // 予測価格から期待リターンを計算
    let expected_return = if let Some(predicted_price) = prediction {
        let current_price = token.current_rate.to_price();
        if current_price.is_zero() || predicted_price.is_zero() {
            0.0
        } else {
            current_price.expected_return(predicted_price)
        }
    } else {
        0.0
    };
    let sharpe = calculate_individual_sharpe(token, historical_prices, expected_return);
    let liquidity = token.liquidity_score.unwrap_or(0.0);
    let confidence = 0.5; // デフォルト信頼度

    // ボラティリティランクを計算（低いほど良い）
    let vol_rank = if !all_volatilities.is_empty() {
        let sorted_vols = {
            let mut v = all_volatilities.to_vec();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            v
        };
        let position = sorted_vols
            .iter()
            .position(|&v| v >= token.historical_volatility)
            .unwrap_or(sorted_vols.len());
        1.0 - (position as f64 / sorted_vols.len() as f64)
    } else {
        0.5
    };

    // 総合スコアを計算
    let composite = sharpe.max(0.0) * 0.4 + liquidity * 0.2 + confidence * 0.2 + vol_rank * 0.2;

    TokenScore {
        token: token.symbol.to_string(),
        sharpe_ratio: sharpe,
        liquidity_score: liquidity,
        prediction_confidence: confidence,
        volatility_rank: vol_rank,
        composite_score: composite,
    }
}

/// 最適なトークンを選択
pub fn select_optimal_tokens(
    tokens: &[TokenData],
    predictions: &BTreeMap<TokenOutAccount, TokenPrice>,
    historical_prices: &[PriceHistory],
    max_tokens: usize,
) -> Vec<TokenData> {
    // フィルタリング: 最小要件を満たすトークンのみ
    let min_cap = min_market_cap();
    let filtered_tokens: Vec<&TokenData> = tokens
        .iter()
        .filter(|t| {
            // 実際のデータ構造に合わせたフィルタリング条件
            // market_capがNoneの場合は流動性スコアのみでフィルタ
            let liquidity_ok = t.liquidity_score.unwrap_or(0.0) >= MIN_LIQUIDITY_SCORE;
            let market_cap_ok = match &t.market_cap {
                Some(cap) => cap >= &min_cap,
                None => true, // market_capがNoneの場合はスキップ
            };
            liquidity_ok && market_cap_ok
        })
        .collect();

    if filtered_tokens.is_empty() {
        // フィルタ条件が厳しすぎる場合は全トークンから選択
        return tokens.iter().take(max_tokens).cloned().collect();
    }

    // 全トークンのボラティリティを収集
    let all_volatilities: Vec<f64> = filtered_tokens
        .iter()
        .map(|t| t.historical_volatility)
        .collect();

    // スコアリング
    let mut scored_tokens: Vec<(TokenScore, &TokenData)> = filtered_tokens
        .iter()
        .map(|&token| {
            let prediction = predictions.get(&token.symbol);
            let score =
                calculate_token_score(token, prediction, historical_prices, &all_volatilities);
            (score, token)
        })
        .collect();

    // スコアでソート（決定的）
    scored_tokens.sort_by(|a, b| {
        b.0.composite_score
            .partial_cmp(&a.0.composite_score)
            .unwrap()
    });

    // 相関を考慮した選択
    select_uncorrelated_tokens(scored_tokens, historical_prices, max_tokens)
}

/// 相関の低いトークンを選択
fn select_uncorrelated_tokens(
    scored_tokens: Vec<(TokenScore, &TokenData)>,
    historical_prices: &[PriceHistory],
    max_tokens: usize,
) -> Vec<TokenData> {
    if scored_tokens.is_empty() {
        return Vec::new();
    }

    // 最高スコアのトークンを最初に選択
    let mut selected = vec![scored_tokens[0].1.clone()];

    for (_score, token) in scored_tokens.iter().skip(1) {
        if selected.len() >= max_tokens {
            break;
        }

        // 既存選択トークンとの平均相関を計算
        let mut correlations = Vec::new();
        for selected_token in &selected {
            let correlation = calculate_token_correlation(
                &token.symbol.to_string(),
                &selected_token.symbol.to_string(),
                historical_prices,
            );
            correlations.push(correlation.abs());
        }

        let avg_correlation = if !correlations.is_empty() {
            correlations.iter().sum::<f64>() / correlations.len() as f64
        } else {
            0.0
        };

        // 相関が閾値以下なら追加
        if avg_correlation < MAX_CORRELATION_THRESHOLD {
            selected.push((*token).clone());
        }
    }

    selected
}

/// 2つのトークン間の相関を計算
fn calculate_token_correlation(
    token1: &str,
    token2: &str,
    historical_prices: &[PriceHistory],
) -> f64 {
    // トークンの価格履歴を取得
    let prices1 = historical_prices
        .iter()
        .find(|p| p.token.to_string() == token1)
        .map(|p| &p.prices);
    let prices2 = historical_prices
        .iter()
        .find(|p| p.token.to_string() == token2)
        .map(|p| &p.prices);

    if let (Some(p1), Some(p2)) = (prices1, prices2) {
        // 日次リターンを計算
        let returns1 = calculate_returns_from_prices(p1);
        let returns2 = calculate_returns_from_prices(p2);

        // 長さが異なる場合は末尾（最新データ）を優先してトリミング
        let min_len = returns1.len().min(returns2.len());
        if min_len >= 2 {
            let r1 = &returns1[returns1.len() - min_len..];
            let r2 = &returns2[returns2.len() - min_len..];

            let std1 = calculate_std_dev(r1);
            let std2 = calculate_std_dev(r2);

            // 標準偏差が0の場合は相関を0とする
            if std1 > 0.0 && std2 > 0.0 {
                let correlation = calculate_covariance(r1, r2) / (std1 * std2);
                // 相関係数を-1から1の範囲にクリップ
                return correlation.clamp(-1.0, 1.0);
            }
        }
    }

    0.0
}

/// 価格からリターンを計算
fn calculate_returns_from_prices(prices: &[PricePoint]) -> Vec<f64> {
    let mut returns = Vec::new();
    for i in 1..prices.len() {
        let price_current = prices[i].price.to_string().parse::<f64>().unwrap_or(0.0);
        let price_prev = prices[i - 1]
            .price
            .to_string()
            .parse::<f64>()
            .unwrap_or(0.0);
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
    // トークン選択を実施
    let selected_tokens = select_optimal_tokens(
        &portfolio_data.tokens,
        &portfolio_data.predictions,
        &portfolio_data.historical_prices,
        10, // 最大10トークンまで
    );

    // 選択されたトークンのみでポートフォリオを構築
    let selected_predictions: BTreeMap<TokenOutAccount, TokenPrice> = portfolio_data
        .predictions
        .iter()
        .filter(|(symbol, _)| selected_tokens.iter().any(|t| &t.symbol == *symbol))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    // 期待リターンを計算
    let expected_returns = calculate_expected_returns(&selected_tokens, &selected_predictions);

    // 選択されたトークンの価格履歴のみをフィルタ
    let selected_price_histories: Vec<PriceHistory> = portfolio_data
        .historical_prices
        .iter()
        .filter(|p| selected_tokens.iter().any(|t| t.symbol == p.token))
        .cloned()
        .collect();

    // 日次リターンと共分散行列を計算
    let daily_returns = calculate_daily_returns(&selected_price_histories);
    let covariance = calculate_covariance_matrix(&daily_returns);

    // 動的リスク調整係数を計算（選択後トークンのボラティリティに基づく）
    let risk_adjustment = calculate_dynamic_risk_adjustment(&selected_price_histories);

    // Issue 1: 期待リターンにリスク調整を適用（高ボラ時は期待リターン縮小）
    let adjusted_returns: Vec<f64> = expected_returns
        .iter()
        .map(|&r| r * risk_adjustment)
        .collect();

    // Sharpe 最適化（リスク調整済みリターンを使用）
    let raw_weights = maximize_sharpe_ratio(&adjusted_returns, &covariance);
    let (validated, had_invalid) = validate_weights(&raw_weights);
    debug_assert!(
        !had_invalid,
        "maximize_sharpe_ratio returned non-finite weights: {:?}",
        raw_weights
    );
    let w_sharpe = validated;

    // Issue 2: Risk Parity を独立に計算（等配分から開始）
    let n = w_sharpe.len();
    let mut w_rp = vec![1.0 / n as f64; n];
    apply_risk_parity(&mut w_rp, &covariance);

    // risk_adjustment 連動 alpha でブレンド（範囲 [0.7, 0.9]）
    // risk_adjustment: 0.7 (高ボラ) → 1.4 (低ボラ)
    // alpha: 0.7 (RP補助) → 0.9 (Sharpe主導)
    let alpha = ((risk_adjustment - 0.7) / (1.4 - 0.7) * (0.9 - 0.7) + 0.7).clamp(0.7, 0.9);

    let mut optimal_weights: Vec<f64> = w_sharpe
        .iter()
        .zip(w_rp.iter())
        .map(|(&ws, &wr)| alpha * ws + (1.0 - alpha) * wr)
        .collect();

    // 制約を適用
    apply_constraints(&mut optimal_weights);

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

    // メトリクスを計算
    let portfolio_return = calculate_portfolio_return(&optimal_weights, &expected_returns);
    let portfolio_vol = calculate_portfolio_std(&optimal_weights, &covariance);
    let sharpe_ratio = if portfolio_vol > 0.0 {
        (portfolio_return - RISK_FREE_RATE / 365.0) / portfolio_vol
    } else {
        0.0
    };

    // 重みをBTreeMapに変換（順序安定化のため）
    let weight_map: BTreeMap<TokenOutAccount, f64> = selected_tokens
        .iter()
        .zip(optimal_weights.iter())
        .filter(|&(_, weight)| *weight > 0.0)
        .map(|(token, weight)| (token.symbol.clone(), *weight))
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
                    .map(|(w, returns)| w * returns[day])
                    .sum()
            })
            .collect()
    } else {
        vec![]
    };

    let daily_risk_free = RISK_FREE_RATE / 365.0;
    let sortino_ratio = super::calculate_sortino_ratio(&portfolio_daily_returns, daily_risk_free);

    let cumulative_values: Vec<f64> = {
        let mut vals = Vec::with_capacity(portfolio_daily_returns.len() + 1);
        vals.push(1.0);
        for &r in &portfolio_daily_returns {
            vals.push(vals.last().unwrap() * (1.0 + r));
        }
        vals
    };
    let max_drawdown = super::calculate_max_drawdown(&cumulative_values);

    let calmar_ratio = if max_drawdown > 0.0 {
        portfolio_return / max_drawdown
    } else {
        0.0
    };

    let expected_metrics = PortfolioMetrics {
        cumulative_return: portfolio_return,
        daily_return: portfolio_return,
        volatility: portfolio_vol,
        sharpe_ratio,
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
                let value_near_f64 = value_near.to_f64();
                println!(
                    "Token {}: rate={}, holding={}, value_near={:.6}, weight={:.6}%",
                    token.symbol,
                    token.current_rate,
                    holding,
                    value_near_f64.as_f64(),
                    weights[i] * 100.0
                );

                if value_near_f64.as_f64() > 100.0 {
                    // 100 NEAR以上の場合は警告
                    println!(
                        "WARNING: Token {} has unusually high value: {:.6} NEAR",
                        token.symbol,
                        value_near_f64.as_f64()
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
fn generate_rebalance_actions(
    tokens: &[TokenInfo],
    current_weights: &[f64],
    target_weights: &[f64],
    rebalance_threshold: f64,
) -> Vec<TradingAction> {
    let mut actions = Vec::new();
    let mut target_map = BTreeMap::new();

    for (i, token) in tokens.iter().enumerate() {
        if target_weights[i] > 0.0 {
            target_map.insert(token.symbol.clone(), target_weights[i]);
        }

        let weight_diff = target_weights[i] - current_weights[i];
        if weight_diff.abs() > rebalance_threshold {
            if weight_diff > 0.0 {
                actions.push(TradingAction::AddPosition {
                    token: token.symbol.clone(),
                    weight: target_weights[i],
                });
            } else if current_weights[i] > 0.0 {
                actions.push(TradingAction::ReducePosition {
                    token: token.symbol.clone(),
                    weight: target_weights[i],
                });
            }
        }
    }

    if !target_map.is_empty() {
        actions.push(TradingAction::Rebalance {
            target_weights: target_map,
        });
    }

    actions
}

#[cfg(test)]
mod tests;
