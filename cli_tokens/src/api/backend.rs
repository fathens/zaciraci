use anyhow::Result;
use chrono::{DateTime, Utc};
use reqwest::Client;
use zaciraci_common::{
    pools::{VolatilityTokensRequest, VolatilityTokensResponse},
    types::TokenAccount,
    ApiResponse,
};

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

        let url = format!("{}/pools/get_volatility_tokens", self.base_url);
        let response = self.client.post(&url).json(&request).send().await?;

        let api_response: ApiResponse<VolatilityTokensResponse, String> = response.json().await?;

        match api_response {
            ApiResponse::Success(data) => Ok(data.tokens),
            ApiResponse::Error(message) => Err(anyhow::anyhow!("API Error: {}", message)),
        }
    }

    pub async fn get_token_history(
        &self,
        token: &str,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<Vec<(DateTime<Utc>, f64)>> {
        // For testing purposes, skip actual API call and generate mock data directly
        println!("Generating mock token history for: {}", token);
        println!(
            "Token: {}, Start: {}, End: {}",
            token,
            start_date.format("%Y-%m-%d"),
            end_date.format("%Y-%m-%d")
        );

        // TODO: Parse actual response format
        // For testing purposes, return mock time series data
        let mut test_data = Vec::new();
        let mut current_time = start_date;
        let time_step = chrono::Duration::hours(2); // 2-hour intervals for more data points
        let mut price: f64 = 1.0;

        while current_time <= end_date {
            // Generate realistic price movement (random walk with drift)
            price += (rand::random::<f64>() - 0.5) * 0.1 + 0.001; // Small upward drift
            price = price.max(0.1); // Ensure price stays positive
            test_data.push((current_time, price));
            current_time += time_step;
        }

        // Ensure we have at least 180 data points for AutoGluon prediction
        if test_data.len() < 180 {
            let mut additional_time = end_date;
            while test_data.len() < 180 {
                additional_time += time_step;
                price += (rand::random::<f64>() - 0.5) * 0.1 + 0.001;
                price = price.max(0.1);
                test_data.push((additional_time, price));
            }
        }

        println!("Generated {} data points for prediction", test_data.len());

        Ok(test_data)
    }
}
