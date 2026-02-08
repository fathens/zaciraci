use anyhow::{Context, Result};
use async_trait::async_trait;
use bigdecimal::BigDecimal;
use chrono::{DateTime, Duration, TimeDelta, Utc};
use common::algorithm::prediction::{
    PredictedPrice as CommonPredictedPrice, PredictionProvider, PriceHistory as CommonPriceHistory,
    TokenPredictionResult, TopTokenInfo,
};
use common::api::chronos::ChronosPredictor;
use common::types::{TimeRange, TokenInAccount, TokenOutAccount, TokenPrice};
use futures::stream::{self, StreamExt};
use logging::*;
use persistence::token_rate::TokenRate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// トークンの価格履歴
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPriceHistory {
    pub token: TokenOutAccount,
    pub quote_token: TokenInAccount,
    pub prices: Vec<PricePoint>,
}

// 共通クレートのPricePointを使用
use common::algorithm::types::PricePoint;

/// 予測結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPrediction {
    pub token: TokenOutAccount,
    pub quote_token: TokenInAccount,
    pub prediction_time: DateTime<Utc>,
    pub predictions: Vec<PredictedPrice>,
}

/// 予測価格
///
/// Chronos ライブラリから返される予測値は price 形式（NEAR/token）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictedPrice {
    pub timestamp: DateTime<Utc>,
    /// 予測価格（NEAR/token）
    pub price: TokenPrice,
    pub confidence: Option<BigDecimal>,
}

/// 価格予測サービス
pub struct PredictionService {
    predictor: ChronosPredictor,
    pub(crate) max_retries: u32,
    pub(crate) retry_delay_seconds: u64,
}

impl Default for PredictionService {
    fn default() -> Self {
        Self::new()
    }
}

impl PredictionService {
    pub fn new() -> Self {
        let config = common::config::config();
        Self {
            predictor: ChronosPredictor::new(),
            max_retries: config.trade.prediction_max_retries,
            retry_delay_seconds: config.trade.prediction_retry_delay_seconds,
        }
    }

    /// ボラティリティ順に全トークンを取得
    pub async fn get_tokens_by_volatility(
        &self,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
        quote_token: &TokenInAccount,
    ) -> Result<Vec<TopTokenInfo>> {
        // 直接データベースからボラティリティ情報を取得
        let range = TimeRange {
            start: start_date.naive_utc(),
            end: end_date.naive_utc(),
        };

        let volatility_tokens = TokenRate::get_by_volatility_in_time_range(&range, quote_token)
            .await
            .context("Failed to get volatility tokens from database")?;

        // 全トークンをTopTokenInfo形式に変換（limit は呼び出し側で適用）
        let top_tokens = volatility_tokens
            .into_iter()
            .map(|vol_token| TopTokenInfo {
                token: vol_token.base.into(),
                volatility: vol_token.variance,
            })
            .collect();

        Ok(top_tokens)
    }

    /// 指定トークンの価格履歴を取得
    pub async fn get_price_history(
        &self,
        token: &TokenOutAccount,
        quote_token: &TokenInAccount,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<TokenPriceHistory> {
        // 直接データベースから価格履歴を取得
        let base_token: TokenOutAccount = token.clone();
        let quote_token_account: TokenInAccount = quote_token.clone();

        let range = TimeRange {
            start: start_date.naive_utc(),
            end: end_date.naive_utc(),
        };

        let get_decimals = super::make_get_decimals();
        let rates = TokenRate::get_rates_in_time_range(
            &range,
            &base_token,
            &quote_token_account,
            &get_decimals,
        )
        .await
        .context("Failed to get price history from database")?;

        // TokenRateをPricePointに変換（スポットレート補正適用）
        // swap_path が NULL のレコードには「自分より新しくもっとも古い」swap_path を使用
        // フォールバックインデックスを事前計算（O(n)）して O(n²) → O(n) に改善
        let fallback_indices = TokenRate::precompute_fallback_indices(&rates);
        let price_points: Vec<PricePoint> = rates
            .iter()
            .enumerate()
            .map(|(i, rate)| {
                let fallback_path = fallback_indices[i]
                    .and_then(|idx| rates.get(idx))
                    .and_then(|r| r.swap_path.as_ref());
                PricePoint {
                    timestamp: DateTime::from_naive_utc_and_offset(rate.timestamp, Utc),
                    price: rate.to_spot_rate_with_fallback(fallback_path).to_price(),
                    volume: None,
                }
            })
            .collect();

        Ok(TokenPriceHistory {
            token: token.clone(),
            quote_token: quote_token.clone(),
            prices: price_points,
        })
    }

    /// 価格予測を実行
    pub async fn predict_price(
        &self,
        history: &TokenPriceHistory,
        prediction_horizon: usize,
    ) -> Result<TokenPrediction> {
        let log = DEFAULT.new(o!("function" => "predict_price"));

        // 履歴データを予測用フォーマットに変換（1回の collect で BTreeMap 構築）
        let data: std::collections::BTreeMap<DateTime<Utc>, BigDecimal> = history
            .prices
            .iter()
            .map(|p| (p.timestamp, p.price.as_bigdecimal().clone()))
            .collect();

        if data.is_empty() {
            return Err(anyhow::anyhow!("No price history available for prediction"));
        }

        // 最後のデータタイムスタンプを保持（時間経過の基準点）
        let last_data_timestamp = *data.keys().last().expect("checked non-empty above");
        let forecast_until = last_data_timestamp + Duration::hours(prediction_horizon as i64);

        info!(log, "Starting prediction";
            "forecast_until" => %forecast_until
        );

        // ライブラリを直接呼び出し
        let chronos_response = self
            .predictor
            .predict_price(data, forecast_until)
            .await
            .context("Failed to execute prediction")?;

        debug!(log, "Prediction completed";
            "model" => &chronos_response.model_name,
            "strategy" => &chronos_response.strategy_name,
            "processing_time_secs" => chronos_response.processing_time_secs,
            "model_count" => chronos_response.model_count
        );

        // 予測結果を変換（最後のデータタイムスタンプを渡す）
        let predictions = self.convert_prediction_result(
            &chronos_response,
            prediction_horizon,
            last_data_timestamp,
        )?;

        Ok(TokenPrediction {
            token: history.token.clone(),
            quote_token: history.quote_token.clone(),
            prediction_time: Utc::now(),
            predictions,
        })
    }

    /// 複数トークンの価格予測を実行（バッチ履歴取得 + 予測の並行化）
    pub async fn predict_multiple_tokens(
        &self,
        tokens: Vec<TokenOutAccount>,
        quote_token: &TokenInAccount,
        history_days: i64,
        prediction_horizon: usize,
    ) -> Result<HashMap<TokenOutAccount, TokenPrediction>> {
        let log = DEFAULT.new(o!("function" => "predict_multiple_tokens"));

        let end_date = Utc::now();
        let start_date = end_date - Duration::days(history_days);
        let range = TimeRange {
            start: start_date.naive_utc(),
            end: end_date.naive_utc(),
        };

        // 1. 全トークンの履歴を一括取得（1回のDBクエリ）
        let token_strs: Vec<String> = tokens.iter().map(|t| t.to_string()).collect();
        let get_decimals = super::make_get_decimals();
        let histories_map = TokenRate::get_rates_for_multiple_tokens(
            &token_strs,
            quote_token,
            &range,
            &get_decimals,
        )
        .await
        .context("Failed to batch fetch price histories")?;

        info!(log, "Fetched price histories";
            "requested" => tokens.len(),
            "fetched" => histories_map.len()
        );

        // 2. 設定から並行実行数を取得
        let concurrency = common::config::config().trade.prediction_concurrency as usize;

        // 3. 予測を並行実行
        let results: Vec<_> = stream::iter(tokens.clone())
            .filter_map(|token| {
                let token_str = token.to_string();
                let rates = histories_map.get(&token_str).cloned();
                async move { rates.map(|r| (token, r)) }
            })
            .map(|(token, rates)| {
                let log = log.clone();
                let quote_token = quote_token.clone();
                async move {
                    // TokenPriceHistory を構築（スポットレート補正適用）
                    // swap_path が NULL のレコードには「自分より新しくもっとも古い」swap_path を使用
                    // フォールバックインデックスを事前計算（O(n)）して O(n²) → O(n) に改善
                    let fallback_indices = TokenRate::precompute_fallback_indices(&rates);
                    let history = TokenPriceHistory {
                        token: token.clone(),
                        quote_token: quote_token.clone(),
                        prices: rates
                            .iter()
                            .enumerate()
                            .map(|(i, r)| {
                                let fallback_path = fallback_indices[i]
                                    .and_then(|idx| rates.get(idx))
                                    .and_then(|rate| rate.swap_path.as_ref());
                                PricePoint {
                                    timestamp: DateTime::from_naive_utc_and_offset(
                                        r.timestamp,
                                        Utc,
                                    ),
                                    price: r.to_spot_rate_with_fallback(fallback_path).to_price(),
                                    volume: None,
                                }
                            })
                            .collect(),
                    };

                    // 予測実行（リトライあり）
                    match self
                        .predict_price_with_retry(&history, prediction_horizon, &log)
                        .await
                    {
                        Ok(prediction) => (token, Some(prediction)),
                        Err(e) => {
                            warn!(log, "Failed to predict"; "token" => %token, "error" => %e);
                            (token, None)
                        }
                    }
                }
            })
            .buffer_unordered(concurrency)
            .collect()
            .await;

        // 4. 結果を収集
        let all_predictions: HashMap<_, _> = results
            .into_iter()
            .filter_map(|(token, pred)| pred.map(|p| (token, p)))
            .collect();

        // 全てのトークンが失敗した場合はエラーを返す
        if all_predictions.is_empty() {
            return Err(anyhow::anyhow!(
                "Failed to predict any tokens. All {} tokens failed.",
                tokens.len()
            ));
        }

        info!(log, "Prediction completed";
            "successful" => all_predictions.len(),
            "total" => tokens.len()
        );

        Ok(all_predictions)
    }

    /// 予測結果を変換
    ///
    /// `last_data_timestamp`: 履歴データの最後のタイムスタンプ（時間経過の基準点）
    fn convert_prediction_result(
        &self,
        chronos_response: &common::prediction::ChronosPredictionResponse,
        horizon: usize,
        last_data_timestamp: DateTime<Utc>,
    ) -> Result<Vec<PredictedPrice>> {
        let predicted_prices: Vec<PredictedPrice> = chronos_response
            .forecast
            .iter()
            .take(horizon)
            .map(|(forecast_ts, price_value)| {
                // 予測までの時間経過を計算
                let time_ahead = forecast_ts.signed_duration_since(last_data_timestamp);

                // 信頼区間から confidence を計算（時間経過を考慮）
                let lower = chronos_response
                    .lower_bound
                    .as_ref()
                    .and_then(|lb| lb.get(forecast_ts));
                let upper = chronos_response
                    .upper_bound
                    .as_ref()
                    .and_then(|ub| ub.get(forecast_ts));
                let confidence =
                    Self::calculate_confidence_from_interval(price_value, lower, upper, time_ahead);

                PredictedPrice {
                    timestamp: *forecast_ts,
                    // forecast は price 形式（NEAR/token）
                    price: TokenPrice::from_near_per_token(price_value.clone()),
                    confidence,
                }
            })
            .collect();

        Ok(predicted_prices)
    }

    /// 信頼区間から confidence を計算（時間経過で正規化）
    ///
    /// 信頼区間の幅は時間経過の平方根に比例してスケールされる。
    /// この関数は相対幅から変動係数（CV）を逆算し、CVからconfidenceを計算する。
    ///
    /// 統計的根拠:
    /// - 信頼区間幅 = 2.56 × σ × sqrt(時間経過)  （80%信頼区間）
    /// - 相対幅 = 2.56 × CV × sqrt(時間経過)
    /// - CV = 相対幅 / (2.56 × sqrt(時間経過))
    ///
    /// CVからconfidenceへの変換:
    /// - CV ≤ 3% → confidence = 1.0 (非常に安定)
    /// - CV ≥ 15% → confidence = 0.0 (非常に不安定)
    fn calculate_confidence_from_interval(
        forecast: &BigDecimal,
        lower: Option<&BigDecimal>,
        upper: Option<&BigDecimal>,
        time_ahead: TimeDelta,
    ) -> Option<BigDecimal> {
        use bigdecimal::ToPrimitive;

        let (lower, upper) = match (lower, upper) {
            (Some(l), Some(u)) => (l, u),
            _ => return None,
        };

        let forecast_f64 = forecast.to_f64()?;
        if forecast_f64 <= 0.0 {
            return None;
        }

        let lower_f64 = lower.to_f64()?;
        let upper_f64 = upper.to_f64()?;
        let interval_width = upper_f64 - lower_f64;

        // 予測値に対する相対的な幅を計算
        let relative_width = interval_width / forecast_f64;

        // 時間経過から CV を逆算（1時間を基準単位とする）
        let hours = time_ahead.num_minutes() as f64 / 60.0;
        let time_factor = hours.max(1.0).sqrt();
        // 80%信頼区間の場合: 2.56 = 2 × 1.28
        let cv = relative_width / (2.56 * time_factor);

        // CV から confidence を計算
        // CV ≤ 3% → confidence = 1.0 (非常に安定)
        // CV ≥ 15% → confidence = 0.0 (非常に不安定)
        const MIN_CV: f64 = 0.03;
        const MAX_CV: f64 = 0.15;
        let confidence = 1.0 - ((cv - MIN_CV) / (MAX_CV - MIN_CV)).clamp(0.0, 1.0);

        Some(BigDecimal::try_from(confidence).unwrap_or_else(|_| BigDecimal::from(0)))
    }

    /// 価格予測を実行（リトライ付き）
    async fn predict_price_with_retry(
        &self,
        history: &TokenPriceHistory,
        prediction_horizon: usize,
        log: &slog::Logger,
    ) -> Result<TokenPrediction> {
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                trace!(log, "Retrying predict_price";
                    "token" => %history.token,
                    "attempt" => attempt,
                    "max_retries" => self.max_retries
                );
                tokio::time::sleep(tokio::time::Duration::from_secs(self.retry_delay_seconds))
                    .await;
            }

            match self.predict_price(history, prediction_horizon).await {
                Ok(prediction) => return Ok(prediction),
                Err(e) => {
                    warn!(log, "Failed to predict price";
                        "token" => %history.token,
                        "attempt" => attempt,
                        "error" => %e
                    );
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.expect("loop executed at least once"))
    }
}

// PredictionProviderトレイトの実装
#[async_trait]
impl PredictionProvider for PredictionService {
    async fn get_tokens_by_volatility(
        &self,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
        quote_token: &TokenInAccount,
    ) -> Result<Vec<TopTokenInfo>> {
        self.get_tokens_by_volatility(start_date, end_date, quote_token)
            .await
    }

    async fn get_price_history(
        &self,
        token: &TokenOutAccount,
        quote_token: &TokenInAccount,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<CommonPriceHistory> {
        // common と backend の TokenAccount は同一型なので直接使用可能
        let history = self
            .get_price_history(token, quote_token, start_date, end_date)
            .await?;
        Ok(CommonPriceHistory {
            token: token.clone(),
            quote_token: quote_token.clone(),
            prices: history.prices,
        })
    }

    async fn predict_price(
        &self,
        history: &CommonPriceHistory,
        prediction_horizon: usize,
    ) -> Result<TokenPredictionResult> {
        // common と backend の TokenAccount は同一型なので直接使用可能
        let backend_history = TokenPriceHistory {
            token: history.token.clone(),
            quote_token: history.quote_token.clone(),
            prices: history.prices.clone(),
        };

        let prediction = self
            .predict_price(&backend_history, prediction_horizon)
            .await?;

        Ok(TokenPredictionResult {
            token: history.token.clone(),
            quote_token: history.quote_token.clone(),
            prediction_time: prediction.prediction_time,
            predictions: prediction
                .predictions
                .into_iter()
                .map(|p| CommonPredictedPrice {
                    timestamp: p.timestamp,
                    price: p.price,
                    confidence: p.confidence.clone(),
                })
                .collect(),
        })
    }

    async fn predict_multiple_tokens(
        &self,
        tokens: Vec<TokenOutAccount>,
        quote_token: &TokenInAccount,
        history_days: i64,
        prediction_horizon: usize,
    ) -> Result<HashMap<TokenOutAccount, TokenPredictionResult>> {
        // common と backend の TokenAccount は同一型なので直接使用可能
        let predictions = self
            .predict_multiple_tokens(
                tokens.clone(),
                quote_token,
                history_days,
                prediction_horizon,
            )
            .await?;

        let mut result = HashMap::new();
        for (token_key, prediction) in predictions {
            result.insert(
                token_key.clone(),
                TokenPredictionResult {
                    token: token_key,
                    quote_token: quote_token.clone(),
                    prediction_time: prediction.prediction_time,
                    predictions: prediction
                        .predictions
                        .into_iter()
                        .map(|p| CommonPredictedPrice {
                            timestamp: p.timestamp,
                            price: p.price,
                            confidence: p.confidence.clone(),
                        })
                        .collect(),
                },
            );
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests;
