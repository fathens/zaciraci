use once_cell::sync::Lazy;
use std::sync::Arc;
use zaciraci_common::config;

fn server_base_url() -> String {
    config::get("SERVER_BASE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string())
}

pub struct ApiClient {
    base_url: String,
    client: reqwest::Client,
}

static API_CLIENT: Lazy<Arc<ApiClient>> = Lazy::new(|| {
    Arc::new(new_client(server_base_url()))
});

pub fn get_client() -> Arc<ApiClient> {
    API_CLIENT.clone()
}

fn new_client(base_url: String) -> ApiClient {
    ApiClient {
        base_url,
        client: reqwest::Client::new(),
    }
}
    
impl ApiClient {
    async fn get(&self, path: &str) -> String {
        let url = format!("{}{}", self.base_url, path);
        match self.client.get(&url).send().await {
            Ok(res) => res.text().await.unwrap_or_else(|e| format!("Error: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }

    pub async fn healthcheck(&self) -> String {
        self.get("/healthcheck").await
    }

    pub async fn native_token_balance(&self) -> String {
        self.get("/native_token/balance").await
    }

    pub async fn native_token_transfer(&self, receiver: &str, amount: &str) -> String {
        self.get(&format!("/native_token/transfer/{receiver}/{amount}")).await
    }
}
