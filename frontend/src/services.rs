use chrono::{DateTime, Utc};
use std::sync::Arc;
use zaciraci_common::{
    ApiResponse, pools::VolatilityTokensRequest, stats::GetValuesRequest, types::TokenAccount,
};

use crate::chronos_api::predict::ChronosApiClient;
use crate::errors::PredictionError;
use crate::prediction_config::get_config;
use crate::prediction_utils::execute_zero_shot_prediction;
use crate::server_api::ApiClient;

/// ボラティリティトークン予測のビジネスロジックを担当するサービス
pub struct VolatilityPredictionService {
    api_client: Arc<ApiClient>,
    chronos_client: Arc<ChronosApiClient>,
}

/// ボラティリティトークンの取得結果
#[derive(Debug, Clone)]
pub struct VolatilityTokenResult {
    pub tokens: Vec<TokenAccount>,
}

/// 予測結果を表現する構造体
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct VolatilityPredictionResult {
    #[allow(dead_code)]
    pub token: String,
    pub predicted_price: f64,
    pub accuracy: f64,
    pub chart_svg: Option<String>,
}

impl VolatilityPredictionService {
    /// 新しいサービスインスタンスを作成
    pub fn new(api_client: Arc<ApiClient>, chronos_client: Arc<ChronosApiClient>) -> Self {
        Self {
            api_client,
            chronos_client,
        }
    }

    /// ボラティリティトークンを取得
    pub async fn get_volatility_tokens(
        &self,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
        limit: u32,
    ) -> Result<VolatilityTokenResult, PredictionError> {
        let _config = get_config();
        let volatility_request = VolatilityTokensRequest {
            start: start_date.naive_utc(),
            end: end_date.naive_utc(),
            limit,
        };

        match self
            .api_client
            .pools
            .get_volatility_tokens(volatility_request)
            .await
        {
            Ok(ApiResponse::Success(response)) => Ok(VolatilityTokenResult {
                tokens: response.tokens,
            }),
            Ok(ApiResponse::Error(error_msg)) => Err(PredictionError::ApiError(error_msg)),
            Err(_) => Err(PredictionError::VolatilityTokensNotFound),
        }
    }

    /// 単一のトークンに対して予測を実行
    pub async fn predict_token(
        &self,
        token: &TokenAccount,
        start_datetime: DateTime<Utc>,
        end_datetime: DateTime<Utc>,
        quote_token: String,
    ) -> Result<VolatilityPredictionResult, PredictionError> {
        let _config = get_config();

        // StringをTokenAccountに変換
        let quote_token_account: TokenAccount = quote_token
            .parse()
            .map_err(|_| PredictionError::ApiError("Invalid quote token".to_string()))?;

        // データ取得リクエスト作成
        let values_request = GetValuesRequest {
            quote_token: quote_token_account.clone(),
            base_token: token.clone(),
            start: start_datetime.naive_utc(),
            end: end_datetime.naive_utc(),
        };

        // 価格データ取得
        match self.api_client.stats.get_values(&values_request).await {
            Ok(ApiResponse::Success(values_response)) => {
                if values_response.values.is_empty() {
                    return Err(PredictionError::InsufficientData);
                }

                // ゼロショット予測実行
                let values_data = values_response.values;

                match execute_zero_shot_prediction(
                    quote_token_account, // quote_token (TokenAccount)
                    token.clone(),       // base_token (TokenAccount)
                    values_data.clone(),
                    "chronos-t5-small".to_string(), // model_name
                    self.chronos_client.clone(),
                )
                .await
                {
                    Ok(result) => Ok(VolatilityPredictionResult {
                        token: token.to_string(),
                        predicted_price: result.predicted_price,
                        accuracy: result.accuracy,
                        chart_svg: result.chart_svg,
                    }),
                    Err(_) => {
                        // 予測失敗時はフォールバック値を使用
                        let fallback_price = values_data
                            .last()
                            .map(|v| v.value * _config.fallback_multiplier)
                            .unwrap_or(0.0);

                        Ok(VolatilityPredictionResult {
                            token: token.to_string(),
                            predicted_price: fallback_price,
                            accuracy: 0.0,
                            chart_svg: None,
                        })
                    }
                }
            }
            Ok(ApiResponse::Error(error_msg)) => Err(PredictionError::ApiError(error_msg)),
            Err(_) => Err(PredictionError::NetworkError),
        }
    }

    /// 複数のトークンに対して並列で予測を実行
    #[allow(dead_code)]
    pub async fn predict_tokens(
        &self,
        tokens: Vec<TokenAccount>,
        start_datetime: DateTime<Utc>,
        end_datetime: DateTime<Utc>,
        quote_token: String,
    ) -> Vec<Result<VolatilityPredictionResult, PredictionError>> {
        use futures::future::join_all;

        let futures = tokens.into_iter().map(|token| {
            let quote_token_clone = quote_token.clone();
            let token_clone = token.clone();
            async move {
                self.predict_token(
                    &token_clone,
                    start_datetime,
                    end_datetime,
                    quote_token_clone,
                )
                .await
            }
        });

        join_all(futures).await
    }
}
