use anyhow::Result;
use reqwest;
use std::sync::Arc;

/// HTTP APIリクエストの基盤となる構造体
pub struct Underlying {
    base_url: String,
    client: reqwest::Client,
}

impl Underlying {
    /// 新しいUnderlyingインスタンスを作成
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: reqwest::Client::new(),
        }
    }
    
    /// 新しいUnderlying共有インスタンスを作成
    pub fn new_shared(base_url: String) -> Arc<Self> {
        Arc::new(Self::new(base_url))
    }
    
    /// GETリクエストを送信してJSONレスポンスをデシリアライズ
    pub async fn get<T>(&self, path: &str) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let url = format!("{}/{}", self.base_url, path);
        Ok(self.client.get(&url).send().await?.json().await?)
    }

    /// プレーンテキストを取得するGETリクエスト
    pub async fn get_text(&self, path: &str) -> String {
        let url = format!("{}/{}", self.base_url, path);
        match self.client.get(&url).send().await {
            Ok(res) => res.text().await.unwrap_or_else(|e| format!("Error: {}", e)),
            Err(e) => format!("Error: {}", e),
        }
    }

    /// POSTリクエストを送信してJSONレスポンスをデシリアライズ
    pub async fn post<A, B>(&self, path: &str, body: &A) -> Result<B>
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
