use crate::models::prediction::{
    AsyncPredictionResponse, PredictionResult, ZeroShotPredictionRequest,
};
use anyhow::Result;
use reqwest::Client;

pub struct ChronosApiClient {
    client: Client,
    base_url: String,
}

impl ChronosApiClient {
    pub fn new(base_url: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
        }
    }

    pub async fn predict_zero_shot(
        &self,
        request: ZeroShotPredictionRequest,
    ) -> Result<AsyncPredictionResponse> {
        let url = format!("{}/api/v1/predict_zero_shot_async", self.base_url);

        let response = self.client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            println!("API Error: {} - {}", status, error_text);
            return Err(anyhow::anyhow!(
                "Chronos API error: {} - {}",
                status,
                error_text
            ));
        }

        let prediction_response: AsyncPredictionResponse = response.json().await?;
        Ok(prediction_response)
    }

    pub async fn get_prediction_status(&self, prediction_id: &str) -> Result<PredictionResult> {
        let url = format!(
            "{}/api/v1/prediction_status/{}",
            self.base_url, prediction_id
        );

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to get prediction status: {}",
                response.status()
            ));
        }

        let prediction_response: PredictionResult = response.json().await?;
        Ok(prediction_response)
    }

    pub async fn poll_prediction_until_complete(
        &self,
        prediction_id: &str,
    ) -> Result<PredictionResult> {
        let mut poll_count = 0u32;
        loop {
            poll_count += 1;
            let response = self.get_prediction_status(prediction_id).await?;

            println!(
                "Poll attempt {}: Status = {}, Progress = {:?}",
                poll_count, response.status, response.progress
            );

            if let Some(message) = &response.message {
                println!("Message: {}", message);
            }

            match response.status.as_str() {
                "completed" => {
                    println!("Prediction completed successfully!");
                    return Ok(response);
                }
                "failed" => {
                    let error_msg = response.error.unwrap_or("Unknown error".to_string());
                    return Err(anyhow::anyhow!("Prediction failed: {}", error_msg));
                }
                "running" | "pending" => {
                    // Wait before next poll
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
                _ => {
                    return Err(anyhow::anyhow!(
                        "Unknown prediction status: {}",
                        response.status
                    ))
                }
            }
        }
    }
}
