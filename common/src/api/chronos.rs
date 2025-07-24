use super::traits::{ApiClient, PredictionClient};
use super::{ApiClientConfig, ApiError};
use crate::{
    ApiResponse,
    prediction::{AsyncPredictionResponse, PredictionResult, ZeroShotPredictionRequest},
};
use async_trait::async_trait;
use reqwest::Client;

pub struct ChronosApiClient {
    client: Client,
    config: ApiClientConfig,
}

impl ChronosApiClient {
    /// 従来の互換性のためのコンストラクタ
    pub fn new(base_url: String) -> Self {
        Self::new_with_config(ApiClientConfig::new(base_url))
    }

    /// 設定を指定するコンストラクタ
    pub fn new_with_config(config: ApiClientConfig) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    /// 予測が完了するまでポーリング
    pub async fn poll_prediction_until_complete(
        &self,
        prediction_id: &str,
    ) -> Result<PredictionResult, ApiError> {
        let max_attempts = 60; // 最大60回試行 (約5分)
        let poll_interval = std::time::Duration::from_secs(5);

        for attempt in 1..=max_attempts {
            let result = self.get_prediction_status(prediction_id).await?;

            match result.status.as_str() {
                "completed" => {
                    println!("✅ Prediction completed successfully");
                    return Ok(result);
                }
                "failed" => {
                    let error_msg = result.error.unwrap_or_else(|| "Unknown error".to_string());
                    return Err(ApiError::Server(format!(
                        "Prediction failed: {}",
                        error_msg
                    )));
                }
                "running" | "pending" => {
                    if let Some(progress) = result.progress {
                        println!("⏳ Prediction in progress: {:.1}%", progress * 100.0);
                    } else {
                        println!(
                            "⏳ Prediction in progress... (attempt {}/{})",
                            attempt, max_attempts
                        );
                    }
                }
                status => {
                    println!("❓ Unknown status: {}", status);
                }
            }

            if attempt < max_attempts {
                tokio::time::sleep(poll_interval).await;
            }
        }

        Err(ApiError::Timeout(format!(
            "Prediction timed out after {} attempts",
            max_attempts
        )))
    }

    /// 予測ステータスを取得
    pub async fn get_prediction_status(
        &self,
        prediction_id: &str,
    ) -> Result<PredictionResult, ApiError> {
        let path = format!("/api/v1/prediction_status/{}", prediction_id);
        let response: ApiResponse<PredictionResult, String> = self
            .request(reqwest::Method::GET, &path, None::<()>)
            .await?;

        match response {
            ApiResponse::Success(result) => Ok(result),
            ApiResponse::Error(message) => Err(ApiError::Server(message)),
        }
    }

    /// 従来のAPIとの互換性のため
    pub async fn predict_zero_shot(
        &self,
        request: ZeroShotPredictionRequest,
    ) -> Result<AsyncPredictionResponse, ApiError> {
        self.predict(request).await
    }
}

#[async_trait]
impl ApiClient for ChronosApiClient {
    type Config = ApiClientConfig;

    fn new(config: Self::Config) -> Self {
        Self::new_with_config(config)
    }

    fn base_url(&self) -> &str {
        &self.config.base_url
    }

    async fn health_check(&self) -> Result<(), ApiError> {
        let response = self
            .client
            .get(format!("{}/health", self.base_url()))
            .timeout(self.config.timeout)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(ApiError::Server(format!(
                "Health check failed: {}",
                response.status()
            )))
        }
    }

    async fn request<T, R>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<T>,
    ) -> Result<ApiResponse<R, String>, ApiError>
    where
        T: serde::Serialize + Send,
        R: serde::de::DeserializeOwned + Send + std::fmt::Debug + Clone,
    {
        let url = format!("{}{}", self.base_url(), path);
        let mut request = self.client.request(method, &url);

        if let Some(body) = body {
            request = request.json(&body);
        }

        let response = request
            .timeout(self.config.timeout)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(ApiError::Server(format!(
                "HTTP Error {}: {}",
                status, error_text
            )));
        }

        let api_response: ApiResponse<R, String> = response
            .json()
            .await
            .map_err(|e| ApiError::Parse(e.to_string()))?;

        Ok(api_response)
    }
}

#[async_trait]
impl PredictionClient for ChronosApiClient {
    type PredictionRequest = ZeroShotPredictionRequest;
    type PredictionResponse = AsyncPredictionResponse;

    async fn predict(
        &self,
        request: Self::PredictionRequest,
    ) -> Result<Self::PredictionResponse, ApiError> {
        let response: ApiResponse<Self::PredictionResponse, String> = self
            .request(
                reqwest::Method::POST,
                "/api/v1/predict_zero_shot_async",
                Some(request),
            )
            .await?;

        match response {
            ApiResponse::Success(data) => Ok(data),
            ApiResponse::Error(message) => Err(ApiError::Server(message)),
        }
    }
}
