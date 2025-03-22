use crate::Result;
use crate::config;
use crate::logging::*;
use anyhow::bail;
use chrono::{DateTime, FixedOffset};
use reqwest;
use serde::{Deserialize, Serialize};

mod chat;
mod generate;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Models {
    pub models: Vec<Model>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub name: ModelName,
    pub model: String,
    #[serde(rename = "modified_at")]
    pub modified_at: DateTime<FixedOffset>,
    pub size: u64,
    pub digest: String,
    pub details: ModelDetails,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDetails {
    #[serde(rename = "parent_model")]
    pub parent_model: String,
    pub format: String,
    pub family: String,
    pub families: Vec<String>,
    #[serde(rename = "parameter_size")]
    pub parameter_size: String,
    #[serde(rename = "quantization_level")]
    pub quantization_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelName(String);

fn get_base_url() -> String {
    config::get("LLM_BASE_URL").unwrap_or_else(|_| "http://localhost:11434/api".to_string())
}

async fn get_model() -> Result<ModelName> {
    let log = DEFAULT.new(o!("function" => "get_model"));
    let name = match config::get("LLM_MODEL") {
        Ok(name) => ModelName(name),
        Err(err) => {
            info!(log, "LLM_MODEL not set, using default"; "error" => %err);
            let models = list_models().await?.models;
            if models.is_empty() {
                bail!("No models found");
            }
            models[0].name.clone()
        }
    };
    Ok(name)
}

async fn list_models() -> Result<Models> {
    let log = DEFAULT.new(o!("function" => "list_models"));
    info!(log, "Listing models");
    let url = get_base_url() + "/tags";
    let response = reqwest::get(&url).await?;
    let models: Models = response.json().await?;
    Ok(models)
}

pub struct LLMClient {
    model: ModelName,
    base_url: String,
    client: reqwest::Client,
}

#[allow(dead_code)]
impl LLMClient {
    pub fn new(model: ModelName, base_url: String) -> Result<LLMClient> {
        let client = reqwest::Client::new();
        Ok(LLMClient {
            model,
            base_url,
            client,
        })
    }

    pub async fn new_default() -> Result<LLMClient> {
        let model = get_model().await?;
        let base_url = get_base_url();
        LLMClient::new(model, base_url)
    }

    pub async fn chat(&self, messages: Vec<chat::Message>) -> Result<String> {
        let log = DEFAULT.new(o!("function" => "chat"));
        info!(log, "Chatting");
        let response = chat::chat(&self.client, self.base_url.clone(), self.model.clone(), messages).await?;
        Ok(response.message.content)
    }

    pub async fn generate(&self, prompt: String, images: Vec<generate::Image>) -> Result<String> {
        let log = DEFAULT.new(o!("function" => "generate"));
        info!(log, "Generating");
        let response = generate::generate(&self.client, self.base_url.clone(), self.model.clone(), prompt, images).await?;
        Ok(response.response)
    }
}
