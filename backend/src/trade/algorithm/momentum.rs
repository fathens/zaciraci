use crate::Result;
use crate::persistence::token_rate::TokenRate;
use crate::trade::predict::{PredictionService, TokenPrediction};
use bigdecimal::BigDecimal;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ==================== 型定義 ====================

/// 予測データを格納する構造体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionData {
    pub token: String,
    pub current_price: BigDecimal,
    pub predicted_price_24h: BigDecimal,
    pub timestamp: DateTime<Utc>,
    pub confidence: Option<f64>,
}

impl PredictionData {
    /// TokenPredictionから変換
    #[allow(dead_code)]
    pub fn from_token_prediction(
        prediction: &TokenPrediction,
        current_price: BigDecimal,
    ) -> Option<Self> {
        // 24時間後の予測価格を取得
        let predicted_24h = prediction.predictions.iter().find(|p| {
            let diff = p.timestamp - prediction.prediction_time;
            diff >= Duration::hours(23) && diff <= Duration::hours(25)
        })?;

        Some(Self {
            token: prediction.token.clone(),
            current_price,
            predicted_price_24h: BigDecimal::from(predicted_24h.price as i64),
            timestamp: prediction.prediction_time,
            confidence: predicted_24h.confidence,
        })
    }
}

/// 取引アクション
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TradingAction {
    /// トークンを保持
    Hold,
    /// トークンを売却して別のトークンに切り替え
    Sell { token: String, target: String },
    /// あるトークンから別のトークンへ切り替え
    Switch { from: String, to: String },
}

/// 実行レポート
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReport {
    pub actions: Vec<TradingAction>,
    pub timestamp: DateTime<Utc>,
    pub expected_return: Option<f64>,
    pub total_trades: usize,
    pub success_count: usize,
    pub failed_count: usize,
    pub skipped_count: usize,
}

impl ExecutionReport {
    #[allow(dead_code)]
    pub fn new(actions: Vec<TradingAction>) -> Self {
        let total_trades = actions.len();
        Self {
            actions,
            timestamp: Utc::now(),
            expected_return: None,
            total_trades,
            success_count: 0,
            failed_count: 0,
            skipped_count: 0,
        }
    }

    #[allow(dead_code)]
    pub fn mark_success(&mut self) {
        self.success_count += 1;
    }

    #[allow(dead_code)]
    pub fn mark_failed(&mut self) {
        self.failed_count += 1;
    }

    #[allow(dead_code)]
    pub fn mark_skipped(&mut self) {
        self.skipped_count += 1;
    }
}

/// ウォレットの保有情報
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TokenHolding {
    pub token: String,
    pub amount: BigDecimal,
    pub current_price: BigDecimal,
}

// ==================== 定数 ====================

/// 最低利益率閾値（5%）
#[allow(dead_code)]
const MIN_PROFIT_THRESHOLD: f64 = 0.05;

/// 切り替え倍率（1.5倍以上の利益で切り替え）
#[allow(dead_code)]
const SWITCH_MULTIPLIER: f64 = 1.5;

/// 上位N個のトークンを考慮
#[allow(dead_code)]
const TOP_N_TOKENS: usize = 3;

/// 取引手数料（0.3%）
#[allow(dead_code)]
const TRADING_FEE: f64 = 0.003;

/// 最小取引額（NEAR）
#[allow(dead_code)]
const MIN_TRADE_AMOUNT: f64 = 1.0;

/// 最大スリッページ許容率（2%）
#[allow(dead_code)]
const MAX_SLIPPAGE: f64 = 0.02;

// ==================== コアアルゴリズム ====================

/// 予測リターンを計算（取引コスト考慮）
#[allow(dead_code)]
pub fn calculate_expected_return(prediction: &PredictionData) -> f64 {
    let current = prediction
        .current_price
        .to_string()
        .parse::<f64>()
        .unwrap_or(0.0);
    let predicted = prediction
        .predicted_price_24h
        .to_string()
        .parse::<f64>()
        .unwrap_or(0.0);

    if current == 0.0 {
        return 0.0;
    }

    let raw_return = (predicted - current) / current;

    // 取引コストを考慮
    adjust_for_trading_costs(raw_return)
}

/// 信頼度で調整されたリターンを計算
#[allow(dead_code)]
pub fn calculate_confidence_adjusted_return(prediction: &PredictionData) -> f64 {
    let base_return = calculate_expected_return(prediction);
    let confidence = prediction.confidence.unwrap_or(0.5);

    // 信頼度で調整（信頼度が低い場合はリターンを減少）
    base_return * confidence
}

/// トークンをモメンタムでランキング
#[allow(dead_code)]
pub fn rank_tokens_by_momentum(
    predictions: Vec<PredictionData>,
) -> Vec<(String, f64, Option<f64>)> {
    let mut ranked: Vec<_> = predictions
        .iter()
        .map(|p| {
            let return_val = calculate_confidence_adjusted_return(p);
            (p.token.clone(), return_val, p.confidence)
        })
        .filter(|(_, return_val, _)| *return_val > 0.0) // 正のリターンのみ
        .collect();

    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // 上位N個に制限
    ranked.truncate(TOP_N_TOKENS);
    ranked
}

/// 取引判断ロジック（改善版）
#[allow(dead_code)]
pub fn make_trading_decision(
    current_token: &str,
    current_return: f64,
    ranked_tokens: &[(String, f64, Option<f64>)],
    holding_amount: &BigDecimal,
) -> TradingAction {
    // 空の場合はHold
    if ranked_tokens.is_empty() {
        return TradingAction::Hold;
    }

    let best_token = &ranked_tokens[0];

    // 現在のトークンが最良の場合はHold
    if best_token.0 == current_token {
        return TradingAction::Hold;
    }

    // 保有額が最小取引額以下の場合はHold
    let amount = holding_amount.to_string().parse::<f64>().unwrap_or(0.0);
    if amount < MIN_TRADE_AMOUNT {
        return TradingAction::Hold;
    }

    // 現在のトークンの期待リターンが閾値以下
    if current_return < MIN_PROFIT_THRESHOLD {
        return TradingAction::Sell {
            token: current_token.to_string(),
            target: best_token.0.clone(),
        };
    }

    // より良いトークンが存在する場合（信頼度も考慮）
    let confidence_factor = best_token.2.unwrap_or(0.5);
    if best_token.1 > current_return * SWITCH_MULTIPLIER * confidence_factor {
        return TradingAction::Switch {
            from: current_token.to_string(),
            to: best_token.0.clone(),
        };
    }

    TradingAction::Hold
}

// ==================== 実行フロー ====================

/// モメンタム戦略の実行（改善版）
#[allow(dead_code)]
pub async fn execute_momentum_strategy(
    current_holdings: Vec<TokenHolding>,
    predictions: Vec<PredictionData>,
) -> Result<ExecutionReport> {
    // トークンをランキング
    let ranked = rank_tokens_by_momentum(predictions.clone());

    // 予測データをHashMapに変換（高速検索用）
    let prediction_map: HashMap<String, &PredictionData> =
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

        let action =
            make_trading_decision(&holding.token, current_return, &ranked, &holding.amount);

        if action != TradingAction::Hold {
            actions.push(action);
        }
    }

    let mut report = ExecutionReport::new(actions);

    // 期待リターンを計算（最良トークンのリターン）
    if !ranked.is_empty() {
        report.expected_return = Some(ranked[0].1);
    }

    Ok(report)
}

/// PredictionServiceを使用した戦略実行
#[allow(dead_code)]
pub async fn execute_with_prediction_service(
    prediction_service: &PredictionService,
    current_holdings: Vec<TokenHolding>,
    quote_token: &str,
    history_days: i64,
) -> Result<ExecutionReport> {
    // 保有トークンの予測を取得
    let tokens: Vec<String> = current_holdings.iter().map(|h| h.token.clone()).collect();

    let predictions_map = prediction_service
        .predict_multiple_tokens(tokens.clone(), quote_token, history_days, 24)
        .await?;

    // PredictionDataに変換
    let mut prediction_data = Vec::new();
    for holding in &current_holdings {
        if let Some(prediction) = predictions_map.get(&holding.token)
            && let Some(data) =
                PredictionData::from_token_prediction(prediction, holding.current_price.clone())
        {
            prediction_data.push(data);
        }
    }

    // 追加: トップトークンの予測も取得
    let end_date = Utc::now();
    let start_date = end_date - Duration::days(history_days);
    let top_tokens = prediction_service
        .get_top_tokens(start_date, end_date, TOP_N_TOKENS, quote_token)
        .await?;

    // トップトークンの予測を追加
    for top_token in top_tokens {
        // 既に予測済みのトークンはスキップ
        if tokens.contains(&top_token.token) {
            continue;
        }

        let history = prediction_service
            .get_price_history(&top_token.token, quote_token, start_date, end_date)
            .await?;

        let prediction = prediction_service.predict_price(&history, 24).await?;

        if let Some(data) = PredictionData::from_token_prediction(
            &prediction,
            BigDecimal::from(top_token.current_price as i64),
        ) {
            prediction_data.push(data);
        }
    }

    // 戦略を実行
    execute_momentum_strategy(current_holdings, prediction_data).await
}

// ==================== 改善機能 ====================

/// 取引コストを考慮した期待リターンの調整
#[allow(dead_code)]
pub fn adjust_for_trading_costs(expected_return: f64) -> f64 {
    // 往復の手数料とスリッページを考慮
    expected_return - (2.0 * TRADING_FEE) - MAX_SLIPPAGE
}

/// 信頼度と期待リターンに基づくポジションサイズの計算
#[allow(dead_code)]
pub fn calculate_position_size(confidence_score: f64, expected_return: f64) -> f64 {
    // 信頼度と期待リターンに基づくポジションサイズ
    (confidence_score * expected_return).clamp(0.0, 1.0)
}

/// ボラティリティによるフィルタリング
#[allow(dead_code)]
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
#[allow(dead_code)]
fn calculate_volatility(prices: &[f64]) -> f64 {
    if prices.len() < 2 {
        return 0.0;
    }

    // リターンを計算
    let mut returns = Vec::new();
    for i in 1..prices.len() {
        if prices[i - 1] != 0.0 {
            let r = (prices[i] - prices[i - 1]) / prices[i - 1];
            returns.push(r);
        }
    }

    if returns.is_empty() {
        return 0.0;
    }

    // 平均リターン
    let mean: f64 = returns.iter().sum::<f64>() / returns.len() as f64;

    // 標準偏差
    let variance: f64 =
        returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;

    variance.sqrt()
}

// ==================== バックテスト ====================

/// バックテストメトリクス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestMetrics {
    pub total_return: f64,
    pub max_drawdown: f64,
    pub sharpe_ratio: f64,
    pub win_rate: f64,
    pub avg_holding_period_hours: f64,
}

/// バックテストの実行
#[allow(dead_code)]
pub async fn run_backtest(
    _historical_data: Vec<TokenRate>,
    _initial_capital: BigDecimal,
) -> Result<BacktestMetrics> {
    // TODO: バックテストロジックの実装
    Ok(BacktestMetrics {
        total_return: 0.0,
        max_drawdown: 0.0,
        sharpe_ratio: 0.0,
        win_rate: 0.0,
        avg_holding_period_hours: 24.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bigdecimal::FromPrimitive;

    #[test]
    fn test_calculate_expected_return() {
        let prediction = PredictionData {
            token: "TEST".to_string(),
            current_price: BigDecimal::from_f64(100.0).unwrap(),
            predicted_price_24h: BigDecimal::from_f64(110.0).unwrap(),
            timestamp: Utc::now(),
            confidence: Some(0.8),
        };

        let return_val = calculate_expected_return(&prediction);
        assert!((return_val - 0.1).abs() < 0.001);
    }

    #[test]
    fn test_rank_tokens_by_momentum() {
        let predictions = vec![
            PredictionData {
                token: "TOKEN1".to_string(),
                current_price: BigDecimal::from_f64(100.0).unwrap(),
                predicted_price_24h: BigDecimal::from_f64(105.0).unwrap(),
                timestamp: Utc::now(),
                confidence: Some(0.7),
            },
            PredictionData {
                token: "TOKEN2".to_string(),
                current_price: BigDecimal::from_f64(100.0).unwrap(),
                predicted_price_24h: BigDecimal::from_f64(115.0).unwrap(),
                timestamp: Utc::now(),
                confidence: Some(0.9),
            },
            PredictionData {
                token: "TOKEN3".to_string(),
                current_price: BigDecimal::from_f64(100.0).unwrap(),
                predicted_price_24h: BigDecimal::from_f64(102.0).unwrap(),
                timestamp: Utc::now(),
                confidence: Some(0.6),
            },
        ];

        let ranked = rank_tokens_by_momentum(predictions);

        assert_eq!(ranked[0].0, "TOKEN2");
        assert_eq!(ranked[1].0, "TOKEN1");
        assert_eq!(ranked[2].0, "TOKEN3");
    }

    #[test]
    fn test_make_trading_decision() {
        let ranked = vec![
            ("BEST_TOKEN".to_string(), 0.2, Some(0.8)),
            ("GOOD_TOKEN".to_string(), 0.1, Some(0.7)),
            ("OK_TOKEN".to_string(), 0.03, Some(0.6)),
        ];

        let amount = BigDecimal::from_f64(10.0).unwrap();

        // Case 1: Hold when current token is best
        let action = make_trading_decision("BEST_TOKEN", 0.2, &ranked, &amount);
        assert_eq!(action, TradingAction::Hold);

        // Case 2: Sell when return is below threshold
        let action = make_trading_decision("BAD_TOKEN", 0.02, &ranked, &amount);
        assert!(matches!(action, TradingAction::Sell { .. }));

        // Case 3: Switch when better option exists
        let action = make_trading_decision("OK_TOKEN", 0.03, &ranked, &amount);
        assert!(matches!(action, TradingAction::Switch { .. }));

        // Case 4: Hold when amount is too small
        let small_amount = BigDecimal::from_f64(0.5).unwrap();
        let action = make_trading_decision("BAD_TOKEN", 0.02, &ranked, &small_amount);
        assert_eq!(action, TradingAction::Hold);
    }

    #[test]
    fn test_calculate_volatility() {
        let prices = vec![100.0, 105.0, 103.0, 108.0, 106.0];
        let volatility = calculate_volatility(&prices);
        assert!(volatility > 0.0 && volatility < 0.05); // 低ボラティリティ

        let high_vol_prices = vec![100.0, 120.0, 90.0, 130.0, 85.0];
        let high_volatility = calculate_volatility(&high_vol_prices);
        assert!(high_volatility > 0.1); // 高ボラティリティ
    }
}
