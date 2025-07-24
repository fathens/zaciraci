pub mod backend;
pub mod chronos;
pub mod traits;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 統一されたAPIエラー型
#[derive(Debug, Clone, Deserialize, Serialize, Error)]
pub enum ApiError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("Authentication error: {0}")]
    Auth(String),
    #[error("Server error: {0}")]
    Server(String),
    #[error("Client error: {0}")]
    Client(String),
    #[error("Timeout error: {0}")]
    Timeout(String),
    #[error("Parse error: {0}")]
    Parse(String),
}

/// 統一されたAPIクライアント設定
#[derive(Debug, Clone)]
pub struct ApiClientConfig {
    pub base_url: String,
    pub timeout: std::time::Duration,
    pub retry_attempts: u32,
    pub api_key: Option<String>,
}

impl Default for ApiClientConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:8080".to_string(),
            timeout: std::time::Duration::from_secs(30),
            retry_attempts: 3,
            api_key: None,
        }
    }
}

impl ApiClientConfig {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            ..Default::default()
        }
    }
    
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }
    
    pub fn with_retry_attempts(mut self, retry_attempts: u32) -> Self {
        self.retry_attempts = retry_attempts;
        self
    }
    
    pub fn with_api_key(mut self, api_key: String) -> Self {
        self.api_key = Some(api_key);
        self
    }
}
