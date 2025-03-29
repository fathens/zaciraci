mod basic;
mod ollama;
mod pools;
mod storage;

use anyhow::Result;
use once_cell::sync::Lazy;
use std::sync::Arc;
use zaciraci_common::config;

fn server_base_url() -> String {
    config::get("SERVER_BASE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string())
}

pub struct ApiClient {
    pub basic: basic::BasicApi,
    pub pools: pools::PoolsApi,
    pub storage: storage::StorageApi,
    pub ollama: ollama::OllamaApi,
}

pub struct Underlying {
    base_url: String,
    client: reqwest::Client,
}

static API_CLIENT: Lazy<Arc<ApiClient>> = Lazy::new(|| Arc::new(new_client(server_base_url())));

pub fn get_client() -> Arc<ApiClient> {
    API_CLIENT.clone()
}

fn new_client(base_url: String) -> ApiClient {
    let underlying = Arc::new(Underlying {
        base_url,
        client: reqwest::Client::new(),
    });
    ApiClient {
        basic: basic::BasicApi {
            underlying: Arc::clone(&underlying),
        },
        pools: pools::PoolsApi {
            underlying: Arc::clone(&underlying),
        },
        storage: storage::StorageApi {
            underlying: Arc::clone(&underlying),
        },
        ollama: ollama::OllamaApi {
            underlying: Arc::clone(&underlying),
        },
    }
}

impl Underlying {
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
}
