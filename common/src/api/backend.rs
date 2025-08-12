use super::traits::ApiClient;
use super::{ApiClientConfig, ApiError};
use crate::{
    ApiResponse,
    pools::{VolatilityTokensRequest, VolatilityTokensResponse},
    stats::{GetValuesRequest, GetValuesResponse, ValueAtTime},
    types::TokenAccount,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use reqwest::Client;

pub struct BackendClient {
    client: Client,
    config: ApiClientConfig,
}

impl BackendClient {
    /// 従来の互換性のためのコンストラクタ
    pub fn new() -> Self {
        Self::new_with_config(ApiClientConfig::default())
    }

    /// URLを指定するコンストラクタ（従来の互換性のため）
    pub fn new_with_url(base_url: String) -> Self {
        Self::new_with_config(ApiClientConfig::new(base_url))
    }

    /// 設定を指定するコンストラクタ
    pub fn new_with_config(config: ApiClientConfig) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }
}

impl Default for BackendClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ApiClient for BackendClient {
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

// 既存のメソッドを統一されたインターフェースで実装
impl BackendClient {
    pub async fn get_volatility_tokens(
        &self,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
        limit: u32,
        quote_token: Option<String>,
        min_depth: Option<u64>,
    ) -> Result<Vec<TokenAccount>> {
        let request = VolatilityTokensRequest {
            start: start_date.naive_utc(),
            end: end_date.naive_utc(),
            limit,
            quote_token,
            min_depth,
        };

        let response: ApiResponse<VolatilityTokensResponse, String> = self
            .request(
                reqwest::Method::POST,
                "/pools/get_volatility_tokens",
                Some(request),
            )
            .await
            .map_err(|e| anyhow::anyhow!("API request failed: {}", e))?;

        match response {
            ApiResponse::Success(data) => Ok(data.tokens),
            ApiResponse::Error(message) => Err(anyhow::anyhow!("API Error: {}", message)),
        }
    }

    pub async fn get_price_history(
        &self,
        quote_token: &str,
        base_token: &str,
        start_date: NaiveDateTime,
        end_date: NaiveDateTime,
    ) -> Result<Vec<ValueAtTime>> {
        let request = GetValuesRequest {
            quote_token: quote_token.parse()?,
            base_token: base_token.parse()?,
            start: start_date,
            end: end_date,
        };

        let response: ApiResponse<GetValuesResponse, String> = self
            .request(reqwest::Method::POST, "/stats/get_values", Some(request))
            .await
            .context("Failed to send request")?;

        match response {
            ApiResponse::Success(data) => Ok(data.values),
            ApiResponse::Error(message) => Err(anyhow::anyhow!("API Error: {}", message)),
        }
    }
}
