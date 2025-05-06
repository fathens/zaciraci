mod basic;
mod ollama;
mod pools;
mod stats;
mod storage;

use crate::api_underlying::Underlying;
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
    pub stats: stats::StatsApi,
}

static API_CLIENT: Lazy<Arc<ApiClient>> = Lazy::new(|| Arc::new(new_client(server_base_url())));

pub fn get_client() -> Arc<ApiClient> {
    API_CLIENT.clone()
}

fn new_client(base_url: String) -> ApiClient {
    let underlying = Underlying::new_shared(base_url);
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
        stats: stats::StatsApi {
            underlying: Arc::clone(&underlying),
        },
    }
}
