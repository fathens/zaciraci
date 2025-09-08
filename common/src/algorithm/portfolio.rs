use crate::Result;
use chrono::{DateTime, Utc};
use ndarray::{Array1, Array2};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::types::*;

// ==================== ポートフォリオ固有の型定義 ====================

/// ポートフォリオデータ
#[derive(Debug, Clone)]
pub struct PortfolioData {
    pub tokens: Vec<TokenData>,
    pub predictions: HashMap<String, f64>,
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

// ==================== 定数 ====================

/// リスクフリーレート（年率2%）
const RISK_FREE_RATE: f64 = 0.02;

/// 単一トークンの最大保有比率
const MAX_POSITION_SIZE: f64 = 0.4;

/// 最小保有比率
const MIN_POSITION_SIZE: f64 = 0.05;

/// リバランス閾値（10%）
const REBALANCE_THRESHOLD: f64 = 0.1;

/// 最大保有トークン数
const MAX_HOLDINGS: usize = 10;

/// 最適化の最大反復回数
const MAX_OPTIMIZATION_ITERATIONS: usize = 100;

/// 数値安定性のための正則化パラメータ
const REGULARIZATION_FACTOR: f64 = 1e-6;

// ==================== コア計算関数 ====================

/// 期待リターンを計算
pub fn calculate_expected_returns(
    tokens: &[TokenInfo],
    predictions: &HashMap<String, f64>,
) -> Vec<f64> {
    tokens
        .iter()
        .map(|token| {
            if let Some(&predicted_price) = predictions.get(&token.symbol) {
                let current = token
                    .current_price
                    .to_string()
                    .parse::<f64>()
                    .unwrap_or(0.0);
                if current != 0.0 {
                    (predicted_price - current) / current
                } else {
                    0.0
                }
            } else {
                0.0
            }
        })
        .collect()
}

/// 日次リターンを計算
pub fn calculate_daily_returns(historical_prices: &[PriceHistory]) -> Vec<Vec<f64>> {
    let mut token_prices: HashMap<String, Vec<(DateTime<Utc>, f64)>> = HashMap::new();

    // トークン別に価格データをグループ化
    for price_data in historical_prices {
        for price_point in &price_data.prices {
            let price_f64 = price_point.price.to_string().parse::<f64>().unwrap_or(0.0);
            token_prices
                .entry(price_data.token.clone())
                .or_default()
                .push((price_point.timestamp, price_f64));
        }
    }

    // 各トークンの日次リターンを計算
    let mut returns = Vec::new();
    for (_, mut prices) in token_prices {
        prices.sort_by_key(|&(timestamp, _)| timestamp);

        let mut token_returns = Vec::new();
        for i in 1..prices.len() {
            let return_rate = (prices[i].1 - prices[i - 1].1) / prices[i - 1].1;
            token_returns.push(return_rate);
        }
        returns.push(token_returns);
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
fn calculate_covariance(returns1: &[f64], returns2: &[f64]) -> f64 {
    if returns1.len() != returns2.len() || returns1.is_empty() {
        return 0.0;
    }

    let mean1: f64 = returns1.iter().sum::<f64>() / returns1.len() as f64;
    let mean2: f64 = returns2.iter().sum::<f64>() / returns2.len() as f64;

    let covariance: f64 = returns1
        .iter()
        .zip(returns2.iter())
        .map(|(&r1, &r2)| (r1 - mean1) * (r2 - mean2))
        .sum::<f64>()
        / (returns1.len() - 1) as f64;

    covariance
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

    // 制約付き最適化（簡略版）
    for _ in 0..MAX_OPTIMIZATION_ITERATIONS {
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

    // 最終的な制約チェック
    for w in weights.iter_mut() {
        *w = w.clamp(0.0, MAX_POSITION_SIZE);
    }

    // 最終正規化
    let sum: f64 = weights.iter().sum();
    if sum > 0.0 {
        for w in weights.iter_mut() {
            *w /= sum;
        }
    }
}

/// リバランスが必要かチェック
pub fn needs_rebalancing(current_weights: &[f64], target_weights: &[f64]) -> bool {
    if current_weights.len() != target_weights.len() {
        return true;
    }

    current_weights
        .iter()
        .zip(target_weights.iter())
        .any(|(&current, &target)| (current - target).abs() > REBALANCE_THRESHOLD)
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

// ==================== ポートフォリオ実行 ====================

/// ポートフォリオ最適化戦略を実行
pub async fn execute_portfolio_optimization(
    wallet: &WalletInfo,
    portfolio_data: PortfolioData,
) -> Result<PortfolioExecutionReport> {
    // 期待リターンを計算
    let expected_returns =
        calculate_expected_returns(&portfolio_data.tokens, &portfolio_data.predictions);

    // 日次リターンと共分散行列を計算
    let daily_returns = calculate_daily_returns(&portfolio_data.historical_prices);
    let covariance = calculate_covariance_matrix(&daily_returns);

    // 最適ポートフォリオを計算
    let mut optimal_weights = maximize_sharpe_ratio(&expected_returns, &covariance);

    // リスクパリティ調整（オプション）
    apply_risk_parity(&mut optimal_weights, &covariance);

    // 制約を適用
    apply_constraints(&mut optimal_weights);

    // 現在のポートフォリオ重みを計算
    let current_weights = calculate_current_weights(&portfolio_data.tokens, wallet);

    // リバランスが必要かチェック
    let rebalance_needed = needs_rebalancing(&current_weights, &optimal_weights);

    // アクションを生成
    let actions = if rebalance_needed {
        generate_rebalance_actions(&portfolio_data.tokens, &current_weights, &optimal_weights)
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

    // 重みをHashMapに変換
    let weight_map: HashMap<String, f64> = portfolio_data
        .tokens
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

    let expected_metrics = PortfolioMetrics {
        cumulative_return: portfolio_return,
        annualized_return: portfolio_return * 365.0,
        volatility: portfolio_vol * (365.0_f64).sqrt(),
        sharpe_ratio,
        sortino_ratio: sharpe_ratio, // 簡略化
        max_drawdown: 0.0,           // 将来実装
        calmar_ratio: 0.0,           // 将来実装
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
fn calculate_current_weights(tokens: &[TokenInfo], wallet: &WalletInfo) -> Vec<f64> {
    let mut weights = vec![0.0; tokens.len()];

    for (i, token) in tokens.iter().enumerate() {
        if let Some(&holding) = wallet.holdings.get(&token.symbol) {
            let price_f64 = token
                .current_price
                .to_string()
                .parse::<f64>()
                .unwrap_or(0.0);
            let value = holding * price_f64;
            weights[i] = value / wallet.total_value;
        }
    }

    weights
}

/// リバランスアクションを生成
fn generate_rebalance_actions(
    tokens: &[TokenInfo],
    current_weights: &[f64],
    target_weights: &[f64],
) -> Vec<TradingAction> {
    let mut actions = Vec::new();
    let mut target_map = HashMap::new();

    for (i, token) in tokens.iter().enumerate() {
        if target_weights[i] > 0.0 {
            target_map.insert(token.symbol.clone(), target_weights[i]);
        }

        let weight_diff = target_weights[i] - current_weights[i];
        if weight_diff.abs() > REBALANCE_THRESHOLD {
            if weight_diff > 0.0 {
                // AddPosition equivalent using Hold for now
                actions.push(TradingAction::Hold);
            } else if current_weights[i] > 0.0 {
                // ReducePosition equivalent using Hold for now
                actions.push(TradingAction::Hold);
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
