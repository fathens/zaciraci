use crate::logging::*;
use crate::persistence::TimeRange;
use crate::persistence::token_rate::TokenRate;
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
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
use zaciraci_common::types::TokenPrice;

/// トークンの価格履歴
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPriceHistory {
    pub token: String,
    pub quote_token: String,
    pub prices: Vec<PricePoint>,
}

// 共通クレートのPricePointを使用
pub use zaciraci_common::algorithm::types::PricePoint;

/// 予測結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPrediction {
    pub token: String,
    pub quote_token: String,
    pub prediction_time: DateTime<Utc>,
    pub predictions: Vec<PredictedPrice>,
}

/// 予測価格
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictedPrice {
    pub timestamp: DateTime<Utc>,
    /// 予測価格（無次元の価格比率）
    pub price: TokenPrice,
    pub confidence: Option<BigDecimal>,
}

/// トップトークン情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopToken {
    pub token: String,
    pub volatility: BigDecimal,
    pub volume_24h: BigDecimal,
    /// 現在価格（無次元の価格比率）
    pub current_price: TokenPrice,
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
        quote_token: &str,
    ) -> Result<Vec<TopToken>> {
        // 直接データベースからボラティリティ情報を取得
        let quote_token_account: TokenInAccount = quote_token
            .parse::<TokenAccount>()
            .map_err(|e| anyhow::anyhow!("Failed to parse quote token: {}", e))?
            .into();

        let range = TimeRange {
            start: start_date.naive_utc(),
            end: end_date.naive_utc(),
        };

        let volatility_tokens =
            TokenRate::get_by_volatility_in_time_range(&range, &quote_token_account)
                .await
                .context("Failed to get volatility tokens from database")?;

        // 全トークンをTopToken形式に変換（limit は呼び出し側で適用）
        let mut top_tokens = Vec::new();
        for vol_token in volatility_tokens.into_iter() {
            // 現在価格を取得
            let current_price = {
                let base_token = TokenOutAccount::from(vol_token.base.clone());
                let quote_token = quote_token_account.clone();

                match TokenRate::get_latest(&base_token, &quote_token).await {
                    Ok(Some(rate)) => TokenPrice::new(rate.rate),
                    Ok(None) => {
                        // ログを後で追加（slogのsetupが必要）
                        TokenPrice::new(BigDecimal::from(1)) // デフォルト値
                    }
                    Err(_e) => {
                        // ログを後で追加（slogのsetupが必要）
                        TokenPrice::new(BigDecimal::from(1)) // デフォルト値
                    }
                }
            };

            top_tokens.push(TopToken {
                token: vol_token.base.to_string(),
                volatility: vol_token.variance,
                volume_24h: BigDecimal::from(0), // ボリュームデータは現在利用不可
                current_price,
            });
        }

        Ok(top_tokens)
    }

    /// 指定トークンの価格履歴を取得
    pub async fn get_price_history(
        &self,
        token: &str,
        quote_token: &str,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<TokenPriceHistory> {
        // 直接データベースから価格履歴を取得
        let base_token: TokenOutAccount = token
            .parse::<TokenAccount>()
            .map_err(|e| anyhow::anyhow!("Failed to parse base token: {}", e))?
            .into();

        let quote_token_account: TokenInAccount = quote_token
            .parse::<TokenAccount>()
            .map_err(|e| anyhow::anyhow!("Failed to parse quote token: {}", e))?
            .into();

        let range = TimeRange {
            start: start_date.naive_utc(),
            end: end_date.naive_utc(),
        };

        let rates = TokenRate::get_rates_in_time_range(&range, &base_token, &quote_token_account)
            .await
            .context("Failed to get price history from database")?;

        // TokenRateをPricePointに変換
        let price_points: Vec<PricePoint> = rates
            .into_iter()
            .map(|rate| PricePoint {
                timestamp: DateTime::from_naive_utc_and_offset(rate.timestamp, Utc),
                price: TokenPrice::new(rate.rate),
                volume: None, // ボリュームデータは現在利用不可
            })
            .collect();

        Ok(TokenPriceHistory {
            token: token.to_string(),
            quote_token: quote_token.to_string(),
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

        let last_timestamp = timestamps.last().unwrap();
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
            &history.prices.last().unwrap().timestamp,
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
        tokens: Vec<String>,
        quote_token: &str,
        history_days: i64,
        prediction_horizon: usize,
    ) -> Result<HashMap<String, TokenPrediction>> {
        let log = DEFAULT.new(o!("function" => "predict_multiple_tokens"));

        let end_date = Utc::now();
        let start_date = end_date - Duration::days(history_days);
        let batch_size = 10;

        let mut all_predictions = HashMap::new();

        // トークンを10個ずつのバッチに分割して処理
        for (batch_index, batch) in tokens.chunks(batch_size).enumerate() {
            info!(log, "Processing batch";
                "batch_index" => batch_index,
                "batch_size" => batch.len()
            );

            // バッチ内の各トークンを順次処理
            // 注: バッチ間では並列化せず、バッチ内のトークンも順次処理する
            // これによりChronosサービスへの同時リクエスト数を制限
            for (token_index, token) in batch.iter().enumerate() {
                info!(log, "Processing token";
                    "batch_index" => batch_index,
                    "token_index" => token_index,
                    "token" => token
                );

                // 価格履歴を取得（リトライあり）
                let history = match self
                    .get_price_history_with_retry(token, quote_token, start_date, end_date, &log)
                    .await
                {
                    Ok(h) => h,
                    Err(e) => {
                        warn!(log, "Failed to get price history after retries, skipping token";
                            "token" => token,
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
                        info!(log, "Successfully predicted price";
                            "token" => token
                        );
                    }
                    Err(e) => {
                        warn!(log, "Failed to predict price after retries, skipping token";
                            "token" => token,
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
    #[allow(dead_code)]
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
            .map(|(i, price)| {
                let timestamp = *last_timestamp + Duration::hours((i + 1) as i64);
                PredictedPrice {
                    timestamp,
                    price: TokenPrice::new(price.clone()),
                    confidence: None, // 信頼度は将来実装
                }
            })
            .collect();

        Ok(predicted_prices)
    }

    /// 価格履歴を取得（リトライ付き）
    async fn get_price_history_with_retry(
        &self,
        token: &str,
        quote_token: &str,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
        log: &slog::Logger,
    ) -> Result<TokenPriceHistory> {
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                info!(log, "Retrying get_price_history";
                    "token" => token,
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
                        "token" => token,
                        "attempt" => attempt,
                        "error" => %e
                    );
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap())
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
                info!(log, "Retrying predict_price";
                    "token" => &history.token,
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
                        "token" => &history.token,
                        "attempt" => attempt,
                        "error" => %e
                    );
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap())
    }
}

// PredictionProviderトレイトの実装
#[async_trait]
impl PredictionProvider for PredictionService {
    async fn get_tokens_by_volatility(
        &self,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
        quote_token: &str,
    ) -> Result<Vec<TopTokenInfo>> {
        let tokens = self
            .get_tokens_by_volatility(start_date, end_date, quote_token)
            .await?;
        Ok(tokens
            .into_iter()
            .map(|t| TopTokenInfo {
                token: t.token,
                volatility: t.volatility.to_string().parse::<f64>().unwrap_or(0.0),
                volume_24h: t.volume_24h.to_string().parse::<f64>().unwrap_or(0.0),
                current_rate: t.current_price.to_price_f64(),
                // TODO: decimals を実際に取得する（現在はデフォルト 24）
                decimals: 24,
            })
            .collect())
    }

    async fn get_price_history(
        &self,
        token: &str,
        quote_token: &str,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<CommonPriceHistory> {
        let history = self
            .get_price_history(token, quote_token, start_date, end_date)
            .await?;
        Ok(CommonPriceHistory {
            token: history.token,
            quote_token: history.quote_token,
            prices: history.prices, // 型が統一されたので変換不要
        })
    }

    async fn predict_price(
        &self,
        history: &CommonPriceHistory,
        prediction_horizon: usize,
    ) -> Result<TokenPredictionResult> {
        // CommonPriceHistoryをTokenPriceHistoryに変換
        let backend_history = TokenPriceHistory {
            token: history.token.clone(),
            quote_token: history.quote_token.clone(),
            prices: history
                .prices
                .iter()
                .map(|p| PricePoint {
                    timestamp: p.timestamp,
                    price: p.price.clone(),
                    volume: p.volume.clone(),
                })
                .collect(),
        };

        let prediction = self
            .predict_price(&backend_history, prediction_horizon)
            .await?;

        Ok(TokenPredictionResult {
            token: prediction.token,
            quote_token: prediction.quote_token,
            prediction_time: prediction.prediction_time,
            predictions: prediction
                .predictions
                .into_iter()
                .map(|p| CommonPredictedPrice {
                    timestamp: p.timestamp,
                    price: p.price, // 既にPrice型
                    confidence: p.confidence.clone(),
                })
                .collect(),
        })
    }

    async fn predict_multiple_tokens(
        &self,
        tokens: Vec<String>,
        quote_token: &str,
        history_days: i64,
        prediction_horizon: usize,
    ) -> Result<HashMap<String, TokenPredictionResult>> {
        let predictions = self
            .predict_multiple_tokens(tokens, quote_token, history_days, prediction_horizon)
            .await?;

        let mut result = HashMap::new();
        for (token, prediction) in predictions {
            result.insert(
                token,
                TokenPredictionResult {
                    token: prediction.token,
                    quote_token: prediction.quote_token,
                    prediction_time: prediction.prediction_time,
                    predictions: prediction
                        .predictions
                        .into_iter()
                        .map(|p| CommonPredictedPrice {
                            timestamp: p.timestamp,
                            price: p.price, // 既にPrice型
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
#[path = "predict/tests.rs"]
mod tests;
