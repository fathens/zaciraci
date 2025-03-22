use super::ModelName;
use crate::logging::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, FixedOffset};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub model: ModelName,
    pub prompt: String,
    pub stream: bool,
    pub images: Vec<Image>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image(String);

#[allow(dead_code)]
impl Image {
    pub fn from_bytes(bytes: &[u8]) -> Image {
        Image(STANDARD.encode(bytes))
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        STANDARD.decode(&self.0).unwrap()
    }
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
    base_url: String,
    model: ModelName,
    prompt: String,
    images: Vec<Image>,
) -> crate::Result<Response> {
    let log = crate::logging::DEFAULT.new(o!("function" => "generate"));
    info!(log, "Generating");
    let request = Request {
        model,
        prompt,
        stream: false,
        images,
    };
    let url = base_url + "/generate";
    let response = client.post(&url).json(&request).send().await?;
    let response: Response = response.json().await?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest;

    #[tokio::test]
    #[ignore]
    async fn test_generate() {
        let log = crate::logging::DEFAULT.new(o!("function" => "test_generate"));
        info!(log, "Testing generate");
        let client = reqwest::Client::new();
        let base_url = "http://localhost:11434/api".to_string();
        let model = ModelName("gemma3:12b".to_string());
        let prompt = "say something".to_string();
        let images = vec![];
        let response = generate(&client, base_url, model, prompt, images).await.unwrap();
        println!("response = {response:#?}");
    }
}