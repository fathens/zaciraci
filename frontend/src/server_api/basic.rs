use crate::api_underlying::Underlying;
use std::sync::Arc;

pub struct BasicApi {
    pub underlying: Arc<Underlying>,
}

impl BasicApi {
    pub async fn healthcheck(&self) -> String {
        self.underlying.get_text("healthcheck").await
    }

    pub async fn native_token_balance(&self) -> String {
        self.underlying.get_text("native_token/balance").await
    }

    pub async fn native_token_transfer(&self, receiver: &str, amount: &str) -> String {
        self.underlying
            .get_text(&format!("native_token/transfer/{receiver}/{amount}"))
            .await
    }
}
