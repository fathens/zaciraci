use crate::models::prediction::{PredictionResponse, ZeroShotPredictionRequest};
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
    ) -> Result<PredictionResponse> {
        let url = format!("{}/predict/zero-shot", self.base_url);

        let response = self.client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Chronos API error: {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            ));
        }

        let prediction_response: PredictionResponse = response.json().await?;
        Ok(prediction_response)
    }

    pub async fn get_prediction_status(&self, prediction_id: &str) -> Result<PredictionResponse> {
        let url = format!("{}/predict/status/{}", self.base_url, prediction_id);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to get prediction status: {}",
                response.status()
            ));
        }

        let prediction_response: PredictionResponse = response.json().await?;
        Ok(prediction_response)
    }

    pub async fn poll_prediction_until_complete(
        &self,
        prediction_id: &str,
        max_retries: u32,
    ) -> Result<PredictionResponse> {
        for _i in 0..max_retries {
            let response = self.get_prediction_status(prediction_id).await?;

            match response.status.as_str() {
                "completed" => return Ok(response),
                "failed" => return Err(anyhow::anyhow!("Prediction failed")),
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

        Err(anyhow::anyhow!(
            "Prediction timed out after {} retries",
            max_retries
        ))
    }
}
