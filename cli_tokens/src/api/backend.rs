use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use reqwest::Client;
use zaciraci_common::{
    pools::{VolatilityTokensRequest, VolatilityTokensResponse},
    stats::{GetValuesRequest, GetValuesResponse, ValueAtTime},
    types::TokenAccount,
    ApiResponse,
};

pub struct BackendClient {
    client: Client,
    base_url: String,
}

impl Default for BackendClient {
    fn default() -> Self {
        Self::new()
    }
}

impl BackendClient {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            base_url: "http://localhost:8080".to_string(),
        }
    }

    pub fn new_with_url(base_url: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
        }
    }

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

        let url = format!("{}/pools/get_volatility_tokens", self.base_url);
        let response = self.client.post(&url).json(&request).send().await?;

        let api_response: ApiResponse<VolatilityTokensResponse, String> = response.json().await?;

        match api_response {
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

        let url = format!("{}/stats/get_values", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context(format!("Failed to send request to {}", url))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error body".to_string());
            return Err(anyhow::anyhow!("HTTP Error {}: {}", status, error_text));
        }

        let api_response: ApiResponse<GetValuesResponse, String> = response
            .json()
            .await
            .context("Failed to parse JSON response")?;

        match api_response {
            ApiResponse::Success(data) => Ok(data.values),
            ApiResponse::Error(message) => Err(anyhow::anyhow!("API Error: {}", message)),
        }
    }
}
