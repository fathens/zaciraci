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
        let max_attempts = 720; // 最大720回試行 (約1時間)
        let poll_interval = std::time::Duration::from_secs(5);

        for attempt in 1..=max_attempts {
            let result = self.get_prediction_status(prediction_id).await?;

            match result.status.as_str() {
                "completed" => {
                    // Removed verbose output: "✅ Prediction completed successfully"
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
                    // Removed verbose progress output
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
        let url = format!("{}{}", self.base_url(), path);

        let response = self
            .client
            .get(&url)
            .timeout(self.config.timeout)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|e| format!("<failed to read body: {e}>"));
            return Err(ApiError::Server(format!(
                "HTTP Error {}: {}",
                status, error_text
            )));
        }

        // Get the response body as text first for debugging
        let response_text = response
            .text()
            .await
            .map_err(|e| ApiError::Network(format!("Failed to get response text: {}", e)))?;

        // Log the response text for debugging (removed to reduce output noise)

        // Parse the response text directly as PredictionResult
        let result: PredictionResult = serde_json::from_str(&response_text).map_err(|e| {
            ApiError::Parse(format!(
                "Error decoding response body: {}. Response: {}",
                e, response_text
            ))
        })?;

        Ok(result)
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
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|e| format!("<failed to read body: {e}>"));
            return Err(ApiError::Server(format!(
                "HTTP Error {}: {}",
                status, error_text
            )));
        }

        // Get the response body as text first for debugging
        let response_text = response
            .text()
            .await
            .map_err(|e| ApiError::Network(format!("Failed to get response text: {}", e)))?;

        // Log the response text for debugging (removed to reduce output noise)

        // Try to parse the response text
        let api_response: ApiResponse<R, String> =
            serde_json::from_str(&response_text).map_err(|e| {
                ApiError::Parse(format!(
                    "Error decoding response body: {}. Response: {}",
                    e, response_text
                ))
            })?;

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
        // Create the request directly without using the generic request method
        let url = format!("{}/api/v1/predict_zero_shot_async", self.base_url());
        let response = self
            .client
            .post(&url)
            .json(&request)
            .timeout(self.config.timeout)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|e| format!("<failed to read body: {e}>"));
            return Err(ApiError::Server(format!(
                "HTTP Error {}: {}",
                status, error_text
            )));
        }

        // Get the response body as text first for debugging
        let response_text = response
            .text()
            .await
            .map_err(|e| ApiError::Network(format!("Failed to get response text: {}", e)))?;

        // Try to parse the response text directly to AsyncPredictionResponse
        let prediction_response: Self::PredictionResponse = serde_json::from_str(&response_text)
            .map_err(|e| {
                ApiError::Parse(format!(
                    "Error decoding response body: {}. Response: {}",
                    e, response_text
                ))
            })?;

        Ok(prediction_response)
    }
}
