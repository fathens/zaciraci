use super::{Message, ModelName};
use crate::Result;
use crate::logging::*;
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub model: ModelName,
    pub messages: Vec<Message>,
    pub stream: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub model: String,
    pub created_at: DateTime<FixedOffset>,
    pub message: Message,
    pub done_reason: String,
    pub done: bool,
    pub total_duration: u64,
    pub load_duration: u64,
    pub prompt_eval_count: u64,
    pub prompt_eval_duration: u64,
    pub eval_count: u64,
    pub eval_duration: u64,
}

pub async fn chat(
    client: &reqwest::Client,
    base_url: String,
    model: ModelName,
    messages: Vec<Message>,
) -> Result<Response> {
    let log = DEFAULT.new(o!("function" => "chat"));
    info!(log, "Chatting");
    let request = Request {
        model,
        messages,
        stream: false,
    };
    let url = base_url + "/chat";
    let response = client.post(&url).json(&request).send().await?;
    let response: Response = response.json().await?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_chat() {
        let client = reqwest::Client::new();
        let base_url = "http://localhost:11434/api".to_string();
        let model = ModelName("gemma3:12b".to_string());
        let messages = vec![Message {
            role: "user".to_string(),
            content: "Hello, world!".to_string(),
        }];
        let response = chat(&client, base_url.clone(), model, messages)
            .await
            .unwrap();
        println!("response = {response:#?}");
    }
}
