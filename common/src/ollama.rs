use serde::{Deserialize, Serialize};
use base64::{Engine as _, engine::general_purpose::STANDARD};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
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

#[derive(Serialize, Deserialize)]
pub struct ChatRequest {
    pub model_name: String,
    pub messages: Vec<Message>,
}

#[derive(Serialize, Deserialize)]
pub struct GenerateRequest {
    pub model_name: String,
    pub prompt: String,
    pub images: Vec<Image>,
}
