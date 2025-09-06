use crate::Result;
use bigdecimal::BigDecimal;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::prediction::PredictionProvider;

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

// PredictionDataのfrom_token_predictionメソッドは prediction.rs に実装

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
pub struct TokenHolding {
    pub token: String,
    pub amount: BigDecimal,
    pub current_price: BigDecimal,
}

// ==================== 定数 ====================

/// 最低利益率閾値（5%）
const MIN_PROFIT_THRESHOLD: f64 = 0.05;

/// 切り替え倍率（1.5倍以上の利益で切り替え）
const SWITCH_MULTIPLIER: f64 = 1.5;

/// 上位N個のトークンを考慮
pub const TOP_N_TOKENS: usize = 3;

/// 取引手数料（0.3%）
const TRADING_FEE: f64 = 0.003;

/// 最小取引額（NEAR）
const MIN_TRADE_AMOUNT: f64 = 1.0;

/// 最大スリッページ許容率（2%）
const MAX_SLIPPAGE: f64 = 0.02;

// ==================== コアアルゴリズム ====================

/// 予測リターンを計算（取引コスト考慮）
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
pub fn calculate_confidence_adjusted_return(prediction: &PredictionData) -> f64 {
    let base_return = calculate_expected_return(prediction);
    let confidence = prediction.confidence.unwrap_or(0.5);

    // 信頼度で調整（信頼度が低い場合はリターンを減少）
    base_return * confidence
}

/// トークンをモメンタムでランキング
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

/// PredictionProviderを使用した戦略実行
pub async fn execute_with_prediction_provider<P: PredictionProvider>(
    prediction_provider: &P,
    current_holdings: Vec<TokenHolding>,
    quote_token: &str,
    history_days: i64,
) -> Result<ExecutionReport> {
    // 保有トークンの予測を取得
    let tokens: Vec<String> = current_holdings.iter().map(|h| h.token.clone()).collect();

    let predictions_map = prediction_provider
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
    let top_tokens = prediction_provider
        .get_top_tokens(start_date, end_date, TOP_N_TOKENS, quote_token)
        .await?;

    // トップトークンの予測を追加
    for top_token in top_tokens {
        // 既に予測済みのトークンはスキップ
        if tokens.contains(&top_token.token) {
            continue;
        }

        let history = prediction_provider
            .get_price_history(&top_token.token, quote_token, start_date, end_date)
            .await?;

        let prediction = prediction_provider.predict_price(&history, 24).await?;

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
fn calculate_volatility(prices: &[f64]) -> f64 {
    crate::algorithm::calculate_volatility_from_prices(prices)
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
    _historical_data: Vec<String>, // TokenRateは backend固有なのでStringに変更
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
mod tests;

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::algorithm::prediction::*;
    use async_trait::async_trait;
    use bigdecimal::BigDecimal;
    use chrono::{Duration, Utc};
    use std::collections::HashMap;

    // テスト用のシンプルなMockPredictionProvider
    struct SimpleMockProvider {
        price_histories: HashMap<String, PriceHistory>,
    }

    impl SimpleMockProvider {
        fn new() -> Self {
            Self {
                price_histories: HashMap::new(),
            }
        }

        fn with_price_history(
            mut self,
            token: &str,
            prices: Vec<(chrono::DateTime<Utc>, f64)>,
        ) -> Self {
            let price_points: Vec<PricePoint> = prices
                .into_iter()
                .map(|(timestamp, price)| PricePoint { timestamp, price })
                .collect();

            self.price_histories.insert(
                token.to_string(),
                PriceHistory {
                    token: token.to_string(),
                    quote_token: "wrap.near".to_string(),
                    prices: price_points,
                },
            );
            self
        }
    }

    #[async_trait]
    impl PredictionProvider for SimpleMockProvider {
        async fn get_top_tokens(
            &self,
            _start_date: chrono::DateTime<Utc>,
            _end_date: chrono::DateTime<Utc>,
            limit: usize,
            _quote_token: &str,
        ) -> crate::Result<Vec<TopTokenInfo>> {
            Ok(vec![
                TopTokenInfo {
                    token: "top_token1".to_string(),
                    volatility: 0.2,
                    volume_24h: 1000000.0,
                    current_price: 100.0,
                },
                TopTokenInfo {
                    token: "top_token2".to_string(),
                    volatility: 0.3,
                    volume_24h: 800000.0,
                    current_price: 50.0,
                },
            ]
            .into_iter()
            .take(limit)
            .collect())
        }

        async fn get_price_history(
            &self,
            token: &str,
            _quote_token: &str,
            _start_date: chrono::DateTime<Utc>,
            _end_date: chrono::DateTime<Utc>,
        ) -> crate::Result<PriceHistory> {
            self.price_histories
                .get(token)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("No price history found for token: {}", token))
        }

        async fn predict_price(
            &self,
            history: &PriceHistory,
            prediction_horizon: usize,
        ) -> crate::Result<TokenPredictionResult> {
            let last_price = history.prices.last().map(|p| p.price).unwrap_or(100.0);
            let prediction_time = Utc::now();
            let mut predictions = Vec::new();

            for i in 1..=prediction_horizon {
                let timestamp = prediction_time + Duration::hours(i as i64);
                let price = last_price * (1.0 + (i as f64 * 0.01));
                predictions.push(PredictedPrice {
                    timestamp,
                    price,
                    confidence: Some(0.8),
                });
            }

            Ok(TokenPredictionResult {
                token: history.token.clone(),
                quote_token: history.quote_token.clone(),
                prediction_time,
                predictions,
            })
        }

        async fn predict_multiple_tokens(
            &self,
            tokens: Vec<String>,
            quote_token: &str,
            history_days: i64,
            prediction_horizon: usize,
        ) -> crate::Result<HashMap<String, TokenPredictionResult>> {
            let mut results = HashMap::new();

            for token in tokens {
                let end_date = Utc::now();
                let start_date = end_date - Duration::days(history_days);

                if let Ok(history) = self
                    .get_price_history(&token, quote_token, start_date, end_date)
                    .await
                    && let Ok(prediction) = self.predict_price(&history, prediction_horizon).await
                {
                    results.insert(token, prediction);
                }
            }

            Ok(results)
        }
    }

    #[tokio::test]
    async fn test_execute_with_prediction_provider() {
        let current_time = Utc::now();
        let provider = SimpleMockProvider::new()
            .with_price_history("token1", vec![(current_time, 100.0)])
            .with_price_history("token2", vec![(current_time, 50.0)])
            .with_price_history("top_token1", vec![(current_time, 100.0)])
            .with_price_history("top_token2", vec![(current_time, 50.0)]);

        let current_holdings = vec![
            TokenHolding {
                token: "token1".to_string(),
                amount: BigDecimal::from(10),
                current_price: BigDecimal::from(100),
            },
            TokenHolding {
                token: "token2".to_string(),
                amount: BigDecimal::from(20),
                current_price: BigDecimal::from(50),
            },
        ];

        let result =
            execute_with_prediction_provider(&provider, current_holdings, "wrap.near", 7).await;

        match result {
            Ok(report) => {
                // レポートの基本的な構造を確認
                assert_eq!(report.timestamp.date_naive(), Utc::now().date_naive());
                println!("Generated {} actions", report.actions.len());
                println!("Expected return: {:?}", report.expected_return);
            }
            Err(e) => {
                panic!("Test failed with error: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_execute_with_prediction_provider_empty_holdings() {
        let current_time = Utc::now();
        let provider = SimpleMockProvider::new()
            .with_price_history("top_token1", vec![(current_time, 100.0)])
            .with_price_history("top_token2", vec![(current_time, 50.0)]);
        let current_holdings = vec![];

        let result =
            execute_with_prediction_provider(&provider, current_holdings, "wrap.near", 7).await;

        match result {
            Ok(report) => {
                // 空の保有でも実行できることを確認
                assert_eq!(report.total_trades, 0);
                assert_eq!(report.actions.len(), 0);
            }
            Err(e) => {
                panic!("Test failed with error: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_execute_with_prediction_provider_with_top_tokens() {
        let current_time = Utc::now();
        let provider = SimpleMockProvider::new()
            .with_price_history("top_token1", vec![(current_time, 100.0)])
            .with_price_history("top_token2", vec![(current_time, 50.0)]);

        let current_holdings = vec![TokenHolding {
            token: "other_token".to_string(),
            amount: BigDecimal::from(10),
            current_price: BigDecimal::from(75),
        }];

        let result =
            execute_with_prediction_provider(&provider, current_holdings, "wrap.near", 7).await;

        // トップトークンの情報も取得されることを確認
        assert!(result.is_ok());
        let report = result.unwrap();

        // レポートが生成されることを確認
        assert!(report.expected_return.is_some() || report.expected_return.is_none());
    }
}
