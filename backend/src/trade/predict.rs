use crate::logging::*;
use crate::persistence::TimeRange;
use crate::persistence::token_rate::TokenRate;
use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount};
use anyhow::{Context, Result};
use async_trait::async_trait;
use bigdecimal::BigDecimal;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zaciraci_common::algorithm::prediction::{
    PredictedPrice as CommonPredictedPrice, PredictionProvider, PriceHistory as CommonPriceHistory,
    TokenPredictionResult, TopTokenInfo,
};
use zaciraci_common::api::chronos::ChronosApiClient;
use zaciraci_common::api::traits::PredictionClient;
use zaciraci_common::prediction::{PredictionResult, ZeroShotPredictionRequest};
use zaciraci_common::types::{
    TokenInAccount as CommonTokenInAccount, TokenOutAccount as CommonTokenOutAccount, TokenPrice,
};

/// トークンの価格履歴
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPriceHistory {
    pub token: TokenOutAccount,
    pub quote_token: TokenInAccount,
    pub prices: Vec<PricePoint>,
}

// 共通クレートのPricePointを使用
pub use zaciraci_common::algorithm::types::PricePoint;

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
/// Chronos API から返される予測値は price 形式（NEAR/token）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictedPrice {
    pub timestamp: DateTime<Utc>,
    /// 予測価格（NEAR/token）
    pub price: TokenPrice,
    pub confidence: Option<BigDecimal>,
}

/// 価格予測サービス
pub struct PredictionService {
    chronos_client: ChronosApiClient,
    pub(crate) max_retries: u32,
    pub(crate) retry_delay_seconds: u64,
}

impl PredictionService {
    pub fn new(chronos_url: String) -> Self {
        let config = zaciraci_common::config::config();
        Self {
            chronos_client: ChronosApiClient::new(chronos_url),
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

        let rates = TokenRate::get_rates_in_time_range(&range, &base_token, &quote_token_account)
            .await
            .context("Failed to get price history from database")?;

        // TokenRateをPricePointに変換（ExchangeRate から正しく TokenPrice に変換）
        let price_points: Vec<PricePoint> = rates
            .into_iter()
            .map(|rate| PricePoint {
                timestamp: DateTime::from_naive_utc_and_offset(rate.timestamp, Utc),
                price: rate.exchange_rate.to_price(),
                volume: None, // ボリュームデータは現在利用不可
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

        // 履歴データを予測用フォーマットに変換
        // BigDecimalを直接使用（ChronosAPIはJSON経由で数値を受け取るため）
        let values: Vec<BigDecimal> = history
            .prices
            .iter()
            .map(|p| p.price.as_bigdecimal().clone())
            .collect();
        let timestamps: Vec<DateTime<Utc>> = history.prices.iter().map(|p| p.timestamp).collect();

        if values.is_empty() {
            return Err(anyhow::anyhow!("No price history available for prediction"));
        }

        let last_timestamp = timestamps.last().expect("checked non-empty above");
        let forecast_until = *last_timestamp + Duration::hours(prediction_horizon as i64);

        // 予測リクエストを作成
        let request = ZeroShotPredictionRequest {
            timestamp: timestamps,
            values,
            forecast_until,
            model_name: Some("chronos_default".to_string()),
            model_params: None,
        };

        // 非同期予測を開始
        let async_response = self
            .chronos_client
            .predict(request)
            .await
            .context("Failed to start prediction")?;

        info!(log, "Prediction started";
            "task_id" => %async_response.task_id
        );

        // 予測完了まで待機
        let result = self
            .chronos_client
            .poll_prediction_until_complete(&async_response.task_id)
            .await
            .context("Failed to get prediction result")?;

        // 予測結果を変換
        let predictions = self.convert_prediction_result(
            &result,
            &history
                .prices
                .last()
                .expect("checked non-empty above")
                .timestamp,
            prediction_horizon,
        )?;

        Ok(TokenPrediction {
            token: history.token.clone(),
            quote_token: history.quote_token.clone(),
            prediction_time: Utc::now(),
            predictions,
        })
    }

    /// 複数トークンの価格予測を実行（10個ずつのバッチで処理）
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
        let batch_size = 10;

        let mut all_predictions = HashMap::new();

        // トークンを10個ずつのバッチに分割して処理
        for (batch_index, batch) in tokens.chunks(batch_size).enumerate() {
            debug!(log, "Processing batch";
                "batch_index" => batch_index,
                "batch_size" => batch.len()
            );

            // バッチ内の各トークンを順次処理
            // 注: バッチ間では並列化せず、バッチ内のトークンも順次処理する
            // これによりChronosサービスへの同時リクエスト数を制限
            for (token_index, token) in batch.iter().enumerate() {
                trace!(log, "Processing token";
                    "batch_index" => batch_index,
                    "token_index" => token_index,
                    "token" => %token
                );

                // 価格履歴を取得（リトライあり）
                let history = match self
                    .get_price_history_with_retry(token, quote_token, start_date, end_date, &log)
                    .await
                {
                    Ok(h) => h,
                    Err(e) => {
                        warn!(log, "Failed to get price history after retries, skipping token";
                            "token" => %token,
                            "error" => %e
                        );
                        continue;
                    }
                };

                // 価格予測を実行（リトライあり）
                match self
                    .predict_price_with_retry(&history, prediction_horizon, &log)
                    .await
                {
                    Ok(prediction) => {
                        all_predictions.insert(token.clone(), prediction);
                        trace!(log, "Successfully predicted price";
                            "token" => %token
                        );
                    }
                    Err(e) => {
                        warn!(log, "Failed to predict price after retries, skipping token";
                            "token" => %token,
                            "error" => %e
                        );
                    }
                }
            }
        }

        // 全てのトークンが失敗した場合はエラーを返す
        if all_predictions.is_empty() {
            return Err(anyhow::anyhow!(
                "Failed to predict any tokens. All {} tokens failed.",
                tokens.len()
            ));
        }

        info!(log, "Successfully predicted prices";
            "successful" => all_predictions.len(),
            "total" => tokens.len()
        );

        Ok(all_predictions)
    }

    /// 予測結果を変換
    fn convert_prediction_result(
        &self,
        result: &PredictionResult,
        last_timestamp: &DateTime<Utc>,
        horizon: usize,
    ) -> Result<Vec<PredictedPrice>> {
        let chronos_response = result.result.as_ref().context("No prediction result")?;

        let predicted_prices: Vec<PredictedPrice> = chronos_response
            .forecast_values
            .iter()
            .take(horizon)
            .enumerate()
            .map(|(i, price_value)| {
                let timestamp = *last_timestamp + Duration::hours((i + 1) as i64);
                PredictedPrice {
                    timestamp,
                    // forecast_values は price 形式（NEAR/token）
                    price: TokenPrice::from_near_per_token(price_value.clone()),
                    confidence: None, // 信頼度は将来実装
                }
            })
            .collect();

        Ok(predicted_prices)
    }

    /// 価格履歴を取得（リトライ付き）
    async fn get_price_history_with_retry(
        &self,
        token: &TokenOutAccount,
        quote_token: &TokenInAccount,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
        log: &slog::Logger,
    ) -> Result<TokenPriceHistory> {
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                trace!(log, "Retrying get_price_history";
                    "token" => %token,
                    "attempt" => attempt,
                    "max_retries" => self.max_retries
                );
                tokio::time::sleep(tokio::time::Duration::from_secs(self.retry_delay_seconds))
                    .await;
            }

            match self
                .get_price_history(token, quote_token, start_date, end_date)
                .await
            {
                Ok(history) => return Ok(history),
                Err(e) => {
                    warn!(log, "Failed to get price history";
                        "token" => %token,
                        "attempt" => attempt,
                        "error" => %e
                    );
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.expect("loop executed at least once"))
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
        quote_token: &CommonTokenInAccount,
    ) -> Result<Vec<TopTokenInfo>> {
        self.get_tokens_by_volatility(start_date, end_date, quote_token)
            .await
    }

    async fn get_price_history(
        &self,
        token: &CommonTokenOutAccount,
        quote_token: &CommonTokenInAccount,
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
        tokens: Vec<CommonTokenOutAccount>,
        quote_token: &CommonTokenInAccount,
        history_days: i64,
        prediction_horizon: usize,
    ) -> Result<HashMap<CommonTokenOutAccount, TokenPredictionResult>> {
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
