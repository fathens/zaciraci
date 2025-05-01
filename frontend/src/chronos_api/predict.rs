use anyhow::Result;
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use zaciraci_common::config;

fn chronos_base_url() -> String {
    config::get("CHRONOS_BASE_URL").unwrap_or_else(|_| "http://localhost:8000".to_string())
}

// ゼロショット予測リクエスト
#[derive(Debug, Serialize, Deserialize)]
pub struct ZeroShotPredictionRequest {
    pub timestamp: Vec<DateTime<Utc>>,
    pub values: Vec<f64>,
    pub forecast_until: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_params: Option<HashMap<String, serde_json::Value>>,
}

impl ZeroShotPredictionRequest {
    /// 新しいZeroShotPredictionRequestを作成する
    pub fn new(
        timestamp: Vec<DateTime<Utc>>,
        values: Vec<f64>,
        forecast_until: DateTime<Utc>,
    ) -> Self {
        Self {
            timestamp,
            values,
            forecast_until,
            model_name: None,
            model_params: None,
        }
    }

    /// モデル名を設定する
    pub fn with_model_name(mut self, model_name: impl Into<String>) -> Self {
        self.model_name = Some(model_name.into());
        self
    }

    /// モデルパラメータを設定する
    pub fn with_model_params(mut self, model_params: HashMap<String, serde_json::Value>) -> Self {
        self.model_params = Some(model_params);
        self
    }

    /// モデルパラメータに単一のキーと値を追加する
    pub fn add_model_param(mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
        let params = self.model_params.get_or_insert_with(HashMap::new);
        params.insert(key.into(), value.into());
        self
    }
}

// 予測レスポンス
#[derive(Debug, Serialize, Deserialize)]
pub struct PredictionResponse {
    pub forecast_timestamp: Vec<DateTime<Utc>>,
    pub forecast_values: Vec<f64>,
    pub model_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_intervals: Option<HashMap<String, Vec<f64>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<HashMap<String, f64>>,
}

pub struct ChronosApiClient {
    pub underlying: Arc<Underlying>,
}

impl ChronosApiClient {
    // ゼロショット予測APIを呼び出す
    pub async fn predict_zero_shot(&self, request: &ZeroShotPredictionRequest) -> Result<PredictionResponse> {
        self.underlying.post("api/v1/predict_zero_shot", request).await
    }
}

pub struct Underlying {
    base_url: String,
    client: reqwest::Client,
}

static CHRONOS_API_CLIENT: Lazy<Arc<ChronosApiClient>> = Lazy::new(|| {
    Arc::new(ChronosApiClient {
        underlying: Arc::new(Underlying {
            base_url: chronos_base_url(),
            client: reqwest::Client::new(),
        }),
    })
});

pub fn get_client() -> Arc<ChronosApiClient> {
    CHRONOS_API_CLIENT.clone()
}

impl Underlying {
    async fn get<T>(&self, path: &str) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let url = format!("{}/{}", self.base_url, path);
        Ok(self.client.get(&url).send().await?.json().await?)
    }

    async fn post<A, B>(&self, path: &str, body: &A) -> Result<B>
    where
        A: serde::Serialize,
        B: serde::de::DeserializeOwned,
    {
        let url = format!("{}/{}", self.base_url, path);
        Ok(self
            .client
            .post(&url)
            .json(body)
            .send()
            .await?
            .json()
            .await?)
    }
}