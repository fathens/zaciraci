use crate::Result;
use crate::types::{NearValue, TokenAmount, TokenInAccount, TokenOutAccount, TokenPrice};
use chrono::{Duration, Utc};
use std::collections::HashMap;

use super::prediction::PredictionProvider;
use super::types::*;

// ==================== 定数 ====================

/// 上位N個のトークンを考慮
pub const TOP_N_TOKENS: usize = 3;

/// 取引手数料（0.3%）
const TRADING_FEE: f64 = 0.003;

/// 最大スリッページ許容率（2%）
const MAX_SLIPPAGE: f64 = 0.02;

// ==================== コアアルゴリズム ====================

/// 予測リターンを計算（取引コスト考慮）
///
/// `TokenPrice.expected_return()` を使用して符号の間違いを防ぐ。
pub fn calculate_expected_return(prediction: &PredictionData) -> f64 {
    if prediction.current_price.is_zero() || prediction.predicted_price_24h.is_zero() {
        return 0.0;
    }

    let raw_return = prediction
        .current_price
        .expected_return(&prediction.predicted_price_24h);

    // 取引コストを考慮
    adjust_for_trading_costs(raw_return)
}

/// 信頼度で調整されたリターンを計算
pub fn calculate_confidence_adjusted_return(prediction: &PredictionData) -> f64 {
    let base_return = calculate_expected_return(prediction);
    let confidence = prediction
        .confidence
        .as_ref()
        .map(|c| c.to_string().parse::<f64>().unwrap_or(0.5))
        .unwrap_or(0.5);

    // 信頼度で調整（信頼度が低い場合はリターンを減少）
    base_return * confidence
}

/// トークンをモメンタムでランキング
pub fn rank_tokens_by_momentum(
    predictions: &[PredictionData],
) -> Vec<(TokenOutAccount, f64, Option<f64>)> {
    let mut ranked: Vec<_> = predictions
        .iter()
        .map(|p| {
            let return_val = calculate_confidence_adjusted_return(p);
            let confidence_f64 = p
                .confidence
                .as_ref()
                .map(|c| c.to_string().parse::<f64>().unwrap_or(0.5));
            (p.token.clone(), return_val, confidence_f64)
        })
        .filter(|(_, return_val, _)| *return_val > 0.0) // 正のリターンのみ
        .collect();

    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // 上位N個に制限
    ranked.truncate(TOP_N_TOKENS);
    ranked
}

/// 取引判断ロジック（改善版）
///
/// # 引数
///
/// * `current_token` - 現在保有しているトークン
/// * `current_return` - 現在のトークンの期待リターン率（無次元）
/// * `ranked_tokens` - ランキングされたトークン（トークン, 期待リターン率, 信頼度）
/// * `holding_amount` - 保有量（TokenAmount）
/// * `holding_price` - 保有トークンの価格（TokenPrice: NEAR/token）
/// * `params` - 取引判断パラメータ（最小利益閾値、スイッチ乗数、最小取引価値）
pub fn make_trading_decision(
    current_token: &TokenOutAccount,
    current_return: f64,
    ranked_tokens: &[(TokenOutAccount, f64, Option<f64>)],
    holding_amount: &TokenAmount,
    holding_price: &TokenPrice,
    params: &TradingDecisionParams,
) -> TradingAction {
    // 空の場合はHold
    if ranked_tokens.is_empty() {
        return TradingAction::Hold;
    }

    let best_token = &ranked_tokens[0];

    // 現在のトークンが最良の場合はHold
    if &best_token.0 == current_token {
        return TradingAction::Hold;
    }

    // 保有価値が最小取引価値以下の場合はHold
    // TokenAmount × TokenPrice = NearValue（型安全な変換）
    let holding_value: NearValue = holding_amount * holding_price;
    if holding_value < params.min_trade_value {
        return TradingAction::Hold;
    }

    // 現在のトークンの期待リターンが閾値以下
    if current_return < params.min_profit_threshold {
        return TradingAction::Sell {
            token: current_token.clone(),
            target: best_token.0.clone(),
        };
    }

    // より良いトークンが存在する場合（信頼度も考慮）
    let confidence_factor = best_token.2.unwrap_or(0.5);
    if best_token.1 > current_return * params.switch_multiplier * confidence_factor {
        return TradingAction::Switch {
            from: current_token.clone(),
            to: best_token.0.clone(),
        };
    }

    TradingAction::Hold
}

// ==================== 実行フロー ====================

/// モメンタム戦略の実行（改善版）
///
/// # 引数
///
/// * `params` - 取引判断パラメータ
pub async fn execute_momentum_strategy(
    current_holdings: Vec<TokenHolding>,
    predictions: &[PredictionData],
    params: &TradingDecisionParams,
) -> Result<ExecutionReport> {
    // トークンをランキング
    let ranked = rank_tokens_by_momentum(predictions);

    // 予測データをHashMapに変換（高速検索用）
    let prediction_map: HashMap<TokenOutAccount, &PredictionData> =
        predictions.iter().map(|p| (p.token.clone(), p)).collect();

    // 各保有トークンについて判断
    let mut actions = Vec::new();
    for holding in current_holdings {
        // 保有トークンの予測データを取得
        let current_return = if let Some(pred_data) = prediction_map.get(&holding.token) {
            calculate_confidence_adjusted_return(pred_data)
        } else {
            // 予測データがない場合は0とする
            0.0
        };

        // ExchangeRate → TokenPrice に変換
        let holding_price = holding.current_rate.to_price();

        let action = make_trading_decision(
            &holding.token,
            current_return,
            &ranked,
            &holding.amount,
            &holding_price,
            params,
        );

        if action != TradingAction::Hold {
            actions.push(action);
        }
    }

    let mut report = ExecutionReport::new(actions, AlgorithmType::Momentum);

    // 期待リターンを計算（最良トークンのリターン）
    if !ranked.is_empty() {
        report.expected_return = Some(ranked[0].1);
    }

    Ok(report)
}

/// PredictionProviderを使用した戦略実行
///
/// # 引数
///
/// * `params` - 取引判断パラメータ
pub async fn execute_with_prediction_provider<P: PredictionProvider>(
    prediction_provider: &P,
    current_holdings: Vec<TokenHolding>,
    quote_token: &TokenInAccount,
    history_days: i64,
    params: &TradingDecisionParams,
) -> Result<ExecutionReport> {
    // 保有トークンの予測を取得
    let tokens: Vec<TokenOutAccount> = current_holdings.iter().map(|h| h.token.clone()).collect();

    let predictions_map = prediction_provider
        .predict_multiple_tokens(tokens, quote_token, history_days, 24)
        .await?;

    // PredictionDataに変換
    let mut prediction_data = Vec::new();
    for holding in &current_holdings {
        if let Some(prediction) = predictions_map.get(&holding.token)
            && let Some(data) =
                PredictionData::from_token_prediction(prediction, holding.current_rate.to_price())
        {
            prediction_data.push(data);
        }
    }

    // 追加: トップトークンの予測も取得
    let end_date = Utc::now();
    let start_date = end_date - Duration::days(history_days);
    let all_tokens = prediction_provider
        .get_tokens_by_volatility(start_date, end_date, quote_token)
        .await?;
    let top_tokens: Vec<_> = all_tokens.into_iter().take(TOP_N_TOKENS).collect();

    // トップトークンの予測を追加
    for top_token in top_tokens {
        // 既に予測済みのトークンはスキップ
        if current_holdings.iter().any(|h| h.token == top_token.token) {
            continue;
        }

        let history = prediction_provider
            .get_price_history(&top_token.token, quote_token, start_date, end_date)
            .await?;

        let prediction = prediction_provider.predict_price(&history, 24).await?;

        // 価格履歴から現在価格を取得
        let current_price = history
            .prices
            .last()
            .map(|p| p.price.clone())
            .unwrap_or_else(TokenPrice::zero);
        if let Some(data) = PredictionData::from_token_prediction(&prediction, current_price) {
            prediction_data.push(data);
        }
    }

    // 戦略を実行
    execute_momentum_strategy(current_holdings, &prediction_data, params).await
}

// ==================== 改善機能 ====================

/// 取引コストを考慮した期待リターンの調整
pub fn adjust_for_trading_costs(expected_return: f64) -> f64 {
    // 往復の手数料とスリッページを考慮
    expected_return - (2.0 * TRADING_FEE) - MAX_SLIPPAGE
}

/// 信頼度と期待リターンに基づくポジションサイズの計算
pub fn calculate_position_size(confidence_score: f64, expected_return: f64) -> f64 {
    // 信頼度と期待リターンに基づくポジションサイズ
    (confidence_score * expected_return).clamp(0.0, 1.0)
}

/// ボラティリティによるフィルタリング
pub async fn filter_by_volatility(
    tokens: Vec<(String, f64, Option<f64>)>,
    max_volatility: f64,
    historical_prices: &HashMap<String, Vec<f64>>,
) -> Result<Vec<(String, f64, Option<f64>)>> {
    let mut filtered = Vec::new();

    for (token, return_val, confidence) in tokens {
        if let Some(prices) = historical_prices.get(&token) {
            let volatility = calculate_volatility(prices);
            if volatility <= max_volatility {
                filtered.push((token, return_val, confidence));
            }
        } else {
            // 価格データがない場合はフィルタリング対象外
            filtered.push((token, return_val, confidence));
        }
    }

    Ok(filtered)
}

/// ボラティリティ計算（標準偏差）
fn calculate_volatility(prices: &[f64]) -> f64 {
    crate::algorithm::calculate_volatility_from_prices(prices)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod integration_tests;
