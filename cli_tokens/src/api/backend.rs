use anyhow::Result;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde_json::Value;
use zaciraci_common::{pools::VolatilityTokensRequest, types::TokenAccount, ApiResponse};

pub struct BackendApiClient {
    client: Client,
    base_url: String,
}

impl BackendApiClient {
    pub fn new(base_url: String) -> Self {
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
    ) -> Result<Vec<TokenAccount>> {
        let request = VolatilityTokensRequest {
            start: start_date.naive_utc(),
            end: end_date.naive_utc(),
            limit,
        };

        let url = format!("{}/api/volatility-tokens", self.base_url);
        let response = self.client.post(&url).json(&request).send().await?;

        let api_response: ApiResponse<Vec<TokenAccount>, String> = response.json().await?;

        match api_response {
            ApiResponse::Success(data) => Ok(data),
            ApiResponse::Error(message) => Err(anyhow::anyhow!("API Error: {}", message)),
        }
    }

    pub async fn get_token_history(
        &self,
        token: &str,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<Vec<(DateTime<Utc>, f64)>> {
        let url = format!("{}/api/token-history/{}", self.base_url, token);
        let response = self
            .client
            .get(&url)
            .query(&[
                ("start_date", start_date.to_rfc3339()),
                ("end_date", end_date.to_rfc3339()),
            ])
            .send()
            .await?;

        let _data: Value = response.json().await?;

        // TODO: Parse actual response format
        Ok(vec![])
    }
}
