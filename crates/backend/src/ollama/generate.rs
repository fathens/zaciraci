use super::{Image, ModelName};
use crate::logging::*;
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub model: ModelName,
    pub prompt: String,
    pub stream: bool,
    pub images: Vec<Image>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub model: ModelName,
    pub created_at: DateTime<FixedOffset>,
    pub response: String,
    pub done: bool,
    pub done_reason: String,
    pub context: Vec<u64>,
    pub total_duration: u64,
    pub load_duration: u64,
    pub prompt_eval_count: u64,
    pub prompt_eval_duration: u64,
    pub eval_count: u64,
    pub eval_duration: u64,
}

pub async fn generate(
    client: &reqwest::Client,
    base_url: &str,
    model: ModelName,
    prompt: String,
    images: Vec<Image>,
) -> crate::Result<Response> {
    let log = DEFAULT.new(o!("function" => "generate"));
    info!(log, "Generating");
    let request = Request {
        model,
        prompt,
        stream: false,
        images,
    };
    let url = format!("{}/generate", base_url);
    let response = client.post(&url).json(&request).send().await?;
    let response: Response = response.json().await?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_generate() {
        let log = DEFAULT.new(o!("function" => "test_generate"));
        info!(log, "Testing generate");
        let client = reqwest::Client::new();
        let base_url = "http://localhost:11434/api".to_string();
        let model = ModelName("gemma3:12b".to_string());
        let prompt = "say something".to_string();
        let images = vec![];
        let response = generate(&client, &base_url, model, prompt, images)
            .await
            .unwrap();
        debug!(log, "response = {response:#?}");
    }
}
