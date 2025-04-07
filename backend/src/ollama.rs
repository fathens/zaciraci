mod chat;
mod generate;

use crate::Result;
use crate::config;
use crate::logging::*;
use anyhow::bail;
use chrono::{DateTime, FixedOffset};
use reqwest;
use serde::{Deserialize, Serialize};
use std::fmt::Display;

pub use zaciraci_common::ollama::Message;
pub use zaciraci_common::ollama::Image;

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

impl Display for ModelName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub fn get_base_url() -> String {
    config::get("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://localhost:11434/api".to_string())
}

async fn get_model(base_url: &str) -> Result<Model> {
    let log = DEFAULT.new(o!("function" => "get_model"));
    match config::get("OLLAMA_MODEL") {
        Ok(name) => find_model(base_url, name).await,
        Err(err) => {
            info!(log, "OLLAMA_MODEL not set, using default"; "error" => %err);
            let models = list_models(base_url).await?.models;
            if models.is_empty() {
                bail!("No models found");
            }
            Ok(models[0].clone())
        }
    }
}

pub async fn find_model(base_url: &str, name: String) -> Result<Model> {
    let log = DEFAULT.new(o!("function" => "find_model"));
    info!(log, "Finding model");
    let models = list_models(base_url).await?;
    for model in models.models {
        if model.name.0 == name {
            return Ok(model);
        }
    }
    bail!("Model not found");
}

pub async fn list_models(base_url: &str) -> Result<Models> {
    let log = DEFAULT.new(o!("function" => "list_models"));
    info!(log, "Listing models");
    let url = base_url.to_string() + "/tags";
    let response = reqwest::get(&url).await?;
    let models: Models = response.json().await?;
    Ok(models)
}

pub struct Client {
    model: ModelName,
    base_url: String,
    client: reqwest::Client,
}

#[allow(dead_code)]
impl Client {
    fn new(model: ModelName, base_url: String) -> Self {
        let client = reqwest::Client::new();
        Self {
            model,
            base_url,
            client,
        }
    }

    pub async fn new_by_name(name: String, base_url: String) -> Result<Self> {
        let model = find_model(&base_url, name).await?;
        Ok(Self::new(model.name, base_url))
    }

    pub async fn new_default() -> Result<Self> {
        let base_url = get_base_url();
        let model = get_model(&base_url).await?;
        Ok(Self::new(model.name, base_url))
    }

    pub async fn chat(&self, messages: Vec<Message>) -> Result<String> {
        let log = DEFAULT.new(o!("function" => "chat"));
        info!(log, "Chatting");
        let response = chat::chat(&self.client, self.base_url.clone(), self.model.clone(), messages).await?;
        Ok(response.message.content)
    }

    pub async fn generate(&self, prompt: String, images: Vec<Image>) -> Result<String> {
        let log = DEFAULT.new(o!("function" => "generate"));
        info!(log, "Generating");
        let response = generate::generate(&self.client, self.base_url.clone(), self.model.clone(), prompt, images).await?;
        Ok(response.response)
    }
}
