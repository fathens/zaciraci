use anyhow::Result;
use once_cell::sync::Lazy;
use std::sync::Arc;
use zaciraci_common::{
    config,
    ollama::{ChatRequest, GenerateRequest},
};

fn server_base_url() -> String {
    config::get("SERVER_BASE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string())
}

pub struct ApiClient {
    base_url: String,
    client: reqwest::Client,
}

static API_CLIENT: Lazy<Arc<ApiClient>> = Lazy::new(|| Arc::new(new_client(server_base_url())));

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
    async fn get_text(&self, path: &str) -> String {
        let url = format!("{}/{}", self.base_url, path);
        match self.client.get(&url).send().await {
            Ok(res) => res.text().await.unwrap_or_else(|e| format!("Error: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }

    async fn get<T>(&self, path: &str) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let url = format!("{}/{}", self.base_url, path);
        match self.client.get(&url).send().await {
            Ok(res) => Ok(res.json().await?),
            Err(e) => Err(e.into()),
        }
    }

    #[allow(unused)]
    async fn post<A, B>(&self, path: &str, body: &A) -> Result<B>
    where
        A: serde::Serialize,
        B: serde::de::DeserializeOwned,
    {
        let url = format!("{}/{}", self.base_url, path);
        match self.client.post(&url).json(body).send().await {
            Ok(res) => Ok(res.json().await?),
            Err(e) => Err(e.into()),
        }
    }

    //// basic

    pub async fn healthcheck(&self) -> String {
        self.get_text("healthcheck").await
    }

    pub async fn native_token_balance(&self) -> String {
        self.get_text("native_token/balance").await
    }

    pub async fn native_token_transfer(&self, receiver: &str, amount: &str) -> String {
        self.get_text(&format!("native_token/transfer/{receiver}/{amount}"))
            .await
    }

    //// pools

    pub async fn get_all_pools(&self) -> String {
        self.get_text("pools/get_all").await
    }

    pub async fn estimate_return(&self, pool_id: &str, amount: &str) -> String {
        self.get_text(&format!("pools/estimate_return/{pool_id}/{amount}"))
            .await
    }

    pub async fn get_return(&self, pool_id: &str, amount: &str) -> String {
        self.get_text(&format!("pools/get_return/{pool_id}/{amount}"))
            .await
    }

    pub async fn list_all_tokens(&self) -> String {
        self.get_text("pools/list_all_tokens").await
    }

    pub async fn list_returns(&self, token_account: &str, amount: &str) -> String {
        self.get_text(&format!("pools/list_returns/{token_account}/{amount}"))
            .await
    }

    pub async fn pick_goals(&self, token_account: &str, amount: &str) -> String {
        self.get_text(&format!("pools/pick_goals/{token_account}/{amount}"))
            .await
    }

    pub async fn run_swap(
        &self,
        token_in_account: &str,
        initial_value: &str,
        token_out_account: &str,
    ) -> String {
        self.get_text(&format!(
            "pools/run_swap/{token_in_account}/{initial_value}/{token_out_account}"
        ))
        .await
    }

    //// storage

    pub async fn storage_deposit_min(&self) -> String {
        self.get_text("storage/deposit_min").await
    }

    pub async fn storage_deposit(&self, amount: &str) -> String {
        self.get_text(&format!("storage/deposit/{amount}")).await
    }

    pub async fn storage_unregister_token(&self, token_account: &str) -> String {
        self.get_text(&format!("storage/unregister/{token_account}"))
            .await
    }

    pub async fn amounts_list(&self) -> String {
        self.get_text("storage/amounts/list").await
    }

    pub async fn amounts_wrap(&self, amount: &str) -> String {
        self.get_text(&format!("storage/amounts/wrap/{amount}"))
            .await
    }

    pub async fn amounts_unwrap(&self, amount: &str) -> String {
        self.get_text(&format!("storage/amounts/unwrap/{amount}"))
            .await
    }

    pub async fn amounts_deposit(&self, token_account: &str, amount: &str) -> String {
        self.get_text(&format!("storage/amounts/deposit/{token_account}/{amount}"))
            .await
    }

    pub async fn amounts_withdraw(&self, token_account: &str, amount: &str) -> String {
        self.get_text(&format!(
            "storage/amounts/withdraw/{token_account}/{amount}"
        ))
        .await
    }

    // ollama

    pub async fn ollama_list_models(&self) -> Vec<String> {
        self.get("ollama/model_names").await.unwrap_or_default()
    }

    pub async fn ollama_chat(&self, request: &ChatRequest) -> String {
        self.post("ollama/chat", request).await.unwrap_or_default()
    }

    pub async fn ollama_generate(&self, request: &GenerateRequest) -> String {
        self.post("ollama/generate", request)
            .await
            .unwrap_or_default()
    }
}
