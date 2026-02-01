use crate::ApiResponse;
use crate::api::ApiError;
use async_trait::async_trait;

/// 統一されたAPIクライアントトレイト
#[async_trait]
pub trait ApiClient {
    type Config: Clone + Default;

    fn new(config: Self::Config) -> Self;
    fn base_url(&self) -> &str;

    /// ヘルスチェック
    async fn health_check(&self) -> Result<(), ApiError>;

    /// 共通のHTTPリクエスト処理
    async fn request<T, R>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<T>,
    ) -> Result<ApiResponse<R, String>, ApiError>
    where
        T: serde::Serialize + Send,
        R: serde::de::DeserializeOwned + Send + std::fmt::Debug + Clone;
}

/// データ取得APIクライアントの共通インターフェース
#[async_trait]
pub trait DataClient: ApiClient {
    type DataRequest: serde::Serialize + Send + std::fmt::Debug + Clone;
    type DataResponse: serde::de::DeserializeOwned + Send + std::fmt::Debug + Clone;

    async fn get_data(&self, request: Self::DataRequest) -> Result<Self::DataResponse, ApiError>;
}
