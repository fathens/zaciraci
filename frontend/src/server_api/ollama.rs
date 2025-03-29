use std::sync::Arc;
use zaciraci_common::ollama::{ChatRequest, GenerateRequest};

use crate::server_api::Underlying;

pub struct OllamaApi {
    pub underlying: Arc<Underlying>,
}
    
impl OllamaApi {
    pub async fn list_models(&self) -> Vec<String> {
        self.underlying.get("ollama/model_names").await.unwrap_or_default()
    }

    pub async fn chat(&self, request: &ChatRequest) -> String {
        self.underlying.post("ollama/chat", request).await.unwrap_or_default()
    }

    pub async fn generate(&self, request: &GenerateRequest) -> String {
        self.underlying.post("ollama/generate", request)
            .await
            .unwrap_or_default()
    }
}